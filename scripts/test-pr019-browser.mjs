import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { chromium } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startPr019Worker } from "./pr019-local-worker.mjs";

const require = createRequire(import.meta.url);
const root = resolve(import.meta.dirname, "..");
const axeSource = require("axe-core").source;
const diagnostics = [];
let browser;
let worker;

async function focusedElement(page) {
  return page.evaluate(() => {
    const element = document.activeElement;
    if (!(element instanceof HTMLElement)) return null;
    const text = (element.innerText || element.textContent || "").replace(/\s+/g, " ").trim();
    const labels = element.labels
      ? [...element.labels].map((label) => label.innerText.replace(/\s+/g, " ").trim()).join(" ")
      : "";
    return {
      tag: element.tagName,
      id: element.id,
      text,
      ariaLabel: element.getAttribute("aria-label") ?? "",
      labels,
      type: element.getAttribute("type") ?? "",
    };
  });
}

async function tabUntil(page, matches, name, maxTabs = 120) {
  const visited = [];
  for (let index = 0; index < maxTabs; index += 1) {
    await page.keyboard.press("Tab");
    const active = await focusedElement(page);
    visited.push(active);
    if (active && matches(active)) return active;
  }
  throw new Error(`Keyboard could not reach ${name}; focus sequence: ${JSON.stringify(visited)}`);
}

function hasName(active, expected) {
  const visibleName = active.text.replace(/^[^\p{L}\p{N}]+/u, "").trim();
  return visibleName === expected || active.ariaLabel === expected || active.labels === expected;
}

async function assertCriticalAxeFindingsAbsent(page, label) {
  if (!(await page.evaluate(() => Boolean(window.axe)))) {
    await page.addScriptTag({ content: axeSource });
  }
  const violations = await page.evaluate(async () => {
    const result = await window.axe.run(document, {
      runOnly: {
        type: "tag",
        values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"],
      },
    });
    return result.violations.map(({ id, impact, help, nodes }) => ({
      id,
      impact,
      help,
      nodeCount: nodes.length,
    }));
  });
  const critical = violations.filter((finding) => finding.impact === "critical");
  assert.deepEqual(critical, [], `${label} critical axe findings: ${JSON.stringify(critical)}`);
  console.log(`${label} axe violations: ${JSON.stringify(violations)}`);
}

async function responseJson(response, expectedStatus, label) {
  const text = await response.text();
  const status = typeof response.status === "function" ? response.status() : response.status;
  assert.equal(status, expectedStatus, `${label}: ${text}`);
  return JSON.parse(text);
}

try {
  worker = await startPr019Worker("browser");
  const executablePath = await access("/usr/bin/chromium").then(
    () => "/usr/bin/chromium",
    () => undefined,
  );
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
  const page = await browser.newPage({ viewport: { width: 1360, height: 1000 } });
  await page.addInitScript(() => {
    window.__pr019PointerEvents = 0;
    for (const eventName of ["pointerdown", "mousedown", "touchstart"]) {
      document.addEventListener(eventName, () => (window.__pr019PointerEvents += 1), true);
    }
  });
  page.on("pageerror", (error) => diagnostics.push(error.stack ?? error.message));

  await page.goto(worker.origin, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Dashboard" }).waitFor();
  await assertCriticalAxeFindingsAbsent(page, "Dashboard home");

  await tabUntil(page, (active) => active.tag === "A" && hasName(active, "Funnels"), "Funnels navigation link");
  await page.keyboard.press("Enter");
  await page.getByRole("heading", { name: "Your funnels" }).waitFor();
  await page.getByRole("button", { name: "Create funnel" }).waitFor();

  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Create funnel"),
    "Create funnel button",
  );
  await page.keyboard.press("Enter");
  const createDialog = page.getByRole("dialog", { name: "Create a funnel" });
  await createDialog.waitFor({ state: "visible" });
  const nameInput = page.getByLabel("Funnel name");
  await nameInput.waitFor();
  assert.equal(await nameInput.evaluate((element) => element === document.activeElement), true);
  const funnelName = "PR-019 keyboard journey";
  await page.keyboard.type(funnelName, { delay: 8 });
  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Create funnel"),
    "Create funnel submit button",
  );
  const createResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().endsWith("/api/funnels"),
  );
  await page.keyboard.press("Enter");
  const createResponse = await createResponsePromise;
  const created = await responseJson(createResponse, 201, "Create funnel");
  const funnelId = created.funnel?.id;
  const slug = created.funnel?.slug;
  const initialRevisionId = created.initial_revision?.id;
  assert.equal(typeof funnelId, "string");
  assert.equal(typeof slug, "string");
  assert.equal(typeof initialRevisionId, "string");
  await createDialog.waitFor({ state: "hidden" });
  await page.getByRole("link", { name: funnelName }).waitFor();
  await assertCriticalAxeFindingsAbsent(page, "Funnel dashboard");

  await tabUntil(
    page,
    (active) => active.tag === "A" && hasName(active, funnelName),
    "created funnel link",
  );
  await page.keyboard.press("Enter");
  await page.getByRole("heading", { name: funnelName }).waitFor();
  await page.getByRole("list", { name: "Funnel sections" }).waitFor();

  const titleInput = page.locator('[id="setting-section.cta.title"]');
  await tabUntil(
    page,
    (active) => active.tag === "INPUT" && active.id === "setting-section.cta.title",
    "CTA heading field",
  );
  const editedTitle = "PR-019 keyboard published CTA heading";
  await page.keyboard.press("Control+A");
  await page.keyboard.type(editedTitle, { delay: 8 });
  await page.frameLocator("iframe").getByRole("heading", { name: editedTitle }).waitFor();
  await page.getByText("Unsaved changes").waitFor();

  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Save revision"),
    "Save revision button",
  );
  const saveResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" &&
      response.url().endsWith(`/api/funnels/${funnelId}/revisions`),
  );
  await page.keyboard.press("Enter");
  const saveResponse = await saveResponsePromise;
  await responseJson(saveResponse, 200, "Save revision");
  await page.getByText("All changes saved").waitFor();
  assert.equal(await titleInput.inputValue(), editedTitle);

  const editorStateResponse = await fetch(`${worker.origin}/api/funnels/${funnelId}`);
  const editorState = await responseJson(editorStateResponse, 200, "Read saved revision");
  assert.notEqual(editorState.base_revision_id, initialRevisionId);
  const publishResponse = await fetch(`${worker.origin}/api/funnels/${funnelId}/publish`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": `pr019-publish-${randomUUID()}`,
    },
    body: JSON.stringify({
      revision_id: editorState.base_revision_id,
      expected_published_revision_id: null,
    }),
  });
  await responseJson(publishResponse, 200, "Publish funnel");
  assert.equal(await page.evaluate(() => window.__pr019PointerEvents), 0);

  const publicResponse = await page.goto(`${worker.origin}/f/${slug}`, { waitUntil: "networkidle" });
  assert.equal(publicResponse?.status(), 200);
  await page.getByRole("heading", { name: editedTitle }).waitFor();
  await assertCriticalAxeFindingsAbsent(page, "Published funnel");
  assert.equal(await page.evaluate(() => window.__pr019PointerEvents), 0);
  assert.deepEqual(diagnostics, [], `Browser errors: ${diagnostics.join("\n")}`);
  console.log(
    JSON.stringify({
      workflow: "create -> edit -> publish -> visit",
      funnelId,
      slug,
      revisionId: editorState.base_revision_id,
      keyboardOnly: true,
      pointerEvents: 0,
      axeCritical: { dashboard: 0, publishedFunnel: 0 },
    }),
  );
} finally {
  if (browser) await browser.close().catch(() => {});
  if (worker) await worker.stop();
}
