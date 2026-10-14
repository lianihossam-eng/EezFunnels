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

async function sectionOrder(page) {
  return page.locator("[data-editor-section]").evaluateAll((rows) =>
    rows.map((row) => row.getAttribute("data-editor-section")),
  );
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
  page.on("request", (request) => {
    const url = request.url();
    if (url.includes("/__pr010_probe/")) probeRequests.push(url);
    if (url.includes(`/api/funnels/${primaryId}/preview`)) previewRequests.push(url);
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

  const heroRow = page.locator('[data-editor-section="hero"]');
  await heroRow.focus();
  await page.keyboard.press("Space");
  await page.keyboard.press("ArrowDown");
  await waitForOrder(page, ["features", "hero", "cta"]);
  assert.equal(await page.locator('[aria-live="polite"]').innerText(), "Moved Hero to position 2 of 3.");
  await page.keyboard.press("Space");
  assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("data-editor-section")), "hero");
  await page.keyboard.press("Space");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Escape");
  await waitForOrder(page, ["features", "hero", "cta"]);
  assert.match(await page.locator('[aria-live="polite"]').innerText(), /original order was restored/);

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

  await page.reload({ waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "PR-010 browser editor" }).waitFor();
  await clickWithPointer(page, page.locator('[data-editor-section="hero"]'));
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
  await page.getByRole("button", { name: "Move CTA up" }).click();
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

  console.log(`PR-010/011 browser: navigation, keyboard, pointer, five inspector controls, live preview, V2 save/reload, sandbox/network, save replay, stale CAS, and route-race checks passed; critical axe findings=${criticalAxeFindings.length}`);
} catch (error) {
  await writeFile(
    failureLogPath,
    `Error: ${error?.stack ?? error}\n\nBrowser diagnostics:\n${pageDiagnostics.join("\n")}\n\nWrangler log:\n${workerLog}\n`,
    "utf8",
  ).catch(() => {});
  throw error;
} finally {
  await cleanup();
}
