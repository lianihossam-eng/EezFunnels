import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { mkdtemp, mkdir, rm, access, readFile, writeFile } from "node:fs/promises";
import { createServer as createHttpServer } from "node:http";
import { createServer } from "node:net";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { setTimeout as delay } from "node:timers/promises";
import { chromium } from "@playwright/test";

const require = createRequire(import.meta.url);
const root = resolve(import.meta.dirname, "..");
const wrangler = join(root, "node_modules/.bin/wrangler");
const browserAssets = [
  "apps/dashboard/dist/assets/eezfunnel.js",
  "apps/dashboard/dist/assets/eezfunnel_bg.wasm",
  "apps/api/build/worker/shim.mjs",
];
const stateDir = await mkdtemp(join(tmpdir(), "eezfunnel-pr010-browser-"));
const logPath = join(stateDir, "wrangler.log");
const localConfigPath = join(root, `.wrangler-pr010-${process.pid}.toml`);
let worker;
let browser;
let probeServer;
let workerExit;
let workerExited = false;
let workerLog = "";
let releaseDelayedSave;
let pageDiagnostics = [];
let probeServerRequests = [];
const failureLogPath = join(tmpdir(), "eezfunnel-pr010-browser-failure.log");

async function assertBuildOutputs() {
  const missing = [];
  for (const file of browserAssets) {
    try {
      await access(join(root, file));
    } catch {
      missing.push(file);
    }
  }
  assert.deepEqual(
    missing,
    [],
    `Build the local Worker and dashboard first; missing: ${missing.join(", ")}`,
  );
}

async function unusedPort() {
  const server = createServer();
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  assert(address && typeof address === "object");
  await new Promise((resolveClose, reject) => {
    server.close((error) => (error ? reject(error) : resolveClose()));
  });
  return address.port;
}

function runWrangler(args) {
  const result = spawnSync(wrangler, args, {
    cwd: root,
    encoding: "utf8",
    env: wranglerEnv,
    stdio: ["ignore", "pipe", "pipe"],
  });
  assert.equal(
    result.status,
    0,
    `wrangler ${args.join(" ")} failed:\n${result.stdout}\n${result.stderr}`,
  );
  return result.stdout;
}

async function waitForHealth(baseURL) {
  const deadline = Date.now() + 90000;
  while (Date.now() < deadline) {
    if (workerExited) {
      const exit = await workerExit;
      throw new Error(`Wrangler exited early (${JSON.stringify(exit)}):\n${await readLog()}`);
    }
    try {
      const response = await fetch(`${baseURL}/health`);
      if (response.ok) return;
      await delay(250);
    } catch {
      await delay(250);
    }
  }
  throw new Error(`Wrangler did not become ready:\n${await readLog()}`);
}

async function readLog() {
  return workerLog || "No Wrangler log was written.";
}

async function apiCreate(baseURL, name) {
  const response = await fetch(`${baseURL}/api/funnels`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": `pr010-create-${crypto.randomUUID()}`,
    },
    body: JSON.stringify({ name, blueprint: "tripwire" }),
  });
  const body = await response.text();
  assert.equal(response.status, 201, body);
  return JSON.parse(body);
}

async function apiEditorState(baseURL, funnelId) {
  const response = await fetch(`${baseURL}/api/funnels/${funnelId}`);
  const body = await response.text();
  assert.equal(response.status, 200, body);
  return JSON.parse(body);
}

function duplicateSettingsSchemaVersion(payload) {
  const body = JSON.stringify(payload);
  const duplicate = body.replace('"schema_version":1', '"schema_version":999,"schema_version":1');
  assert.notEqual(duplicate, body, "Settings payload has a schema version to duplicate");
  return duplicate;
}

function duplicateTextSettingValue(payload) {
  const change = payload.settings.changes.find(({ id }) => id === "section.hero.title");
  assert(change?.value?.kind === "text", "Save payload contains the hero title text setting");
  const body = JSON.stringify(payload);
  const encodedValue = JSON.stringify(change.value);
  const duplicatedValue = `{"kind":"text","value":"discarded","value":${JSON.stringify(change.value.value)}}`;
  const duplicate = body.replace(encodedValue, duplicatedValue);
  assert.notEqual(duplicate, body, "Hero title value object is present to duplicate");
  return duplicate;
}

async function sectionOrder(page) {
  return page.locator("[data-editor-section]").evaluateAll((rows) =>
    rows.map((row) => row.getAttribute("data-editor-section")),
  );
}

async function responsivePreviewSnapshot(page, selectedLabel) {
  const frame = page.locator("iframe");
  const frameClientWidth = await frame.evaluate((element) => element.clientWidth);
  const rendered = await page.frameLocator("iframe").locator("html").evaluate((root) => {
    const features = root.querySelector(".features-grid");
    const funnel = root.querySelector("#funnel");
    const firstFeature = features?.querySelector(".feature");
    if (!features || !firstFeature || !funnel) return null;
    const columnWidths = getComputedStyle(features)
      .gridTemplateColumns.trim().split(/\s+/).map((width) => Number.parseFloat(width));
    return {
      browsingContextWidth: window.innerWidth,
      documentWidth: root.clientWidth,
      featureColumns: columnWidths.filter((width) => width > 0).length,
      featureGridWidth: Math.round(features.getBoundingClientRect().width),
      featureCardWidth: Math.round(firstFeature.getBoundingClientRect().width),
      funnelWidth: Math.round(funnel.getBoundingClientRect().width),
      sectionOrder: [...root.querySelectorAll("[data-section-id]")].map((section) =>
        section.getAttribute("data-section-id"),
      ),
    };
  });
  assert(rendered, "Rendered funnel should contain its layout root and features grid");
  const canvas = await page.locator(".editor-preview-canvas").evaluate((element) => ({
    clientWidth: element.clientWidth,
    scrollWidth: element.scrollWidth,
  }));
  const pageWidth = await page.evaluate(() => ({
    clientWidth: document.documentElement.clientWidth,
    scrollWidth: document.documentElement.scrollWidth,
  }));
  return {
    selectedLabel,
    frameClientWidth,
    ...rendered,
    canvas,
    pageWidth,
  };
}

async function waitForOrder(page, expected) {
  await page.waitForFunction(
    (order) => {
      const rows = [...document.querySelectorAll("[data-editor-section]")];
      return JSON.stringify(rows.map((row) => row.getAttribute("data-editor-section"))) === JSON.stringify(order);
    },
    expected,
    { timeout: 10000 },
  );
}

function editorPreviewResponse(page, funnelId) {
  return page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith(`/api/funnels/${funnelId}/preview`),
    { timeout: 10000 },
  );
}

async function clickWithPointer(page, locator) {
  const box = await locator.boundingBox();
  assert(box, "Navigation link must be visible for pointer click");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}

async function cleanup() {
  if (releaseDelayedSave) releaseDelayedSave();
  if (browser) await browser.close().catch(() => {});
  if (probeServer?.listening) {
    await new Promise((resolveClose) => probeServer.close(() => resolveClose()));
  }
  if (worker?.pid) {
    if (!workerExited) {
      try {
        process.kill(-worker.pid, "SIGTERM");
      } catch {}
      await Promise.race([workerExit, delay(5000)]);
    }
    if (!workerExited) {
      try {
        process.kill(-worker.pid, "SIGKILL");
      } catch {}
      await Promise.race([workerExit, delay(1000)]);
    }
  }
  await rm(stateDir, { recursive: true, force: true });
  await rm(localConfigPath, { force: true });
}

const port = await unusedPort();
const origin = `http://127.0.0.1:${port}`;
const wranglerEnv = {
  ...process.env,
  XDG_CONFIG_HOME: join(stateDir, "xdg-config"),
  XDG_CACHE_HOME: join(stateDir, "xdg-cache"),
  WRANGLER_SEND_METRICS: "false",
  NO_COLOR: "1",
};

try {
  await assertBuildOutputs();
  const rootConfig = await readFile(join(root, "wrangler.toml"), "utf8");
  const localConfig = rootConfig
    .replace(/^\[build\]\r?\ncommand = .+\r?\n/m, "")
    .replace('main = "apps/api/build/worker/shim.mjs"', 'main = "apps/api/build/index.js"');
  assert.notEqual(localConfig, rootConfig, "Expected to remove the custom build from local test config");
  await writeFile(localConfigPath, localConfig, "utf8");
  await mkdir(wranglerEnv.XDG_CONFIG_HOME, { recursive: true });
  await mkdir(wranglerEnv.XDG_CACHE_HOME, { recursive: true });
  await writeFile(logPath, "", "utf8");
  runWrangler([
    "d1",
    "migrations",
    "apply",
    "DB",
    "--local",
    "--env",
    "dev",
    "--persist-to",
    stateDir,
  ]);

  worker = spawn(
    wrangler,
    [
      "dev",
      "--local",
      "--no-bundle",
      "--env",
      "dev",
      "--config",
      localConfigPath,
      "--port",
      String(port),
      "--ip",
      "127.0.0.1",
      "--var",
      `ALLOWED_HOSTS:127.0.0.1:${port}`,
      "--persist-to",
      stateDir,
      "--show-interactive-dev-session=false",
    ],
    {
      cwd: root,
      detached: true,
      env: wranglerEnv,
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  worker.stdout.on("data", (chunk) => (workerLog += chunk.toString()));
  worker.stderr.on("data", (chunk) => (workerLog += chunk.toString()));
  workerExit = new Promise((resolveExit) =>
    worker.once("exit", (code, signal) => {
      workerExited = true;
      resolveExit({ code, signal });
    }),
  );
  await waitForHealth(origin);

  const primary = await apiCreate(origin, "PR-010 browser editor");
  const secondary = await apiCreate(origin, "PR-010 route race");
  const primaryId = primary.funnel.id;
  const secondaryId = secondary.funnel.id;
  probeServerRequests = [];
  probeServer = createHttpServer((request, response) => {
    probeServerRequests.push(request.url ?? "/");
    response.writeHead(204).end();
  });
  const probePort = await unusedPort();
  await new Promise((resolveListen, reject) => {
    probeServer.once("error", reject);
    probeServer.listen(probePort, "127.0.0.1", resolveListen);
  });
  const probeOrigin = `http://127.0.0.1:${probePort}`;

  const executablePath = await access("/usr/bin/chromium").then(
    () => "/usr/bin/chromium",
    () => undefined,
  );
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
  const page = await browser.newPage({ viewport: { width: 1360, height: 1000 } });
  pageDiagnostics = [];
  page.on("console", (message) => pageDiagnostics.push(`console ${message.type()}: ${message.text()}`));
  page.on("pageerror", (error) => pageDiagnostics.push(`pageerror: ${error.stack ?? error.message}`));
  page.on("requestfailed", (request) => pageDiagnostics.push(`requestfailed ${request.url()}: ${request.failure()?.errorText}`));
  page.on("crash", () => pageDiagnostics.push("page crashed"));
  page.on("request", (request) => {
    if (request.isNavigationRequest() || request.url().includes("/api/")) {
      pageDiagnostics.push(`request ${request.method()}: ${request.url()}`);
    }
  });
  page.on("framenavigated", (frame) => {
    if (frame === page.mainFrame()) pageDiagnostics.push(`framenavigated: ${frame.url()}`);
  });
  page.on("response", (response) => {
    if (response.request().isNavigationRequest() || response.url().includes("/api/")) {
      pageDiagnostics.push(`response ${response.status()}: ${response.url()}`);
    }
  });
  const probeRequests = [];
  const previewRequests = [];
  const saveRequests = [];
  page.on("request", (request) => {
    const url = request.url();
    if (url.includes("/__pr010_probe/")) probeRequests.push(url);
    if (url.includes(`/api/funnels/${primaryId}/preview`)) previewRequests.push(url);
    if (request.method() === "PUT" && url.includes(`/api/funnels/${primaryId}/revisions`)) {
      saveRequests.push(url);
    }
  });
  await page.goto(`${origin}/funnels`, { waitUntil: "networkidle" });
  const primaryLink = page.getByRole("link", { name: "PR-010 browser editor" });
  try {
    await primaryLink.waitFor({ timeout: 10000 });
  } catch {
    throw new Error(`Funnel card missing at ${page.url()}; document=${(await page.locator("body").innerText()).slice(0, 2000)}; diagnostics=${pageDiagnostics.join("\n")}`);
  }
  try {
    await clickWithPointer(page, primaryLink);
  } catch (error) {
    throw new Error(`Card navigation click failed at ${page.url()} (closed=${page.isClosed()}); diagnostics=${pageDiagnostics.join("\n")}; cause=${error}`);
  }
  await page.getByRole("heading", { name: "PR-010 browser editor" }).waitFor();
  await page.getByRole("list", { name: "Funnel sections" }).waitFor();

  const rows = await sectionOrder(page);
  assert.deepEqual(rows, ["hero", "features", "cta"]);
  const frame = page.locator("iframe");
  await frame.waitFor();
  assert.equal(await frame.getAttribute("sandbox"), "");
  const frameDocument = page.frameLocator("iframe");
  await frameDocument
    .locator('meta[http-equiv="Content-Security-Policy"]')
    .waitFor({ state: "attached" });
  const previewContent = await frameDocument.locator("body").innerText();
  assert.match(previewContent, /PR-010 browser editor/);
  assert.equal(await frameDocument.locator("a, img, script").count(), 0);
  const csp = await frameDocument
    .locator('meta[http-equiv="Content-Security-Policy"]')
    .getAttribute("content");
  assert.match(csp ?? "", /default-src 'none'/);
  assert.match(csp ?? "", /connect-src 'none'/);

  await page.addScriptTag({ content: require("axe-core").source });
  const axe = await page.evaluate(async () => {
    const result = await window.axe.run(document, {
      runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"] },
    });
    return result.violations.map(({ id, impact, help, nodes }) => ({ id, impact, help, nodes: nodes.length }));
  });
  const criticalAxeFindings = axe.filter((finding) => finding.impact === "critical");
  assert.deepEqual(criticalAxeFindings, [], `Critical axe findings: ${JSON.stringify(criticalAxeFindings)}`);

  {
    const viewportGroup = page.getByRole("group", { name: "Preview viewport" });
    const viewportSnapshots = [
      { label: "Mobile (375 px)", width: 375, columns: 1, featureGridWidth: 327, featureCardWidth: 327 },
      { label: "Tablet (768 px)", width: 768, columns: 2, featureGridWidth: 720, featureCardWidth: 352 },
      { label: "Desktop (1280 px)", width: 1280, columns: 2, featureGridWidth: 1104, featureCardWidth: 544 },
    ];
    const frame = page.locator("iframe");
    const originalSrcdoc = await frame.getAttribute("srcdoc");
    const originalTitle = await frame.getAttribute("title");
    const originalSandbox = await frame.getAttribute("sandbox");
    const previewCountBefore = previewRequests.length;
    const saveCountBefore = saveRequests.length;
    const snapshots = [];

    assert.equal(await viewportGroup.getByRole("button", { name: "Desktop (1280 px)" }).getAttribute("aria-pressed"), "true");
    for (const expected of viewportSnapshots) {
      const option = viewportGroup.getByRole("button", { name: expected.label });
      await option.focus();
      await page.keyboard.press("Enter");
      await page.waitForFunction(
        ({ label }) =>
          document.querySelector('[aria-label="Preview viewport"]')?.querySelector('[aria-pressed="true"]')?.textContent?.trim() === label,
        { label: expected.label },
      );
      const snapshot = await responsivePreviewSnapshot(page, expected.label);
      assert.equal(snapshot.frameClientWidth, expected.width);
      assert.equal(snapshot.browsingContextWidth, expected.width);
      assert.equal(snapshot.documentWidth, expected.width);
      assert.equal(snapshot.featureColumns, expected.columns, JSON.stringify(snapshot));
      assert.equal(snapshot.featureGridWidth, expected.featureGridWidth, JSON.stringify(snapshot));
      assert.equal(snapshot.featureCardWidth, expected.featureCardWidth, JSON.stringify(snapshot));
      assert.equal(snapshot.funnelWidth, expected.width);
      assert.deepEqual(snapshot.sectionOrder, ["hero", "features", "cta"]);
      assert.equal(snapshot.pageWidth.scrollWidth, snapshot.pageWidth.clientWidth);
      assert(snapshot.canvas.scrollWidth >= expected.width);
      snapshots.push(snapshot);
    }
    assert.equal(await frame.getAttribute("srcdoc"), originalSrcdoc);
    assert.equal(await frame.getAttribute("title"), originalTitle);
    assert.equal(await frame.getAttribute("sandbox"), originalSandbox);
    assert.equal(originalSandbox, "");
    assert.equal(previewRequests.length, previewCountBefore);
    assert.equal(saveRequests.length, saveCountBefore);
    assert.equal(await page.getByRole("button", { name: "Undo" }).isDisabled(), true);
    assert.equal(await page.getByRole("button", { name: "Redo" }).isDisabled(), true);
    assert.equal(await page.getByRole("button", { name: "Save revision" }).isDisabled(), true);
    assert.equal(await page.getByText("All changes saved").count(), 1);

    await page.setViewportSize({ width: 375, height: 1000 });
    const mobileCanvas = await page.locator(".editor-preview-canvas").evaluate((element) => ({
      clientWidth: element.clientWidth,
      scrollWidth: element.scrollWidth,
    }));
    const mobilePageWidth = await page.evaluate(() => ({
      clientWidth: document.documentElement.clientWidth,
      scrollWidth: document.documentElement.scrollWidth,
    }));
    assert(mobileCanvas.scrollWidth >= 1280);
    assert(mobileCanvas.scrollWidth > mobileCanvas.clientWidth);
    assert.equal(mobilePageWidth.scrollWidth, mobilePageWidth.clientWidth);
    assert.equal(await page.frameLocator("iframe").locator("html").evaluate(() => window.innerWidth), 1280);
    await page.setViewportSize({ width: 1360, height: 1000 });
    assert.equal(previewRequests.length, previewCountBefore);
    assert.equal(saveRequests.length, saveCountBefore);
    console.log(`PR-013 responsive layout snapshots: ${JSON.stringify(snapshots)}`);
  }

  await clickWithPointer(page, page.locator('[data-editor-section="hero"]'));
  {
    const historyTitle = page.locator('[id="setting-section.hero.title"]');
    const savedTitle = await historyTitle.inputValue();
    const undoButton = page.getByRole("button", { name: "Undo" });
    const redoButton = page.getByRole("button", { name: "Redo" });
    const historySaveButton = page.getByRole("button", { name: "Save revision" });
    const tabletViewport = page
      .getByRole("group", { name: "Preview viewport" })
      .getByRole("button", { name: "Tablet (768 px)" });

    const previewCountBeforeSelection = previewRequests.length;
    await tabletViewport.focus();
    await page.keyboard.press("Enter");
    assert.equal(previewRequests.length, previewCountBeforeSelection);
    assert.equal(await tabletViewport.getAttribute("aria-pressed"), "true");
    assert.equal(await undoButton.isDisabled(), true);

    let historyPreview = editorPreviewResponse(page, primaryId);
    await historyTitle.fill("PR-012 undo shortcut headline");
    assert.equal((await historyPreview).status(), 200);
    assert.equal(await page.locator("iframe").getAttribute("width"), "768");
    assert.equal(await page.frameLocator("iframe").locator("html").evaluate(() => window.innerWidth), 768);
    assert.equal(await undoButton.isDisabled(), false);
    assert.equal(await redoButton.isDisabled(), true);

    historyPreview = editorPreviewResponse(page, primaryId);
    await page.keyboard.press("Control+z");
    assert.equal((await historyPreview).status(), 200);
    await page.waitForFunction(
      ({ id, value }) => document.getElementById(id)?.value === value,
      { id: "setting-section.hero.title", value: savedTitle },
    );
    assert.equal(await undoButton.isDisabled(), true);
    assert.equal(await redoButton.isDisabled(), false);
    assert.equal(await historySaveButton.isDisabled(), true);

    historyPreview = editorPreviewResponse(page, primaryId);
    await page.keyboard.press("Control+y");
    assert.equal((await historyPreview).status(), 200);
    await page.waitForFunction(
      ({ id, value }) => document.getElementById(id)?.value === value,
      { id: "setting-section.hero.title", value: "PR-012 undo shortcut headline" },
    );
    assert.equal(await undoButton.isDisabled(), false);
    assert.equal(await redoButton.isDisabled(), true);

    historyPreview = editorPreviewResponse(page, primaryId);
    await page.keyboard.press("Control+z");
    assert.equal((await historyPreview).status(), 200);
    await page.waitForFunction(
      ({ id, value }) => document.getElementById(id)?.value === value,
      { id: "setting-section.hero.title", value: savedTitle },
    );
    assert.equal(await historySaveButton.isDisabled(), true);
  }

  const heroRow = page.locator('[data-editor-section="hero"]');
  await heroRow.focus();
  await page.keyboard.press("Space");
  await page.keyboard.press("ArrowDown");
  await waitForOrder(page, ["features", "hero", "cta"]);
  assert.equal(await page.locator('[aria-live="polite"]').innerText(), "Moved Hero to position 2 of 3.");
  await page.keyboard.press("Space");
  assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("data-editor-section")), "hero");
  assert.equal(await page.locator('[aria-live="polite"]').innerText(), "Dropped Hero at position 2 of 3.");

  let orderHistoryPreview = editorPreviewResponse(page, primaryId);
  await page.keyboard.press("Control+z");
  assert.equal((await orderHistoryPreview).status(), 200);
  await waitForOrder(page, ["hero", "features", "cta"]);
  assert.equal(await page.locator('[aria-live="polite"]').innerText(), "Undid moving Hero to position 1 of 3.");
  assert.equal(await page.getByRole("button", { name: "Redo" }).isDisabled(), false);
  orderHistoryPreview = editorPreviewResponse(page, primaryId);
  await page.getByRole("button", { name: "Redo" }).click();
  assert.equal((await orderHistoryPreview).status(), 200);
  await waitForOrder(page, ["features", "hero", "cta"]);
  assert.equal(await page.locator('[aria-live="polite"]').innerText(), "Redid moving Hero to position 2 of 3.");

  await heroRow.focus();
  await page.keyboard.press("Space");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Escape");
  await waitForOrder(page, ["features", "hero", "cta"]);
  assert.match(await page.locator('[aria-live="polite"]').innerText(), /original order was restored/);

  {
    const historyTitle = page.locator('[id="setting-section.hero.title"]');
    const savedTitle = await historyTitle.inputValue();
    let historyPreview = editorPreviewResponse(page, primaryId);
    await historyTitle.fill("PR-012 consecutive edit first");
    assert.equal((await historyPreview).status(), 200);
    historyPreview = editorPreviewResponse(page, primaryId);
    await historyTitle.fill("PR-012 consecutive edit second");
    assert.equal((await historyPreview).status(), 200);

    await page.evaluate(() => {
      const region = document.querySelector('[aria-live="polite"]');
      if (!region) throw new Error("Editor history live region is missing");
      window.__historyLiveAnnouncements = [];
      window.__historyLiveObserver = new MutationObserver(() => {
        const text = region.textContent?.trim();
        if (text) window.__historyLiveAnnouncements.push(text);
      });
      window.__historyLiveObserver.observe(region, {
        subtree: true,
        childList: true,
        characterData: true,
      });
    });

    const transition = async (shortcut, expectedValue, expectedCount) => {
      historyPreview = editorPreviewResponse(page, primaryId);
      await page.keyboard.press(shortcut);
      assert.equal((await historyPreview).status(), 200);
      await page.waitForFunction(
        ({ id, value }) => document.getElementById(id)?.value === value,
        { id: "setting-section.hero.title", value: expectedValue },
      );
      assert.equal(
        await page.evaluate(() => window.__historyLiveAnnouncements.length),
        expectedCount,
        `${shortcut} should produce a new live-region mutation`,
      );
    };

    await transition("Control+z", "PR-012 consecutive edit first", 1);
    await transition("Control+z", savedTitle, 2);
    await transition("Control+y", "PR-012 consecutive edit first", 3);
    await transition("Control+y", "PR-012 consecutive edit second", 4);
    await transition("Control+z", "PR-012 consecutive edit first", 5);
    await transition("Control+z", savedTitle, 6);
    assert.deepEqual(
      await page.evaluate(() => window.__historyLiveAnnouncements),
      [
        "Undid change to Headline.",
        "Undid change to Headline.",
        "Redid change to Headline.",
        "Redid change to Headline.",
        "Undid change to Headline.",
        "Undid change to Headline.",
      ],
    );
    await page.evaluate(() => window.__historyLiveObserver.disconnect());
    assert.equal(await historyTitle.inputValue(), savedTitle);
  }

  {
    const boundaryTitle = page.locator('[id="setting-section.hero.title"]');
    const savedTitle = await boundaryTitle.inputValue();
    const finalHistoryValue = "PR-012 boundary edit 100";
    await page.evaluate((id) => {
      window.__historyBoundaryKeys = [];
      window.__historyBoundaryInputEvents = 0;
      document.addEventListener("keydown", (event) => {
        if (event.ctrlKey && !event.altKey && !event.metaKey && (event.key === "z" || event.key === "y")) {
          window.__historyBoundaryKeys.push({ key: event.key, defaultPrevented: event.defaultPrevented });
        }
      });
      document.addEventListener("input", (event) => {
        if (event.target?.id === id) window.__historyBoundaryInputEvents += 1;
      }, true);
    }, "setting-section.hero.title");

    for (let index = 1; index <= 100; index += 1) {
      const preview = editorPreviewResponse(page, primaryId);
      await boundaryTitle.fill(`PR-012 boundary edit ${index}`);
      assert.equal((await preview).status(), 200);
    }
    assert.equal(await boundaryTitle.inputValue(), finalHistoryValue);
    assert.equal(await page.getByRole("button", { name: "Redo" }).isDisabled(), true);

    const assertBlockedAtBoundary = async (shortcut, expectedValue) => {
      const inputEventsBefore = await page.evaluate(() => window.__historyBoundaryInputEvents);
      await page.keyboard.press(shortcut);
      assert.equal(await boundaryTitle.inputValue(), expectedValue, `${shortcut} changed the editor input at an empty history direction`);
      assert.equal(await page.evaluate(() => window.__historyBoundaryKeys.at(-1)?.defaultPrevented), true);
      assert.equal(await page.evaluate(() => window.__historyBoundaryInputEvents), inputEventsBefore);
    };

    await assertBlockedAtBoundary("Control+y", finalHistoryValue);
    let historyPreview = editorPreviewResponse(page, primaryId);
    await page.keyboard.press("Control+z");
    assert.equal((await historyPreview).status(), 200);
    await page.waitForFunction(
      ({ id, value }) => document.getElementById(id)?.value === value,
      { id: "setting-section.hero.title", value: "PR-012 boundary edit 99" },
    );
    historyPreview = editorPreviewResponse(page, primaryId);
    await page.keyboard.press("Control+y");
    assert.equal((await historyPreview).status(), 200);
    await page.waitForFunction(
      ({ id, value }) => document.getElementById(id)?.value === value,
      { id: "setting-section.hero.title", value: finalHistoryValue },
    );

    for (let index = 100; index >= 1; index -= 1) {
      const preview = editorPreviewResponse(page, primaryId);
      await page.keyboard.press("Control+z");
      assert.equal((await preview).status(), 200);
      const expectedValue = index === 1 ? savedTitle : `PR-012 boundary edit ${index - 1}`;
      await page.waitForFunction(
        ({ id, value }) => document.getElementById(id)?.value === value,
        { id: "setting-section.hero.title", value: expectedValue },
      );
    }
    assert.equal(await page.getByRole("button", { name: "Undo" }).isDisabled(), true);
    await assertBlockedAtBoundary("Control+z", savedTitle);

    historyPreview = editorPreviewResponse(page, primaryId);
    await page.keyboard.press("Control+y");
    assert.equal((await historyPreview).status(), 200);
    await page.waitForFunction(
      ({ id, value }) => document.getElementById(id)?.value === value,
      { id: "setting-section.hero.title", value: "PR-012 boundary edit 1" },
    );
    historyPreview = editorPreviewResponse(page, primaryId);
    await page.keyboard.press("Control+z");
    assert.equal((await historyPreview).status(), 200);
    await page.waitForFunction(
      ({ id, value }) => document.getElementById(id)?.value === value,
      { id: "setting-section.hero.title", value: savedTitle },
    );
    assert.equal(await boundaryTitle.inputValue(), savedTitle);
  }

  const editorSaveButton = page.getByRole("button", { name: "Save revision" });
  await page.locator('[data-editor-section="hero"]').focus();
  await page.keyboard.press("Space");
  await page.keyboard.press("ArrowDown");
  await waitForOrder(page, ["features", "cta", "hero"]);
  assert.equal(await editorSaveButton.isDisabled(), true, "Save must wait for an active keyboard reorder to finish");
  await page.keyboard.press("Escape");
  await waitForOrder(page, ["features", "hero", "cta"]);

  const dragHandle = page.locator(".editor-section-list [data-sort-handle]");
  const nativeGripCount = await page.locator('[data-sort-handle]').count();
  assert.equal(nativeGripCount, 3, "Each section must expose a DnD pointer handle");
  const heroPosition = (await sectionOrder(page)).indexOf("hero");
  assert(heroPosition >= 0, "Hero remains in the section order after keyboard reordering");
  const heroDragHandle = dragHandle.nth(heroPosition);
  const targetRow = page.locator('[data-editor-section="cta"]');
  const handleBox = await heroDragHandle.boundingBox();
  const targetBox = await targetRow.boundingBox();
  assert(handleBox && targetBox, "Pointer drag source and target must be visible");
  await page.mouse.move(handleBox.x + handleBox.width / 2, handleBox.y + handleBox.height / 2);
  await page.mouse.down();
  await page.mouse.move(targetBox.x + targetBox.width / 2, targetBox.y + targetBox.height - 5, { steps: 14 });
  await page.mouse.up();
  await page.locator(".editor-unsaved-badge").waitFor({ timeout: 10000 });
  const pointerOrder = await sectionOrder(page);
  assert.notDeepEqual(pointerOrder, ["features", "hero", "cta"], "Pointer drag must reorder a section");
  await page.waitForFunction(() => {
    const frame = document.querySelector("iframe");
    return frame?.getAttribute("srcdoc")?.includes("Content-Security-Policy");
  });
  assert(previewRequests.length >= 1, "Reorder must call the editor preview API");

  await clickWithPointer(page, page.locator('[data-editor-section="hero"]'));
  const setting = (id) => page.locator(`[id="${id}"]`);
  let nextPreview = editorPreviewResponse(page, primaryId);
  await setting("setting-section.hero.title").fill("PR-011 inspector headline");
  let settingPreview = await nextPreview;
  assert.equal(settingPreview.status(), 200, await settingPreview.text());
  await frameDocument.getByRole("heading", { name: "PR-011 inspector headline" }).waitFor();

  nextPreview = editorPreviewResponse(page, primaryId);
  await setting("setting-theme.preset").selectOption("bold");
  settingPreview = await nextPreview;
  assert.equal(settingPreview.status(), 200, await settingPreview.text());
  assert.equal(await setting("setting-theme.primary_color").inputValue(), "#f97316");
  await page.waitForFunction(() =>
    document.querySelector("iframe")?.getAttribute("srcdoc")?.includes("--color-background:#111113"),
  );

  nextPreview = editorPreviewResponse(page, primaryId);
  await setting("setting-theme.primary_color").fill("#334155");
  settingPreview = await nextPreview;
  assert.equal(settingPreview.status(), 200, await settingPreview.text());
  await page.waitForFunction(() =>
    document.querySelector("iframe")?.getAttribute("srcdoc")?.includes("--color-primary:#334155"),
  );

  nextPreview = editorPreviewResponse(page, primaryId);
  await page.getByRole("button", { name: "Reset to preset" }).click();
  settingPreview = await nextPreview;
  assert.equal(settingPreview.status(), 200, await settingPreview.text());
  assert.equal(await setting("setting-theme.primary_color").inputValue(), "#f97316");

  nextPreview = editorPreviewResponse(page, primaryId);
  await setting("setting-theme.primary_color").fill("#123456");
  settingPreview = await nextPreview;
  assert.equal(settingPreview.status(), 200, await settingPreview.text());
  await page.waitForFunction(() =>
    document.querySelector("iframe")?.getAttribute("srcdoc")?.includes("--color-primary:#123456"),
  );

  nextPreview = editorPreviewResponse(page, primaryId);
  await setting("setting-theme.square_buttons").check();
  settingPreview = await nextPreview;
  assert.equal(settingPreview.status(), 200, await settingPreview.text());
  await page.waitForFunction(() =>
    document.querySelector("iframe")?.getAttribute("srcdoc")?.includes("--radius-button:0"),
  );

  nextPreview = editorPreviewResponse(page, primaryId);
  await setting("setting-section.hero.image-url").fill("https://example.test/pr011-image.png");
  settingPreview = await nextPreview;
  assert.equal(settingPreview.status(), 200, await settingPreview.text());
  nextPreview = editorPreviewResponse(page, primaryId);
  await setting("setting-section.hero.image-alt").fill("PR-011 preview image");
  settingPreview = await nextPreview;
  assert.equal(settingPreview.status(), 200, await settingPreview.text());
  await frameDocument.getByText("PR-011 preview image", { exact: true }).waitFor();
  await frameDocument.getByText("https://example.test/pr011-image.png", { exact: true }).waitFor();
  assert.equal(await frameDocument.locator("img").count(), 0, "Image edits stay inert in preview");

  await clickWithPointer(page, page.locator('[data-editor-section="cta"]'));
  const ctaDestination = setting("setting-section.cta.cta_url");
  nextPreview = editorPreviewResponse(page, primaryId);
  await ctaDestination.fill("javascript:alert(1)");
  settingPreview = await nextPreview;
  assert.equal(settingPreview.status(), 400, await settingPreview.text());
  const rejectedSaveRequest = page.waitForRequest(
    (request) => request.method() === "PUT" && request.url().endsWith(`/api/funnels/${primaryId}/revisions`),
  );
  const rejectedSaveResponse = page.waitForResponse(
    (response) => response.request().method() === "PUT" && response.url().endsWith(`/api/funnels/${primaryId}/revisions`),
  );
  await page.getByRole("button", { name: "Save revision" }).click();
  const rejectedSave = await rejectedSaveResponse;
  const rejectedSaveText = await rejectedSave.text();
  const rejectedSaveHeaders = (await rejectedSaveRequest).headers();
  assert.equal(rejectedSave.status(), 400, rejectedSaveText);
  assert(rejectedSaveHeaders["idempotency-key"], "Rejected validation save carries its own key");
  assert.equal(await ctaDestination.isDisabled(), false, "A definitive validation error unlocks the editor");

  nextPreview = editorPreviewResponse(page, primaryId);
  await ctaDestination.fill("https://example.test/repaired");
  settingPreview = await nextPreview;
  assert.equal(settingPreview.status(), 200, await settingPreview.text());

  const fixture = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src 'none'; connect-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"></head><body><script>document.body.dataset.scriptRan="yes";fetch("${probeOrigin}/__pr010_probe/script").catch(()=>{});</script><img src="${probeOrigin}/__pr010_probe/image"><a href="${probeOrigin}/__pr010_probe/navigation">inert link</a></body></html>`;
  await frame.evaluate((iframe, srcdoc) => {
    iframe.setAttribute("srcdoc", srcdoc);
  }, fixture);
  const probeFrame = page.frameLocator("iframe");
  await probeFrame.locator("body").waitFor();
  await delay(500);
  assert.equal(await probeFrame.locator("body").getAttribute("data-script-ran"), null);
  assert.deepEqual(
    probeServerRequests,
    [],
    `Sandbox preview reached the local network probe: ${probeServerRequests.join(", ")}`,
  );

  const saveRequestPromise = page.waitForRequest(
    (request) => request.method() === "PUT" && request.url().endsWith(`/api/funnels/${primaryId}/revisions`),
  );
  const saveResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "PUT" && response.url().endsWith(`/api/funnels/${primaryId}/revisions`),
  );
  await page.getByRole("button", { name: "Save revision" }).click();
  const firstSaveRequest = await saveRequestPromise;
  const firstSaveResponse = await saveResponsePromise;
  const firstSaveText = await firstSaveResponse.text();
  assert.equal(firstSaveResponse.status(), 200, firstSaveText);
  const firstSavePayload = JSON.parse(firstSaveRequest.postData() ?? "{}");
  const firstSaveKey = firstSaveRequest.headers()["idempotency-key"];
  assert(firstSaveKey, "Save request must carry an idempotency key");
  assert.notEqual(firstSaveKey, rejectedSaveHeaders["idempotency-key"], "Corrected save uses a fresh idempotency key");
  const firstSaveResult = JSON.parse(firstSaveText);
  assert.equal(firstSavePayload.settings?.schema_version, 1);
  assert.deepEqual(
    [...new Set(firstSavePayload.settings.changes.map(({ value }) => value.kind))].sort(),
    ["color", "image", "select", "text", "toggle"],
    "Save must persist all five planned setting control types",
  );
  await page.getByText("All changes saved").waitFor();
  const replay = await fetch(`${origin}/api/funnels/${primaryId}/revisions`, {
    method: "PUT",
    headers: { "Content-Type": "application/json", "Idempotency-Key": firstSaveKey },
    body: JSON.stringify(firstSavePayload),
  });
  const replayText = await replay.text();
  assert.equal(replay.status, 200, replayText);
  assert.equal(JSON.parse(replayText).revision_id, firstSaveResult.revision_id);

  const duplicateBaseRevisionId = (await apiEditorState(origin, primaryId)).base_revision_id;
  assert.equal(duplicateBaseRevisionId, firstSaveResult.revision_id);
  const duplicatePayload = { ...firstSavePayload, base_revision_id: duplicateBaseRevisionId };
  const duplicateBodies = [
    duplicateSettingsSchemaVersion(duplicatePayload),
    duplicateTextSettingValue(duplicatePayload),
  ];
  for (const body of duplicateBodies) {
    const response = await fetch(`${origin}/api/funnels/${primaryId}/preview`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body,
    });
    assert.equal(response.status, 400, await response.text());
  }
  assert.equal(
    (await apiEditorState(origin, primaryId)).base_revision_id,
    duplicateBaseRevisionId,
    "Duplicate preview keys do not write a revision",
  );

  for (const body of [
    duplicateSettingsSchemaVersion(firstSavePayload),
    duplicateTextSettingValue(firstSavePayload),
  ]) {
    const response = await fetch(`${origin}/api/funnels/${primaryId}/revisions`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": firstSaveKey },
      body,
    });
    assert.equal(response.status, 400, await response.text());
  }
  assert.equal(
    (await apiEditorState(origin, primaryId)).base_revision_id,
    duplicateBaseRevisionId,
    "Duplicate save keys cannot replay or advance the active revision",
  );

  const freshIdempotencyKey = `pr011-duplicate-${crypto.randomUUID()}`;
  for (const body of duplicateBodies) {
    const response = await fetch(`${origin}/api/funnels/${primaryId}/revisions`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": freshIdempotencyKey },
      body,
    });
    assert.equal(response.status, 400, await response.text());
  }
  assert.equal(
    (await apiEditorState(origin, primaryId)).base_revision_id,
    duplicateBaseRevisionId,
    "Duplicate saves with a fresh key do not write a revision",
  );
  const freshSave = await fetch(`${origin}/api/funnels/${primaryId}/revisions`, {
    method: "PUT",
    headers: { "Content-Type": "application/json", "Idempotency-Key": freshIdempotencyKey },
    body: JSON.stringify(duplicatePayload),
  });
  const freshSaveText = await freshSave.text();
  assert.equal(freshSave.status, 200, freshSaveText);
  assert.notEqual(
    JSON.parse(freshSaveText).revision_id,
    duplicateBaseRevisionId,
    "The fresh key remains available after duplicate-key errors",
  );

  await page.reload({ waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "PR-010 browser editor" }).waitFor();
  const reloadedHeroRow = page.locator('[data-editor-section="hero"]');
  await reloadedHeroRow.waitFor({ state: "visible" });
  await reloadedHeroRow.focus();
  await page.waitForFunction(() =>
    document.querySelector('[data-editor-section="hero"]')?.getAttribute("aria-current") === "true",
  );
  await setting("setting-section.hero.title").waitFor({ state: "visible" });
  assert.equal(await setting("setting-section.hero.title").inputValue(), "PR-011 inspector headline");
  assert.equal(await setting("setting-theme.preset").inputValue(), "bold");
  assert.equal(await setting("setting-theme.primary_color").inputValue(), "#123456");
  assert.equal(await setting("setting-theme.square_buttons").isChecked(), true);
  assert.equal(await setting("setting-section.hero.image-url").inputValue(), "https://example.test/pr011-image.png");
  assert.equal(await setting("setting-section.hero.image-alt").inputValue(), "PR-011 preview image");
  await page.frameLocator("iframe").getByText("PR-011 preview image", { exact: true }).waitFor();
  assert.equal(await page.frameLocator("iframe").locator("img").count(), 0);

  const idempotencyRaceFunnels = await Promise.all([
    apiCreate(origin, "PR-010 shared key first funnel"),
    apiCreate(origin, "PR-010 shared key second funnel"),
  ]);
  const idempotencyRaceStates = await Promise.all(
    idempotencyRaceFunnels.map(async ({ funnel }) => {
      const response = await fetch(`${origin}/api/funnels/${funnel.id}`);
      const body = await response.text();
      assert.equal(response.status, 200, body);
      return JSON.parse(body);
    }),
  );
  const sharedKey = `pr010-shared-${crypto.randomUUID()}`;
  const sharedKeySaves = await Promise.all(
    idempotencyRaceStates.map((state) =>
      fetch(`${origin}/api/funnels/${state.funnel.id}/revisions`, {
        method: "PUT",
        headers: { "Content-Type": "application/json", "Idempotency-Key": sharedKey },
        body: JSON.stringify({
          base_revision_id: state.base_revision_id,
          section_ids: state.sections.map(({ id }) => id).reverse(),
        }),
      }),
    ),
  );
  const sharedKeyResults = await Promise.all(
    sharedKeySaves.map(async (response) => ({
      status: response.status,
      body: await response.text(),
    })),
  );
  assert.deepEqual(
    sharedKeyResults.map(({ status }) => status).sort(),
    [200, 409],
    JSON.stringify(sharedKeyResults),
  );
  const idempotencyRaceAfter = await Promise.all(
    idempotencyRaceStates.map(async (state) => {
      const response = await fetch(`${origin}/api/funnels/${state.funnel.id}`);
      const body = await response.text();
      assert.equal(response.status, 200, body);
      return JSON.parse(body);
    }),
  );
  const losingSaveIndex = sharedKeyResults.findIndex(({ status }) => status === 409);
  assert.equal(
    idempotencyRaceAfter[losingSaveIndex].base_revision_id,
    idempotencyRaceStates[losingSaveIndex].base_revision_id,
    "A request that loses an idempotency-key race must not advance its funnel head",
  );
  const winningSaveIndex = sharedKeyResults.findIndex(({ status }) => status === 200);
  assert.notEqual(
    idempotencyRaceAfter[winningSaveIndex].base_revision_id,
    idempotencyRaceStates[winningSaveIndex].base_revision_id,
  );
  assert.equal(JSON.parse(sharedKeyResults[losingSaveIndex].body).code, "conflict/idempotency_payload_mismatch");

  const stale = await fetch(`${origin}/api/funnels/${primaryId}/revisions`, {
    method: "PUT",
    headers: { "Content-Type": "application/json", "Idempotency-Key": `pr010-stale-${crypto.randomUUID()}` },
    body: JSON.stringify(firstSavePayload),
  });
  const staleText = await stale.text();
  assert.equal(stale.status, 409, staleText);
  assert.equal(JSON.parse(staleText).code, "conflict/edit_stale_revision");

  await clickWithPointer(page, page.locator('.primary-nav a[href="/funnels"]'));
  await page.getByRole("heading", { name: "Your funnels" }).waitFor();
  await clickWithPointer(page, page.getByRole("link", { name: "PR-010 route race" }));
  await page.getByRole("heading", { name: "PR-010 route race" }).waitFor();
  await page.getByRole("button", { name: "Move up CTA" }).click();
  await waitForOrder(page, ["hero", "cta", "features"]);

  let release;
  const savedOnServer = new Promise((resolve) => {
    releaseDelayedSave = () => release?.();
    release = resolve;
  });
  let delayedResponse;
  let finishDelayedResponse;
  const responseReady = new Promise((resolve) => (finishDelayedResponse = resolve));
  await page.route(`**/api/funnels/${secondaryId}/revisions`, async (route) => {
    const response = await route.fetch();
    delayedResponse = {
      status: response.status(),
      headers: response.headers(),
      body: await response.body(),
    };
    finishDelayedResponse();
    await savedOnServer;
    await route.fulfill(delayedResponse);
  });
  const raceSavePromise = page.getByRole("button", { name: "Save revision" }).click();
  await responseReady;
  assert.equal(delayedResponse.status, 200, "Delayed save must commit before the route changes");
  await page.getByRole("button", { name: "Saving…" }).waitFor();
  await clickWithPointer(page, page.locator('.primary-nav a[href="/funnels"]'));
  await page.getByRole("heading", { name: "Your funnels" }).waitFor();
  await clickWithPointer(page, page.getByRole("link", { name: "PR-010 browser editor" }));
  await page.getByRole("heading", { name: "PR-010 browser editor" }).waitFor();
  const primaryBaseBeforeOldResponse = await page.locator(".editor-toolbar code").first().innerText();
  release();
  await raceSavePromise;
  await page.waitForTimeout(100);
  assert.equal(await page.getByRole("heading", { name: "PR-010 browser editor" }).count(), 1);
  assert.equal(await page.locator(".editor-toolbar code").first().innerText(), primaryBaseBeforeOldResponse);
  await page.getByText("All changes saved").waitFor();

  const conversionFunnel = await apiCreate(origin, "PR-024 browser conversion");
  await page.goto(`${origin}/funnels/${conversionFunnel.funnel.id}`, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "PR-024 browser conversion" }).waitFor();
  const conversionPreviewRoute = `**/api/funnels/${conversionFunnel.funnel.id}/convert-v2/preview`;
  let releaseConversionPreview;
  let reportConversionPreviewReady;
  const conversionPreviewReady = new Promise((resolve) => (reportConversionPreviewReady = resolve));
  const conversionPreviewReleased = new Promise((resolve) => (releaseConversionPreview = resolve));
  await page.route(conversionPreviewRoute, async (route) => {
    const response = await route.fetch();
    const preview = await response.json();
    reportConversionPreviewReady();
    await conversionPreviewReleased;
    await route.fulfill({
      response,
      body: JSON.stringify({
        ...preview,
        conversion_available: true,
        blockers: [],
        warnings: [],
        preview_html: "<!doctype html><html><body>stale conversion preview</body></html>",
      }),
    });
  });
  const delayedConversionPreview = page
    .getByRole("button", { name: "Review typed conversion" })
    .click();
  await conversionPreviewReady;
  await page.evaluate((funnelId) => {
    window.history.pushState(null, "", `/funnels/${funnelId}`);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }, secondaryId);
  await page.getByRole("heading", { name: "PR-010 route race" }).waitFor();
  releaseConversionPreview();
  await delayedConversionPreview;
  await delay(100);
  assert.equal(
    await page.locator(".editor-conversion-preview iframe").count(),
    0,
    "A late V1 conversion preview must not be applied to the next funnel route",
  );
  await page.unroute(conversionPreviewRoute);
  await page.goto(`${origin}/funnels/${conversionFunnel.funnel.id}`, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "PR-024 browser conversion" }).waitFor();
  const reviewConversion = page.getByRole("button", { name: "Review typed conversion" });
  const blockedConversionResponse = page.waitForResponse(
    (response) => response.request().method() === "POST"
      && response.url().endsWith(`/api/funnels/${conversionFunnel.funnel.id}/convert-v2/preview`),
  );
  await reviewConversion.click();
  const blockedConversion = await (await blockedConversionResponse).json();
  assert.equal(blockedConversion.conversion_available, false);
  assert.deepEqual(blockedConversion.blockers.map(({ path }) => path), [
    "contents.hero.cta_url",
    "contents.cta.cta_url",
  ]);
  await page.getByText("contents.hero.cta_url").waitFor();

  await clickWithPointer(page, page.locator('[data-editor-section="hero"]'));
  await page.locator('[id="setting-section.hero.cta_url"]').fill("https://example.com/offer");
  await clickWithPointer(page, page.locator('[data-editor-section="cta"]'));
  await page.locator('[id="setting-section.cta.cta_url"]').fill("https://example.com/offer");
  const convertedV1SaveResponse = page.waitForResponse(
    (response) => response.request().method() === "PUT"
      && response.url().endsWith(`/api/funnels/${conversionFunnel.funnel.id}/revisions`),
  );
  await page.getByRole("button", { name: "Save revision" }).click();
  const convertedV1Save = await convertedV1SaveResponse;
  assert.equal(convertedV1Save.status(), 200, await convertedV1Save.text());
  await page.getByText("All changes saved").waitFor();

  const readyConversionResponse = page.waitForResponse(
    (response) => response.request().method() === "POST"
      && response.url().endsWith(`/api/funnels/${conversionFunnel.funnel.id}/convert-v2/preview`),
  );
  await page.getByRole("button", { name: "Review typed conversion" }).click();
  const readyConversion = await (await readyConversionResponse).json();
  assert.equal(readyConversion.conversion_available, true);
  assert.deepEqual(readyConversion.blockers, []);
  await page.frameLocator(".editor-conversion-preview iframe")
    .getByText("PR-024 browser conversion", { exact: true }).waitFor();

  const conversionCommitRoute = `**/api/funnels/${conversionFunnel.funnel.id}/convert-v2`;
  let conversionCommitCount = 0;
  let firstConversionRequest;
  let retryConversionRequest;
  let signalConversionCommitStarted;
  let releaseConversionCommit;
  const conversionCommitStarted = new Promise((resolve) => (signalConversionCommitStarted = resolve));
  const conversionCommitGate = new Promise((resolve) => (releaseConversionCommit = resolve));
  await page.route(conversionCommitRoute, async (route) => {
    conversionCommitCount += 1;
    const request = {
      key: (await route.request().allHeaders())["idempotency-key"],
      body: route.request().postData(),
    };
    if (conversionCommitCount === 1) {
      firstConversionRequest = request;
      signalConversionCommitStarted();
      await conversionCommitGate;
      const committedResponse = await route.fetch();
      assert.equal(committedResponse.status(), 201, await committedResponse.text());
      await route.fulfill({
        status: 503,
        contentType: "application/json",
        body: JSON.stringify({ code: "service/temporarily_unavailable" }),
      });
      return;
    }
    retryConversionRequest = request;
    assert.deepEqual(retryConversionRequest, firstConversionRequest);
    await route.continue();
  });
  const startConversion = page.getByRole("button", { name: "Save as a new typed revision" }).click();
  await conversionCommitStarted;
  const conversionTitle = page.locator('[id="setting-section.cta.cta_url"]');
  assert.equal(await conversionTitle.isDisabled(), true, "V1 settings stay locked while conversion commits");
  assert.equal(
    await page.getByRole("button", { name: "Move down Hero" }).isDisabled(),
    true,
    "V1 section reordering stays locked while conversion commits",
  );
  releaseConversionCommit();
  await startConversion;
  await page.getByRole("button", { name: "Retry typed conversion" }).waitFor();
  assert.equal(await conversionTitle.isDisabled(), true, "V1 settings stay locked while conversion is uncertain");
  assert.equal(await page.getByRole("button", { name: "Retry typed conversion" }).isDisabled(), false);
  const retryConversionResponse = page.waitForResponse(
    (response) => response.request().method() === "POST"
      && response.url().endsWith(`/api/funnels/${conversionFunnel.funnel.id}/convert-v2`),
  );
  await page.getByRole("button", { name: "Retry typed conversion" }).click();
  const retriedConversion = await retryConversionResponse;
  assert.equal(retriedConversion.status(), 201, await retriedConversion.text());
  assert.deepEqual(retryConversionRequest, firstConversionRequest);
  assert.equal(await conversionTitle.isDisabled(), false, "V1 settings unlock after conversion is confirmed");
  await page.unroute(conversionCommitRoute);
  await page.getByRole("link", { name: "Open typed editor" }).waitFor();

  let failedSchemaProbe = false;
  const funnelMetadataRoute = `**/api/funnels/${conversionFunnel.funnel.id}`;
  await page.route(funnelMetadataRoute, async (route) => {
    if (route.request().method() === "GET" && !failedSchemaProbe) {
      failedSchemaProbe = true;
      await route.fulfill({ status: 503, contentType: "application/json", body: "{}" });
      return;
    }
    await route.continue();
  });
  await page.reload({ waitUntil: "networkidle" });
  await page.getByRole("alert", { name: "Funnel editor type error" }).waitFor();
  await page.getByRole("button", { name: "Try again" }).click();
  await page.getByRole("region", { name: "Typed visual editor" }).waitFor();
  await page.unroute(funnelMetadataRoute);

  await page.getByLabel("Block type").selectOption("button");
  await page.getByRole("button", { name: "Add block" }).click();
  const buttonUrl = page.locator('input[aria-label^="Destination URL for button"]').last();
  await buttonUrl.waitFor();
  assert.equal(await buttonUrl.inputValue(), "https://example.invalid/");
  const buttonEditResponse = page.waitForResponse(
    (response) => response.request().method() === "POST"
      && response.url().endsWith(`/api/funnels/${conversionFunnel.funnel.id}/edit-v2`)
      && response.request().postData()?.includes("https://example.test/pr031-checkout"),
  );
  await buttonUrl.fill("https://example.test/pr031-checkout");
  await buttonUrl.press("Tab");
  const buttonEdit = await buttonEditResponse;
  assert.equal(buttonEdit.status(), 200, await buttonEdit.text());
  const buttonEditBody = await buttonEdit.json();
  assert.ok(
    buttonEditBody.document_json.includes("https://example.test/pr031-checkout"),
    "the Rust edit command should return the changed external destination",
  );
  assert.match(buttonEditBody.preview_html, /<span class="v2-button" aria-disabled="true">Continue<\/span>/);

  await page.route("**/api/v2/stage-capacity", async (route) => {
    await route.fulfill({ status: 503, contentType: "application/json", body: "{}" });
  });
  await page.reload({ waitUntil: "networkidle" });
  await page.getByRole("region", { name: "Typed visual editor" }).waitFor();
  await page.getByRole("alert").filter({ hasText: "temporary storage budget is unavailable" }).waitFor();
  assert.equal(await page.getByRole("button", { name: "Save revision" }).isDisabled(), true);
  await page.unroute("**/api/v2/stage-capacity");
  await page.reload({ waitUntil: "networkidle" });
  await page.getByRole("region", { name: "Typed visual editor" }).waitFor();
  await page.getByRole("button", { name: "Save revision" }).waitFor({ state: "visible" });
  assert.equal(await page.getByRole("button", { name: "Save revision" }).isDisabled(), false);

  let stageCreateCount = 0;
  let stageUploadCount = 0;
  let finalizationCount = 0;
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (url.pathname === `/api/funnels/${conversionFunnel.funnel.id}/stages-v2` && request.method() === "POST") {
      stageCreateCount += 1;
    }
    if (url.pathname === `/api/funnels/${conversionFunnel.funnel.id}/stages-v2` && request.method() === "PUT") {
      stageUploadCount += 1;
    }
  });
  const finalizationRoute = `**/api/funnels/${conversionFunnel.funnel.id}/revisions-v2`;
  await page.route(finalizationRoute, async (route) => {
    finalizationCount += 1;
    if (finalizationCount === 1) {
      const response = await route.fetch();
      assert.equal(response.status(), 201, await response.text());
      await route.abort("failed");
      return;
    }
    await route.continue();
  });
  await page.getByRole("button", { name: "Save revision" }).click();
  await page.getByRole("button", { name: "Retry save" }).waitFor();
  assert.equal(await buttonUrl.isDisabled(), true, "V2 controls stay locked while the final save receipt is uncertain");
  await page.getByRole("button", { name: "Retry save" }).click();
  await page.getByText("All changes saved").waitFor();
  assert.equal(await buttonUrl.isDisabled(), false, "V2 controls unlock after the final save receipt is confirmed");
  assert.equal(finalizationCount, 2, "retry replays the final save receipt");
  assert.equal(stageCreateCount, 1, "retry does not mint another stage");
  assert.equal(stageUploadCount, 1, "retry does not upload to the consumed stage");
  await page.unroute(finalizationRoute);

  const firstPublishResponse = page.waitForResponse(
    (response) => response.request().method() === "POST"
      && response.url().endsWith(`/api/funnels/${conversionFunnel.funnel.id}/publish`),
  );
  await page.getByRole("button", { name: "Publish saved revision" }).click();
  assert.equal((await firstPublishResponse).status(), 200);
  await page.getByRole("link", { name: "Open live funnel" }).waitFor();

  const unpublishRoute = `**/api/funnels/${conversionFunnel.funnel.id}/unpublish`;
  await page.route(unpublishRoute, async (route) => {
    await route.fulfill({
      status: 409,
      contentType: "application/json",
      body: JSON.stringify({ code: "conflict/publish_stale_revision" }),
    });
  });
  await page.route(funnelMetadataRoute, async (route) => {
    if (route.request().method() === "GET") {
      await route.fulfill({ status: 503, contentType: "application/json", body: "{}" });
      return;
    }
    await route.continue();
  });
  const unpublishConflictResponse = page.waitForResponse(
    (response) => response.request().method() === "POST"
      && response.url().endsWith(`/api/funnels/${conversionFunnel.funnel.id}/unpublish`),
  );
  await page.getByRole("button", { name: "Unpublish" }).click();
  await page.getByRole("button", { name: "Confirm unpublish" }).click();
  assert.equal((await unpublishConflictResponse).status(), 409);
  await page.getByRole("alert").filter({ hasText: "current state could not be loaded" }).waitFor();
  await page.getByText("Status unavailable").waitFor();
  assert.equal(await page.getByRole("link", { name: "Open live funnel" }).count(), 0);
  assert.equal(await page.getByRole("button", { name: "Publish saved revision" }).isDisabled(), true);
  assert.equal(await page.getByRole("button", { name: "Unpublish" }).isDisabled(), true);
  await page.unroute(unpublishRoute);
  await page.unroute(funnelMetadataRoute);


  await page.locator(".editor-section-list .text-button").first().click();
  const addFormButton = page.getByRole("button", { name: "Add form" });
  await addFormButton.waitFor();
  assert.equal(await addFormButton.isEnabled(), true);
  const createFormResponse = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
    { timeout: 10000 },
  );
  await addFormButton.click();
  const createdForm = await createFormResponse.catch(async (error) => {
    const alerts = await page.getByRole("alert").allTextContents();
    throw new Error(`${error}; alerts=${alerts.join(" | ")}`);
  });
  assert.equal(createdForm.status(), 200, await createdForm.text());
  await page.getByRole("group", { name: "Form settings" }).waitFor();
  await page.getByLabel("Submission purpose").waitFor();
  await page.getByLabel("Required request consent").waitFor();
  const previewFrame = page.frameLocator(".editor-preview-frame");
  const previewForm = previewFrame.locator("form[data-preview='true']");
  await previewForm.waitFor();
  assert.equal(await previewForm.getAttribute("action"), null);
  assert.equal(await previewForm.getByRole("button", { name: "Send" }).getAttribute("type"), "button");

  const resultDestination = page.getByLabel("After submission");
  assert.equal(await resultDestination.inputValue(), "inline");
  await page.waitForFunction(() => {
    const control = [...document.querySelectorAll("select")].find((item) =>
      item.parentElement?.textContent.includes("After submission"),
    );
    return control && !control.disabled;
  });
  const resultOptions = await resultDestination.locator("option").evaluateAll((options) =>
    options.map((option) => ({ value: option.value, label: option.textContent })),
  );
  assert.deepEqual(resultOptions, [{ value: "inline", label: "Inline confirmation" }]);
  await page.getByText("No valid success page is available yet. Add another page or remove the path back to this form page.").waitFor();
  assert.equal(await previewFrame.locator(".v2-form-result").count(), 1);

  const purposeInput = page.getByLabel("Submission purpose");
  const purposeErrorId = await purposeInput.getAttribute("aria-describedby");
  const purposeError = page.locator(`[id="${purposeErrorId}"]`);
  const lastValidPurpose = await previewForm.locator("legend").textContent();
  let invalidEditRequests = 0;
  const countInvalidEditRequests = (request) => {
    if (request.method() === "POST" && request.url().includes("/edit-v2")) invalidEditRequests += 1;
  };
  page.on("request", countInvalidEditRequests);
  await purposeInput.fill("");
  await purposeError.getByText("Enter a purpose between 1 and 1,024 characters.").waitFor();
  assert.equal(await purposeInput.getAttribute("aria-invalid"), "true");
  const saveButton = page.getByRole("button", { name: "Save revision" });
  assert.equal(await saveButton.isDisabled(), true);
  await page.waitForTimeout(100);
  assert.equal(invalidEditRequests, 0);
  page.off("request", countInvalidEditRequests);
  assert.equal(await previewForm.locator("legend").textContent(), lastValidPurpose);
  const purposeEditResponse = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
    { timeout: 10000 },
  );
  await purposeInput.fill("Request a product demonstration");
  await purposeInput.press("Tab");
  const purposeEdit = await purposeEditResponse;
  assert.equal(purposeEdit.status(), 200, await purposeEdit.text());
  await page.waitForFunction(() => {
    const button = [...document.querySelectorAll("button")].find((item) => item.textContent.includes("Save revision"));
    return button && !button.disabled;
  });
  assert.equal(await previewForm.locator("legend").textContent(), "Request a product demonstration");

  const addDropdownResponse = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
    { timeout: 10000 },
  );
  await page.getByRole("button", { name: "Add dropdown field" }).click();
  const addedDropdown = await addDropdownResponse;
  assert.equal(addedDropdown.status(), 200, await addedDropdown.text());
  await previewForm.getByRole("combobox").waitFor();
  assert.equal(await previewForm.getByRole("option", { name: "Option 1" }).count(), 1);
  const dropdownType = page.getByLabel("Field type").last();
  await dropdownType.selectOption("text");
  await page.getByText("Applying this change clears the saved dropdown options.").waitFor();
  assert.equal(await saveButton.isDisabled(), true);
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  assert.equal(await dropdownType.inputValue(), "select");
  const applyTypeResponse = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
    { timeout: 10000 },
  );
  await dropdownType.selectOption("text");
  await page.getByRole("button", { name: "Apply type change" }).click();
  const appliedType = await applyTypeResponse;
  assert.equal(appliedType.status(), 200, await appliedType.text());
  await previewForm.getByRole("combobox").waitFor({ state: "detached" });

  const marketingConsentResponse = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
    { timeout: 10000 },
  );
  await page.getByLabel("Offer separate, optional marketing consent").check();
  const marketingConsent = await marketingConsentResponse;
  assert.equal(marketingConsent.status(), 200, await marketingConsent.text());
  await page.getByLabel("Optional marketing consent wording").waitFor();
  assert.equal(await previewForm.locator('input[name="_request_consent"]').count(), 1);
  assert.equal(await previewForm.locator('input[name="_marketing_opt_in"]').count(), 1);

  await page.addScriptTag({ content: require("axe-core").source });
  const convertedAxe = await page.evaluate(async () => {
    const result = await window.axe.run(document, {
      runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"] },
    });
    return result.violations.map(({ id, impact, help, nodes }) => ({ id, impact, help, nodes: nodes.length }));
  });
  assert.deepEqual(convertedAxe, [], `Converted typed editor axe findings: ${JSON.stringify(convertedAxe)}`);

  console.log(`PR-010/011/012/013/024/031 browser: navigation, keyboard, responsive preview snapshots, undo/redo shortcuts, pointer, inspector controls, live preview, V1-to-V2 conversion blockers, schema-probe retry, button destination editing, stage-capacity fallback, final-save receipt replay, failed publication refresh UI, conversion preview/save/reload, duplicate-key rejection, sandbox/network, save replay, stale CAS, route-race checks passed; critical V1 axe findings=${criticalAxeFindings.length}, converted V2 axe violations=${convertedAxe.length}`);
} catch (error) {
  console.error(`Browser diagnostics:\n${pageDiagnostics.slice(-60).join("\n")}\nWrangler log:\n${workerLog.slice(-10000)}`);
  await writeFile(
    failureLogPath,
    `Error: ${error?.stack ?? error}\n\nBrowser diagnostics:\n${pageDiagnostics.join("\n")}\n\nWrangler log:\n${workerLog}\n`,
    "utf8",
  ).catch(() => {});
  throw error;
} finally {
  await cleanup();
}
