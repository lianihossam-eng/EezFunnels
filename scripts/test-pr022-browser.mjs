import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { chromium } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startPr019Worker } from "./pr019-local-worker.mjs";
import { finalizePr019Browser } from "./pr019-browser-finalizer.mjs";

const require = createRequire(import.meta.url);
const root = resolve(import.meta.dirname, "..");
const axeSource = require("axe-core").source;
const diagnostics = [];
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
      type: element.getAttribute("type") ?? "",
    };
  });
}

async function pressKeyboard(page, key, options) {
  keyboardActions.push({ kind: "press", key });
  await page.keyboard.press(key, options);
}

async function typeKeyboard(page, text, options) {
  keyboardActions.push({ kind: "type", length: text.length });
  await page.keyboard.type(text, options);
}

async function tabUntil(page, matches, name, maxTabs = 120) {
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
      tag: element.tagName,
      name: (element.innerText || element.textContent || "").replace(/\s+/g, " ").trim(),
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
    window.__pr022PointerEvents = 0;
    for (const eventName of ["pointerdown", "mousedown", "touchstart"]) {
      document.addEventListener(eventName, () => (window.__pr022PointerEvents += 1), true);
    }
  });
  page.on("pageerror", (error) => diagnostics.push(error.stack ?? error.message));

  await page.goto(worker.origin, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Dashboard" }).waitFor();
  await assertCriticalAxeFindingsAbsent(page, "Dashboard home");

  await tabUntil(page, (active) => active.tag === "A" && hasName(active, "Funnels"), "Funnels navigation link");
  await pressKeyboard(page, "Enter");
  await page.getByRole("heading", { name: "Your funnels" }).waitFor();
  await page.getByRole("button", { name: "Create funnel" }).waitFor();

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
  const funnelName = "PR-022 publication journey";
  await typeKeyboard(page, funnelName, { delay: 8 });
  await pressKeyboard(page, "Tab");
  const publicLanguage = page.getByLabel("Public language");
  assert.equal(await publicLanguage.evaluate((element) => element === document.activeElement), true);
  await pressKeyboard(page, "ArrowDown");
  assert.equal(await publicLanguage.inputValue(), "fr");
  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Create funnel"),
    "Create funnel submit button",
  );
  const createResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().endsWith("/api/funnels/v2"),
  );
  const createRequestPromise = page.waitForRequest(
    (request) => request.method() === "POST" && request.url().endsWith("/api/funnels/v2"),
  );
  await pressKeyboard(page, "Enter");
  const [createResponse, createRequest] = await Promise.all([
    createResponsePromise,
    createRequestPromise,
  ]);
  assert.equal(createRequest.postDataJSON()?.language, "fr");
  const created = await responseJson(createResponse, 201, "Create funnel");
  const createdV2FunnelId = created.funnel?.id;
  assert.equal(typeof createdV2FunnelId, "string");
  assert.equal(typeof created.funnel?.slug, "string");
  assert.equal(typeof created.initial_revision?.id, "string");
  await createDialog.waitFor({ state: "hidden" });
  await page.getByRole("link", { name: funnelName }).waitFor();
  await assertCriticalAxeFindingsAbsent(page, "Funnel dashboard");

  await page.getByRole("link", { name: funnelName }).click();
  await page.getByRole("heading", { name: funnelName }).waitFor();
  await page.locator(".editor-section-list .text-button").first().waitFor();
  await page.locator(".editor-section-list .text-button").first().click();
  const frenchFormResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
  );
  await page.getByRole("button", { name: "Add form" }).click();
  assert.equal((await frenchFormResponsePromise).status(), 200);
  const frenchPurpose = page.getByLabel("Submission purpose");
  await frenchPurpose.waitFor();
  assert.equal(await frenchPurpose.inputValue(), "Répondre à cette demande.");
  assert.equal(
    await page.getByLabel("Required request consent").inputValue(),
    "J’accepte d’être contacté au sujet de cette demande.",
  );
  const frenchPreview = page.frameLocator(".editor-preview-frame");
  const frenchPreviewForm = frenchPreview.locator("form[data-preview='true']");
  await frenchPreviewForm.getByLabel("Votre nom").waitFor();
  await frenchPreviewForm.getByLabel("Adresse e-mail").waitFor();
  await frenchPreviewForm.getByRole("button", { name: "Envoyer" }).waitFor();
  assert.equal(await frenchPreview.getByText("Merci, votre demande a bien été reçue.").count(), 1);

  const frenchTextFieldResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
  );
  await page.getByRole("button", { name: "Add text field" }).click();
  assert.equal((await frenchTextFieldResponsePromise).status(), 200);
  await frenchPreviewForm.getByLabel("Nouveau champ texte").waitFor();

  const frenchMarketingResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
  );
  await page.getByLabel("Offer separate, optional marketing consent").check();
  assert.equal((await frenchMarketingResponsePromise).status(), 200);
  assert.equal(
    await page.getByLabel("Optional marketing consent wording").inputValue(),
    "Je souhaite recevoir ponctuellement des nouvelles du produit.",
  );

  const frenchPageResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
  );
  await page.getByRole("button", { name: "Add page" }).click();
  assert.equal((await frenchPageResponsePromise).status(), 200);
  await page.getByLabel("Current page").selectOption("page-home");
  const resultDestination = page.getByLabel("After submission");
  const frenchSuccessPageId = await resultDestination.locator("option").evaluateAll((options) =>
    options.find((option) => option.label === "Success page: Étape 2")?.value,
  );
  assert.ok(frenchSuccessPageId);
  const successPageModeResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
  );
  await resultDestination.selectOption(frenchSuccessPageId);
  assert.equal((await successPageModeResponsePromise).status(), 200);
  const inlineResultResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
  );
  await resultDestination.selectOption("inline");
  assert.equal((await inlineResultResponsePromise).status(), 200);
  await frenchPreview.getByText("Merci, votre demande a bien été reçue.").waitFor({ state: "attached" });

  await page.getByLabel("Block type").selectOption("heading");
  const frenchBlockResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().includes("/edit-v2"),
  );
  await page.getByRole("button", { name: "Add block" }).click();
  assert.equal((await frenchBlockResponsePromise).status(), 200);
  await frenchPreview.getByRole("heading", { name: "Nouveau titre" }).waitFor();

  const funnelNameV1 = "PR-022 V1 publication journey";
  const createV1Response = await fetch(`${worker.origin}/api/funnels`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": `pr022-v1-${randomUUID()}`,
    },
    body: JSON.stringify({ name: funnelNameV1, blueprint: "tripwire" }),
  });
  const createdV1 = await responseJson(createV1Response, 201, "Create V1 funnel fixture");
  const funnelId = createdV1.funnel?.id;
  const slug = createdV1.funnel?.slug;
  const initialRevisionId = createdV1.initial_revision?.id;
  assert.equal(typeof funnelId, "string");
  assert.equal(typeof slug, "string");
  assert.equal(typeof initialRevisionId, "string");

  await page.goto(`${worker.origin}/funnels`, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Your funnels" }).waitFor();
  await page.getByRole("link", { name: funnelNameV1 }).waitFor();

  await tabUntil(
    page,
    (active) => active.tag === "A" && hasName(active, funnelNameV1),
    "created V1 funnel link",
  );
  await pressKeyboard(page, "Enter");
  await page.getByRole("heading", { name: funnelNameV1 }).waitFor();
  await page.getByRole("list", { name: "Funnel sections" }).waitFor();
  const publicationPanel = page.getByRole("region", { name: "Publication" });
  await publicationPanel.waitFor();
  await publicationPanel.getByText("Not published").waitFor();
  await publicationPanel.getByRole("button", { name: "Publish revision" }).waitFor();
  await assertCriticalAxeFindingsAbsent(page, "Funnel editor before publication");

  const titleInput = page.locator('[id="setting-section.cta.title"]');
  await tabUntil(
    page,
    (active) => active.tag === "INPUT" && active.id === "setting-section.cta.title",
    "CTA heading field",
  );
  await assertVisibleFocus(page, "CTA heading field");
  const savedTitle = "PR-022 saved CTA heading";
  await pressKeyboard(page, "Control+A");
  await typeKeyboard(page, savedTitle, { delay: 8 });
  await page.frameLocator("iframe").getByRole("heading", { name: savedTitle }).waitFor();
  await page.getByText("Unsaved changes").waitFor();

  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Save revision"),
    "Save revision button",
  );
  await assertVisibleFocus(page, "Save revision button");
  const saveResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" &&
      response.url().endsWith(`/api/funnels/${funnelId}/revisions`),
  );
  await pressKeyboard(page, "Enter");
  const saveResponse = await saveResponsePromise;
  await responseJson(saveResponse, 200, "Save revision");
  await page.getByText("All changes saved").waitFor();
  assert.equal(await titleInput.inputValue(), savedTitle);

  const editorStateResponse = await fetch(`${worker.origin}/api/funnels/${funnelId}`);
  const firstSavedState = await responseJson(editorStateResponse, 200, "Read saved revision");
  const firstSavedRevisionId = firstSavedState.base_revision_id;
  assert.notEqual(firstSavedRevisionId, initialRevisionId);

  const publishUrl = `${worker.origin}/api/funnels/${funnelId}/publish`;
  let staleRequestCount = 0;
  let signalStaleRequestSeen;
  let releaseStaleRequest;
  const staleRequestSeen = new Promise((resolvePromise) => (signalStaleRequestSeen = resolvePromise));
  const staleRequestGate = new Promise((resolvePromise) => (releaseStaleRequest = resolvePromise));
  const staleRouteHandler = async (route) => {
    staleRequestCount += 1;
    if (staleRequestCount === 1) {
      const staleBody = route.request().postDataJSON();
      assert.equal(staleBody.expected_published_revision_id, null);
      const competingResponse = await fetch(publishUrl, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": `pr022-competitor-${randomUUID()}`,
        },
        body: JSON.stringify({
          revision_id: initialRevisionId,
          expected_published_revision_id: null,
        }),
      });
      await responseJson(competingResponse, 200, "Competing publish establishes live pointer");
      const secondStaleResponse = await fetch(publishUrl, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": `pr022-stale-${randomUUID()}`,
        },
        body: JSON.stringify(staleBody),
      });
      await responseJson(secondStaleResponse, 409, "Second stale publish request");
      signalStaleRequestSeen({ staleBody, request: route.request() });
      await staleRequestGate;
    }
    await route.continue();
  };
  await page.route(publishUrl, staleRouteHandler);
  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Publish revision"),
    "Publish revision button",
  );
  await assertVisibleFocus(page, "Publish revision button");
  const staleUiResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url() === publishUrl,
  );
  await pressKeyboard(page, "Enter");
  const staleDetails = await staleRequestSeen;

  const postConflictTitle = "PR-022 unsaved title preserved after conflict";
  await tabUntil(
    page,
    (active) => active.tag === "INPUT" && active.id === "setting-section.cta.title",
    "CTA heading while publication is pending",
  );
  await pressKeyboard(page, "Control+A");
  await typeKeyboard(page, postConflictTitle, { delay: 8 });
  await page.frameLocator("iframe").getByRole("heading", { name: postConflictTitle }).waitFor();
  releaseStaleRequest();
  const staleUiResponse = await staleUiResponsePromise;
  await responseJson(staleUiResponse, 409, "Stale dashboard publish request");
  await page.unroute(publishUrl, staleRouteHandler);
  await publicationPanel.getByRole("alert").waitFor();
  await publicationPanel.getByText(initialRevisionId).waitFor();
  assert.equal(await titleInput.inputValue(), postConflictTitle);
  await page.getByText("Unsaved changes").waitFor();

  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Save revision"),
    "Save preserved draft after conflict",
  );
  const secondSavePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" &&
      response.url().endsWith(`/api/funnels/${funnelId}/revisions`),
  );
  await pressKeyboard(page, "Enter");
  await responseJson(await secondSavePromise, 200, "Save preserved draft");
  await page.getByText("All changes saved").waitFor();
  assert.equal(await titleInput.inputValue(), postConflictTitle);

  const finalEditorState = await responseJson(
    await fetch(`${worker.origin}/api/funnels/${funnelId}`),
    200,
    "Read revision after conflict recovery",
  );
  const finalRevisionId = finalEditorState.base_revision_id;
  assert.notEqual(finalRevisionId, firstSavedRevisionId);
  let lostResponseRequest;
  let idempotentRetryRequest;
  let retryCount = 0;
  let signalLostRequest;
  let releaseLostRequest;
  const lostRequestSeen = new Promise((resolvePromise) => (signalLostRequest = resolvePromise));
  const lostRequestGate = new Promise((resolvePromise) => (releaseLostRequest = resolvePromise));
  const lostResponseRoute = async (route) => {
    retryCount += 1;
    const request = {
      key: (await route.request().allHeaders())["idempotency-key"],
      body: route.request().postData(),
    };
    if (retryCount === 1) {
      lostResponseRequest = request;
      signalLostRequest();
      await lostRequestGate;
      const committedResponse = await route.fetch();
      assert.equal(committedResponse.status(), 200);
      await route.abort("failed");
      return;
    }
    idempotentRetryRequest = request;
    assert.deepEqual(idempotentRetryRequest, lostResponseRequest);
    const committedResponse = await route.fetch();
    assert.equal(committedResponse.status(), 200);
    await route.fulfill({ response: committedResponse });
  };
  await page.route(publishUrl, lostResponseRoute);
  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Publish revision"),
    "Publish saved revision after conflict recovery",
  );
  await pressKeyboard(page, "Enter");
  await lostRequestSeen;
  const lostResponseDraftTitle = "PR-022 unsaved edit during ambiguous publish";
  await tabUntil(
    page,
    (active) => active.tag === "INPUT" && active.id === "setting-section.cta.title",
    "CTA heading while the publish response is pending",
  );
  await pressKeyboard(page, "Control+A");
  await typeKeyboard(page, lostResponseDraftTitle, { delay: 8 });
  await page.frameLocator("iframe").getByRole("heading", { name: lostResponseDraftTitle }).waitFor();
  await page.getByText("Unsaved changes").waitFor();
  releaseLostRequest();
  await publicationPanel.getByRole("button", { name: "Retry publish" }).waitFor();
  assert.equal(await titleInput.inputValue(), lostResponseDraftTitle);
  const publishedLiveState = await responseJson(
    await fetch(`${worker.origin}/api/funnels/${funnelId}`),
    200,
    "Verify server committed the lost-response publish",
  );
  assert.equal(publishedLiveState.published_revision_id, finalRevisionId);
  const competingUnpublish = await fetch(`${worker.origin}/api/funnels/${funnelId}/unpublish`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": `pr022-competing-unpublish-${randomUUID()}`,
    },
    body: JSON.stringify({ expected_published_revision_id: finalRevisionId }),
  });
  await responseJson(competingUnpublish, 200, "Competing session unpublishes after the lost response");
  const competingUnpublishState = await responseJson(
    await fetch(`${worker.origin}/api/funnels/${funnelId}`),
    200,
    "Verify competing unpublish changed the current live pointer",
  );
  assert.equal(competingUnpublishState.published_revision_id, null);
  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Retry publish"),
    "Retry publish button",
  );
  await assertVisibleFocus(page, "Retry publish button");
  const retryResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url() === publishUrl,
  );
  await pressKeyboard(page, "Enter");
  await responseJson(await retryResponsePromise, 200, "Retry publish with original key and body");
  await publicationPanel.getByRole("status").filter({ hasText: "Unpublished." }).waitFor();
  assert.equal(await titleInput.inputValue(), lostResponseDraftTitle);
  await page.getByText("Unsaved changes").waitFor();
  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Save revision"),
    "Save the draft preserved through publication retry",
  );
  const recoveredSavePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" &&
      response.url().endsWith(`/api/funnels/${funnelId}/revisions`),
  );
  await pressKeyboard(page, "Enter");
  await responseJson(await recoveredSavePromise, 200, "Save draft preserved through publication retry");
  await page.getByText("All changes saved").waitFor();
  const recoveredDraft = await responseJson(
    await fetch(`${worker.origin}/api/funnels/${funnelId}`),
    200,
    "Read draft after publication retry recovery",
  );
  assert.notEqual(recoveredDraft.base_revision_id, finalRevisionId);
  assert.equal(recoveredDraft.published_revision_id, null);
  const lostResponseDraftPreserved = (await titleInput.inputValue()) === lostResponseDraftTitle;
  assert.equal(lostResponseDraftPreserved, true);
  await page.unroute(publishUrl, lostResponseRoute);
  const republishButton = page.getByRole("button", { name: "Publish revision" });
  await republishButton.waitFor({ state: "visible" });
  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Publish revision"),
    "Publish the recovered local draft",
  );
  const republishResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url() === publishUrl,
  );
  await pressKeyboard(page, "Enter");
  await responseJson(await republishResponsePromise, 200, "Publish the recovered local draft");
  const republishedDraft = await responseJson(
    await fetch(`${worker.origin}/api/funnels/${funnelId}`),
    200,
    "Read current publication after republishing the recovered draft",
  );
  assert.equal(republishedDraft.published_revision_id, recoveredDraft.base_revision_id);
  await publicationPanel
    .getByRole("status")
    .filter({ hasText: `Published revision ${recoveredDraft.base_revision_id}` })
    .waitFor();
  const dashboardPointerEvents = await page.evaluate(() => window.__pr022PointerEvents);
  assert.equal(dashboardPointerEvents, 0);
  const dashboardKeyboardOnly = keyboardActions.length > 0 && dashboardPointerEvents === 0;
  assert.equal(dashboardKeyboardOnly, true, "Dashboard create/edit/save/publish actions must use keyboard input");

  const publicResponse = await page.goto(`${worker.origin}/f/${slug}`, { waitUntil: "networkidle" });
  assert.equal(publicResponse?.status(), 200);
  await page.getByRole("heading", { name: lostResponseDraftTitle }).waitFor();
  await assertCriticalAxeFindingsAbsent(page, "Published funnel");
  await page.goBack({ waitUntil: "networkidle" });
  await publicationPanel.locator(".editor-publication-facts p").nth(1).getByText(recoveredDraft.base_revision_id).waitFor();
  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Unpublish"),
    "Unpublish button",
  );
  await assertVisibleFocus(page, "Unpublish button");
  await pressKeyboard(page, "Enter");
  const unpublishDialog = page.getByRole("dialog", { name: "Unpublish this funnel?" });
  await unpublishDialog.waitFor({ state: "visible" });
  await assertVisibleFocus(page, "Unpublish confirmation cancel button");
  await pressKeyboard(page, "Escape");
  await unpublishDialog.waitFor({ state: "hidden" });
  await publicationPanel.locator(".editor-publication-facts p").nth(1).getByText(recoveredDraft.base_revision_id).waitFor();

  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Unpublish"),
    "Unpublish button for pointer cancel flow",
  );
  await pressKeyboard(page, "Enter");
  await unpublishDialog.waitFor({ state: "visible" });
  await unpublishDialog.getByRole("button", { name: "Cancel" }).click();
  await unpublishDialog.waitFor({ state: "hidden" });
  const pointerEventsAfterCancel = await page.evaluate(() => window.__pr022PointerEvents);
  assert.ok(pointerEventsAfterCancel > 0, "The confirmation dialog must support pointer input");
  await publicationPanel.locator(".editor-publication-facts p").nth(1).getByText(recoveredDraft.base_revision_id).waitFor();

  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Unpublish"),
    "Unpublish button for confirmed flow",
  );
  await pressKeyboard(page, "Enter");
  await unpublishDialog.waitFor({ state: "visible" });
  await unpublishDialog.getByText(/This removes the live revision pointer/).waitFor();
  await tabUntil(
    page,
    (active) => active.tag === "BUTTON" && hasName(active, "Confirm unpublish"),
    "Confirm unpublish button",
  );
  await assertVisibleFocus(page, "Confirm unpublish button");
  const unpublishResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST" && response.url().endsWith(`/api/funnels/${funnelId}/unpublish`),
  );
  await pressKeyboard(page, "Enter");
  await responseJson(await unpublishResponsePromise, 200, "Confirm unpublish");
  await publicationPanel.getByRole("status").filter({ hasText: "Unpublished." }).waitFor();
  const publicAfterUnpublish = await page.goto(`${worker.origin}/f/${slug}`, { waitUntil: "networkidle" });
  assert.equal(publicAfterUnpublish?.status(), 404);
  const finalPointerEvents = await page.evaluate(() => window.__pr022PointerEvents);
  assert.deepEqual(diagnostics, [], `Browser errors: ${diagnostics.join("\n")}`);
  console.log(
    JSON.stringify({
      workflow: "create V2 in dashboard -> seed existing V1 -> edit -> save -> stale publish conflict -> preserve draft -> save -> publish -> lose response while editing -> exact retry -> save preserved draft -> public read -> confirm unpublish -> 404",
      createdV2FunnelId,
      funnelId,
      slug,
      initialRevisionId,
      firstSavedRevisionId,
      finalRevisionId,
      recoveredDraftRevisionId: recoveredDraft.base_revision_id,
      dashboardKeyboardOnly,
      keyboardScope: "dashboard create/edit/save/publish/conflict recovery/unpublish",
      keyboardActionCount: keyboardActions.length,
      publicationMethod: "dashboard UI",
      staleRequests: 2,
      lostResponseDraftPreserved,
      lostResponseRetrySameRequest: lostResponseRequest?.key === idempotentRetryRequest?.key &&
        lostResponseRequest?.body === idempotentRetryRequest?.body,
      pointerCancelEvents: pointerEventsAfterCancel,
      finalPointerEvents,
      axeCritical: { dashboard: 0, editor: 0, publishedFunnel: 0 },
    }),
  );
} finally {
  await finalizePr019Browser(closeBrowser, worker);
}
