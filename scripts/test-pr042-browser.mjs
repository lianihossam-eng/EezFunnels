import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { chromium } from "@playwright/test";
import { startPr019Worker } from "./pr019-local-worker.mjs";
import { finalizePr019Browser } from "./pr019-browser-finalizer.mjs";

const require = createRequire(import.meta.url);
const root = resolve(import.meta.dirname, "..");
const axeSource = require("axe-core").source;
const keyboardActions = [];
let browser;
let worker;

async function closeBrowser() {
  const activeBrowser = browser;
  if (activeBrowser) {
    await activeBrowser.close();
    browser = undefined;
  }
}

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
    };
  });
}

async function pressKeyboard(page, key) {
  keyboardActions.push({ kind: "press", key });
  await page.keyboard.press(key);
}

async function typeKeyboard(page, text) {
  keyboardActions.push({ kind: "type", length: text.length });
  await page.keyboard.type(text, { delay: 8 });
}

async function tabUntil(page, matches, name, maxTabs = 160) {
  const visited = [];
  for (let index = 0; index < maxTabs; index += 1) {
    await pressKeyboard(page, "Tab");
    const active = await focusedElement(page);
    visited.push(active);
    if (active && matches(active)) return active;
  }
  throw new Error(`Keyboard could not reach ${name}; focus sequence: ${JSON.stringify(visited)}`);
}

async function assertVisibleFocus(page, label) {
  const focus = await page.evaluate(() => {
    const element = document.activeElement;
    if (!(element instanceof HTMLElement)) return null;
    const style = getComputedStyle(element);
    return {
      outlineStyle: style.outlineStyle,
      outlineWidth: style.outlineWidth,
      boxShadow: style.boxShadow,
    };
  });
  assert.ok(
    focus &&
      ((focus.outlineStyle !== "none" && Number.parseFloat(focus.outlineWidth) > 0) ||
        (focus.boxShadow !== "none" && focus.boxShadow !== "")),
    `${label} must have visible keyboard focus: ${JSON.stringify(focus)}`,
  );
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
      nodes: nodes.map(({ target, html, failureSummary }) => ({
        target,
        html: html.slice(0, 300),
        failureSummary,
      })),
    }));
  });
  const seriousOrCritical = violations.filter(
    (finding) => finding.impact === "serious" || finding.impact === "critical",
  );
  assert.deepEqual(
    seriousOrCritical,
    [],
    `${label} serious or critical axe findings: ${JSON.stringify(seriousOrCritical)}`,
  );
  console.log(`${label} axe violations: ${JSON.stringify(violations)}`);
}

async function responseJson(response, expectedStatus, label) {
  const text = await response.text();
  const status = typeof response.status === "function" ? response.status() : response.status;
  assert.equal(status, expectedStatus, `${label}: ${text}`);
  return JSON.parse(text);
}

try {
  worker = await startPr019Worker("browser", { onSignalCleanup: closeBrowser });
  const executablePath = await access("/usr/bin/chromium").then(
    () => "/usr/bin/chromium",
    () => undefined,
  );
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
  const page = await browser.newPage({ viewport: { width: 1360, height: 1000 } });
  await page.addInitScript(() => {
    window.__pr042PointerEvents = 0;
    for (const eventName of ["pointerdown", "mousedown", "touchstart"]) {
      document.addEventListener(eventName, () => (window.__pr042PointerEvents += 1), true);
    }
  });

  await page.goto(worker.origin, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Dashboard" }).waitFor();
  await tabUntil(page, (active) => active.tag === "A" && hasName(active, "Funnels"), "Funnels navigation link");
  await pressKeyboard(page, "Enter");
  await page.getByRole("heading", { name: "Your funnels" }).waitFor();

  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Create funnel"),
    "Create funnel button",
  );
  await pressKeyboard(page, "Enter");
  const createDialog = page.getByRole("dialog", { name: "Create a funnel" });
  await createDialog.waitFor({ state: "visible" });
  const nameInput = page.getByLabel("Funnel name");
  await nameInput.waitFor();
  assert.equal(await nameInput.evaluate((element) => element === document.activeElement), true);
  const funnelName = "PR-042 page routing journey";
  await typeKeyboard(page, funnelName);
  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Create funnel"),
    "Create funnel submit button",
  );
  const createResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().endsWith("/api/funnels/v2"),
  );
  await pressKeyboard(page, "Enter");
  const created = await responseJson(await createResponsePromise, 201, "Create V2 funnel");
  const funnelId = created.funnel?.id;
  const funnelSlug = created.funnel?.slug;
  assert.equal(typeof funnelId, "string");
  assert.equal(typeof funnelSlug, "string");
  await createDialog.waitFor({ state: "hidden" });
  await page.getByRole("link", { name: funnelName }).waitFor();
  await tabUntil(page, (active) => active.tag === "A" && hasName(active, funnelName), "created funnel link");
  await pressKeyboard(page, "Enter");
  await page.getByRole("heading", { name: funnelName }).waitFor();
  await page.getByRole("region", { name: "Typed visual editor" }).waitFor();
  const publicationPanel = page.getByRole("region", { name: "Publication" });
  await publicationPanel.waitFor();
  await publicationPanel.getByRole("button", { name: "Publish saved revision" }).waitFor();

  await page.getByText("A Continue button will be added to the selected page so visitors can reach this page.").waitFor();
  await tabUntil(
    page,
    (active) => active.tag === "INPUT" && active.id === "v2-new-page-title",
    "new page title field",
  );
  await assertVisibleFocus(page, "new page title field");
  await pressKeyboard(page, "Control+A");
  await typeKeyboard(page, "Thank you");
  await tabUntil(
    page,
    (active) => active.tag === "INPUT" && active.id === "v2-new-page-slug",
    "new page slug field",
  );
  await pressKeyboard(page, "Control+A");
  await typeKeyboard(page, "thanks");
  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Add page"),
    "Add page button",
  );
  await assertVisibleFocus(page, "Add page button");
  const addPageResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().endsWith(`/api/funnels/${funnelId}/edit-v2`),
  );
  await pressKeyboard(page, "Enter");
  await responseJson(await addPageResponsePromise, 200, "Add routed page");
  const currentPage = page.getByLabel("Current page");
  await currentPage.waitFor();
  await page.waitForFunction(() => {
    const control = document.getElementById("v2-current-page");
    return control instanceof HTMLSelectElement && !control.disabled;
  });
  const newPageId = await currentPage.inputValue();
  assert.equal(await page.locator(`#v2-page-title-${newPageId}`).inputValue(), "Thank you");
  assert.equal(await page.locator(`#v2-page-slug-${newPageId}`).inputValue(), "thanks");
  assert.match(
    await currentPage.locator(`option[value="${newPageId}"]`).textContent(),
    new RegExp(`Thank you · /f/${funnelSlug}/thanks`),
  );

  await currentPage.focus();
  await pressKeyboard(page, "Home");
  await pressKeyboard(page, "Enter");
  await page.waitForFunction(() => {
    const control = document.getElementById("v2-current-page");
    return control instanceof HTMLSelectElement && !control.disabled && control.value === "page-home";
  });
  const ctaSection = page.getByRole("button", { name: "Call to action", exact: true });
  await ctaSection.focus();
  await pressKeyboard(page, "Enter");
  const routeButtonId = await page.locator('select[aria-label^="Button action for"]')
    .evaluateAll((elements, destinationPageId) => elements.find((element) => {
      const blockId = element.id.replace("v2-button-action-", "");
      const destination = document.getElementById(`v2-button-page-${blockId}`);
      return element.value === "page" && destination?.value === destinationPageId;
    })?.id.replace("v2-button-action-", ""), newPageId);
  assert.equal(typeof routeButtonId, "string", "The new page has an internal route button");
  const editedButtonId = await page.locator('select[aria-label^="Button action for"]')
    .evaluateAll((elements) => elements.find((element) => element.value === "external")
      ?.id.replace("v2-button-action-", ""));
  assert.equal(typeof editedButtonId, "string", "The starter CTA retains its external destination");
  const actionControl = page.locator(`#v2-button-action-${editedButtonId}`);
  const destinationControl = page.locator(`#v2-button-page-${editedButtonId}`);

  await page.waitForFunction((id) => {
    const control = document.getElementById(`v2-button-action-${id}`);
    return control instanceof HTMLSelectElement && !control.disabled;
  }, editedButtonId);
  await tabUntil(page, (active) => active.tag === "SELECT" && active.id === `v2-button-action-${editedButtonId}`, "button action selector");
  await assertVisibleFocus(page, "button action selector");
  assert.equal(await actionControl.getAttribute("aria-label"), `Button action for ${editedButtonId}`);
  let editResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().endsWith(`/api/funnels/${funnelId}/edit-v2`),
  );
  editResponsePromise.catch(() => {});
  await pressKeyboard(page, "End");
  await pressKeyboard(page, "Enter");
  await responseJson(await editResponsePromise, 200, "Change button action to internal page");
  assert.equal(await actionControl.inputValue(), "page");
  assert.equal(await destinationControl.inputValue(), newPageId);
  assert.match(
    await destinationControl.locator(`option[value="${newPageId}"]`).textContent(),
    new RegExp(`Thank you · /f/${funnelSlug}/thanks`),
  );
  await page.waitForFunction((id) => {
    const control = document.getElementById(`v2-button-action-${id}`);
    return control instanceof HTMLSelectElement && !control.disabled;
  }, editedButtonId);
  await actionControl.focus();
  assert.equal(await actionControl.evaluate((element) => element === document.activeElement), true);
  editResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().endsWith(`/api/funnels/${funnelId}/edit-v2`),
  );
  editResponsePromise.catch(() => {});
  await pressKeyboard(page, "Home");
  await pressKeyboard(page, "Enter");
  await responseJson(await editResponsePromise, 200, "Restore HTTPS destination");
  assert.equal(await actionControl.inputValue(), "external");
  const buttonLabels = await page.locator('textarea[aria-label^="Button content for"]')
    .evaluateAll((elements) => elements.map((element) => element.value));
  assert.ok(buttonLabels.includes("Get started"), "Changing button action preserves its label");
  assert.ok(buttonLabels.includes("Continue"), "The new page route remains available");
  await assertCriticalAxeFindingsAbsent(page, "V2 page routing controls");

  const headingInput = page.getByLabel("Heading content for block-cta-title");
  await tabUntil(
    page,
    (active) => active.tag === "TEXTAREA" && active.ariaLabel === "Heading content for block-cta-title",
    "CTA heading field",
  );
  await assertVisibleFocus(page, "CTA heading field");
  await pressKeyboard(page, "Control+A");
  await typeKeyboard(page, "PR-042 route and publication test");
  await pressKeyboard(page, "Tab");
  await page.frameLocator("iframe").getByRole("heading", { name: "PR-042 route and publication test" }).waitFor();

  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Save revision"),
    "Save revision button",
  );
  const saveResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().endsWith(`/api/funnels/${funnelId}/revisions-v2`),
  );
  await pressKeyboard(page, "Enter");
  const saved = await responseJson(await saveResponsePromise, 201, "Save routed revision");
  const revisionId = saved.revision_id ?? saved.revision?.id;
  assert.equal(typeof revisionId, "string");
  await page.getByText("All changes saved").waitFor();
  await assertCriticalAxeFindingsAbsent(page, "Saved V2 routing editor");

  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Publish saved revision"),
    "Publish saved revision button",
  );
  await assertVisibleFocus(page, "Publish saved revision button");
  const publishResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().endsWith(`/api/funnels/${funnelId}/publish`),
  );
  await pressKeyboard(page, "Enter");
  await responseJson(await publishResponsePromise, 200, "Publish routed revision");
  await publicationPanel.getByRole("status").filter({ hasText: "Revision published." }).waitFor();

  const homeResponse = await page.goto(`${worker.origin}/f/${funnelSlug}`, { waitUntil: "networkidle" });
  assert.equal(homeResponse?.status(), 200);
  await page.getByRole("heading", { name: "PR-042 route and publication test" }).waitFor();
  await tabUntil(page, (active) => active.tag === "A" && hasName(active, "Continue"), "public route button");
  await assertVisibleFocus(page, "public route button");
  const destinationResponsePromise = page.waitForResponse(
    (response) => response.url().endsWith(`/f/${funnelSlug}/thanks`),
  );
  await pressKeyboard(page, "Enter");
  const destinationResponse = await destinationResponsePromise;
  assert.equal(destinationResponse.status(), 200);
  await page.waitForURL(`**/f/${funnelSlug}/thanks`);
  await page.getByRole("heading", { name: "Thank you" }).waitFor();
  await assertCriticalAxeFindingsAbsent(page, "Published thank-you page");

  const pointerEvents = await page.evaluate(() => window.__pr042PointerEvents);
  assert.equal(pointerEvents, 0, "The create, route, publish, and visit journey uses keyboard input");
  assert.ok(keyboardActions.length > 0);
  console.log(JSON.stringify({
    workflow: "keyboard create V2 funnel -> add reachable page -> edit button action -> save -> publish -> follow route to new page",
    funnelId,
    funnelSlug,
    newPageId,
    newPageRoute: `/f/${funnelSlug}/thanks`,
    revisionId,
    keyboardActionCount: keyboardActions.length,
    pointerEvents,
    axeCritical: { routingEditor: 0, savedEditor: 0, publishedPage: 0 },
  }));
} finally {
  await finalizePr019Browser(closeBrowser, worker);
}
