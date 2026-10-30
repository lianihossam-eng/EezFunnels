import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { chromium } from "@playwright/test";

const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
const pageUrl = process.argv[2];
assert(pageUrl, "Pass the local published form URL");

const executablePath = await access("/usr/bin/chromium").then(
  () => "/usr/bin/chromium",
  () => undefined,
);
const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
try {
  const context = await browser.newContext();
  const page = await context.newPage();
  const interactionRequests = [];
  page.context().on("request", (request) => {
    if (new URL(request.url()).pathname.endsWith("/interaction-events")) {
      interactionRequests.push(request);
    }
  });
  await page.goto(pageUrl, { waitUntil: "networkidle" });

  const interactionPanel = page.getByRole("region", {
    name: "Interaction measurement choices",
  });
  await interactionPanel.waitFor({ state: "visible" });
  assert.equal(
    await page.evaluate(() => localStorage.getItem("eezfunnel:interaction-consent:v1")),
    null,
  );
  await page.waitForTimeout(250);
  assert.equal(interactionRequests.length, 0, "no event request before consent");
  await interactionPanel.getByRole("button", { name: "No thanks" }).click();
  await page.waitForTimeout(250);
  assert.equal(interactionRequests.length, 0, "refusal must not send events");
  assert.equal(
    await page.evaluate(() => localStorage.getItem("eezfunnel:interaction-consent:v1")),
    "refused",
  );
  await page.getByRole("button", { name: "Privacy choices" }).click();
  const pageViewResponse = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname.endsWith("/interaction-events") &&
      response.request().method() === "POST" &&
      response.request().postDataJSON()?.kind === "page_viewed",
  );
  await interactionPanel.getByRole("button", { name: "Allow measurement" }).click();
  assert.equal((await pageViewResponse).status(), 204);
  await page.waitForTimeout(100);
  assert.equal(
    await page.evaluate(() => localStorage.getItem("eezfunnel:interaction-consent:v1")),
    "granted",
  );
  const firstPageEvent = interactionRequests[0].postDataJSON();
  assert.equal(firstPageEvent.kind, "page_viewed");
  assert.equal(firstPageEvent.page_id, "page-home");
  assert.equal(firstPageEvent.revision_id.length > 0, true);
  assert.equal("utm_medium" in firstPageEvent, false);
  assert.equal("utm_campaign" in firstPageEvent, false);
  assert.equal("email" in firstPageEvent, false);

  const name = page.getByLabel("Name");
  const email = page.getByLabel("Email");
  const form = page.locator("form.v2-form");
  const actionUrl = new URL(await form.getAttribute("action"), pageUrl);
  const formId = actionUrl.pathname.match(/\/forms\/([^/]+)\/submissions$/)?.[1];
  assert.ok(formId, actionUrl.pathname);
  const marketingOptIn = page.locator('input[name="_marketing_opt_in"]');

  assert.equal(await name.count(), 1);
  assert.equal(await email.count(), 1);
  assert.equal(await marketingOptIn.isChecked(), false);
  assert.equal(await form.evaluate((element) => element.checkValidity()), false);
  await page.evaluate(() => document.activeElement?.blur());
  for (
    let attempts = 0;
    attempts < 20 && !(await name.evaluate((element) => element === document.activeElement));
    attempts += 1
  ) {
    await page.keyboard.press("Tab");
  }
  assert.equal(await name.evaluate((element) => element === document.activeElement), true);
  assert.equal(await name.evaluate((element) => getComputedStyle(element).outlineWidth), "3px");

  await name.fill("Grace Hopper");
  await email.fill("not-an-email");
  assert.equal(await form.evaluate((element) => element.checkValidity()), false);
  await email.fill("grace@example.test");
  await page.locator('input[name="_request_consent"]').check();
  assert.equal(await form.evaluate((element) => element.checkValidity()), true);

  const noScriptContext = await browser.newContext({ javaScriptEnabled: false });
  const recoveryPage = await noScriptContext.newPage();
  await recoveryPage.goto(pageUrl, { waitUntil: "networkidle" });
  const recoveryForm = recoveryPage.locator("form.v2-form");
  const recoveryAction = new URL(await recoveryForm.getAttribute("action"), pageUrl);
  const recoveryKey = recoveryAction.searchParams.get("key");
  assert.ok(recoveryKey);
  const recoveryEmail = recoveryPage.getByLabel("Email");
  await recoveryEmail.evaluate((element) => {
    element.type = "text";
    element.required = false;
  });
  await recoveryPage.getByLabel("Name").fill("\nGrace Hopper");
  await recoveryEmail.fill("not-an-email");
  await recoveryPage.locator('input[name="_request_consent"]').check();
  const validationResponsePromise = recoveryPage.waitForResponse(
    (response) =>
      new URL(response.url()).pathname.endsWith("/submissions") &&
      response.request().method() === "POST",
  );
  await recoveryPage.getByRole("button", { name: "Send" }).click();
  const validationResponse = await validationResponsePromise;
  assert.equal(validationResponse.status(), 422);
  await recoveryPage.waitForLoadState("load");
  const recoveryMarkup = await recoveryPage.content();
  const summaryId = `form-${formId}-errors`;
  await recoveryPage.waitForFunction(
    (id) => document.activeElement?.id === id,
    summaryId,
  );
  const recoverySummary = recoveryPage.locator(`#${summaryId}`);
  assert.equal(await recoverySummary.getAttribute("autofocus"), "");
  assert.equal(await recoverySummary.getAttribute("role"), "alert");
  assert.equal(await recoveryPage.getByLabel("Name").inputValue(), "\nGrace Hopper");
  assert.equal(await recoveryEmail.inputValue(), "not-an-email");
  const retryAction = new URL(
    await recoveryPage.locator("form.v2-form").getAttribute("action"),
    pageUrl,
  );
  assert.equal(retryAction.searchParams.get("key"), recoveryKey);
  const recoveryEmailId = await recoveryEmail.getAttribute("id");
  await recoveryPage.getByRole("link", { name: /Email: Enter a valid value/ }).click();
  assert.equal(new URL(recoveryPage.url()).hash, `#${recoveryEmailId}`);
  assert.equal(
    await recoveryEmail.evaluate((element) => element === document.activeElement),
    true,
  );
  await recoveryEmail.fill("grace@example.test");
  const successfulResponsePromise = recoveryPage.waitForResponse(
    (response) =>
      new URL(response.url()).pathname.endsWith("/submissions") &&
      response.request().method() === "POST",
  );
  await recoveryPage.getByRole("button", { name: "Send" }).click();
  const successfulResponse = await successfulResponsePromise;
  assert.equal(successfulResponse.status(), 303);
  await recoveryPage.waitForURL((url) => url.hash === `#form-success-${formId}`);
  const successMarkup = await recoveryPage.content();
  const status = recoveryPage.getByRole("status");
  await status.waitFor({ state: "visible" });
  assert.equal(
    await status.evaluate((element) => element === document.activeElement),
    true,
  );
  assert.equal(await recoveryPage.locator('input[name="_marketing_opt_in"]').count(), 1);
  const recoveryName = recoveryPage.getByLabel("Name");
  const nameLabel = recoveryPage.locator(
    `label[for="${await recoveryName.getAttribute("id")}"]`,
  );
  assert.equal(await nameLabel.evaluate((element) => getComputedStyle(element).textAlign), "left");
  const [formBox, statusBox] = await Promise.all([
    recoveryForm.evaluate((element) => element.getBoundingClientRect().toJSON()),
    status.evaluate((element) => element.getBoundingClientRect().toJSON()),
  ]);
  assert.ok(Math.abs(formBox.width - statusBox.width) < 1, { formBox, statusBox });
  assert.ok(statusBox.top >= formBox.bottom + 16, { formBox, statusBox });
  const accessibilityPage = await page.context().newPage();
  for (const [index, markup] of [recoveryMarkup, successMarkup].entries()) {
    await accessibilityPage.setContent(markup, { waitUntil: "load" });
    if (index === 1) {
      await accessibilityPage.evaluate((fragment) => {
        window.location.hash = fragment;
      }, `form-success-${formId}`);
      await accessibilityPage.getByRole("status").waitFor({ state: "visible" });
    }
    await accessibilityPage.addScriptTag({ content: axeSource });
    const violations = await accessibilityPage.evaluate(async () => {
      const result = await window.axe.run(document, {
        runOnly: {
          type: "tag",
          values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"],
        },
      });
      return result.violations.filter(({ impact }) => impact === "critical" || impact === "serious");
    });
    assert.deepEqual(violations, [], JSON.stringify(violations));
  }
  await accessibilityPage.close();
  if (process.env.PR033_SCREENSHOT) {
    await recoveryPage.screenshot({ path: process.env.PR033_SCREENSHOT, fullPage: true });
  }
  assert.ok(
    interactionRequests.some((request) => request.postDataJSON()?.kind === "form_viewed"),
    "visible form should be measured after grant",
  );
  assert.ok(
    interactionRequests.some((request) => request.postDataJSON()?.kind === "form_started"),
    "form start should be measured after grant",
  );
  assert.ok(
    interactionRequests.every(
      (request) => !("name" in request.postDataJSON()) && !("email" in request.postDataJSON()),
    ),
    "interaction payloads must not contain form answers",
  );

  const secondPage = await page.context().newPage();
  const secondPageViewResponse = secondPage.waitForResponse(
    (response) =>
      new URL(response.url()).pathname.endsWith("/interaction-events") &&
      response.request().postDataJSON()?.kind === "page_viewed",
  );
  await secondPage.goto(pageUrl, { waitUntil: "networkidle" });
  assert.equal((await secondPageViewResponse).status(), 204);
  const requestsBeforeWithdrawal = interactionRequests.length;
  const secondPageStorageUpdate = secondPage.evaluate(
    () =>
      new Promise((resolve) => {
        const onStorage = (event) => {
          if (event.key !== "eezfunnel:interaction-consent:v1") return;
          window.removeEventListener("storage", onStorage);
          resolve(event.newValue);
        };
        window.addEventListener("storage", onStorage);
      }),
  );
  await page.getByRole("button", { name: "Privacy choices" }).click();
  await interactionPanel.getByRole("button", { name: "No thanks" }).click();
  assert.equal(await secondPageStorageUpdate, "refused");
  await secondPage.getByLabel("Name").fill("Not measured after withdrawal");
  await secondPage.waitForTimeout(250);
  assert.equal(
    interactionRequests.length,
    requestsBeforeWithdrawal,
    "withdrawal in one tab must stop collection in every same-origin tab",
  );
  console.log("PR-067 consented interaction events and PR-033 browser forms passed");
} finally {
  await browser.close();
}
