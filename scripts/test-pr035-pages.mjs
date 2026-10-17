import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { chromium } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startPr019Worker } from "./pr019-local-worker.mjs";

const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
let browser;
let worker;
const pageErrors = [];

async function jsonResponse(response, expectedStatus, label) {
  const text = await response.text();
  const status = typeof response.status === "function" ? response.status() : response.status;
  assert.equal(status, expectedStatus, `${label}: ${text}`);
  return JSON.parse(text);
}

async function assertNoSeriousAxeFindings(page, label) {
  await page.addScriptTag({ content: axeSource });
  const violations = await page.evaluate(async () => {
    const result = await window.axe.run(document, {
      runOnly: {
        type: "tag",
        values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"],
      },
    });
    return result.violations
      .filter(({ impact }) => impact === "serious" || impact === "critical")
      .map(({ id, impact, help, nodes }) => ({
        id,
        impact,
        help,
        targets: nodes.map(({ target }) => target),
      }));
  });
  assert.deepEqual(violations, [], `${label}: ${JSON.stringify(violations)}`);
}

try {
  worker = await startPr019Worker("pr035-pages", {
    onSignalCleanup: async () => browser?.close(),
  });
  const chromiumPath = await access("/usr/bin/chromium").then(
    () => "/usr/bin/chromium",
    () => undefined,
  );
  browser = await chromium.launch({
    headless: true,
    ...(chromiumPath ? { executablePath: chromiumPath } : {}),
  });
  const page = await browser.newPage({ viewport: { width: 1360, height: 1000 } });
  page.on("pageerror", (error) => pageErrors.push(error.stack ?? error.message));

  const createResponse = await fetch(`${worker.origin}/api/funnels/v2`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": `pr035-pages-${randomUUID()}`,
    },
    body: JSON.stringify({ name: "PR-035 Page management", blueprint: "lead_gen" }),
  });
  const created = await jsonResponse(createResponse, 201, "Create V2 funnel");
  const funnelId = created.funnel?.id;
  const funnelSlug = created.funnel?.slug;
  assert.equal(typeof funnelId, "string");
  assert.equal(typeof funnelSlug, "string");

  await page.goto(`${worker.origin}/funnels/${funnelId}`, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "PR-035 Page management" }).waitFor();
  const pageTitle = page.getByRole("textbox", { name: "Page title" });
  await pageTitle.waitFor();

  const addPageResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith(`/api/funnels/${funnelId}/edit-v2`),
  );
  await page.getByRole("button", { name: "Add page" }).click();
  const addedPage = await jsonResponse(await addPageResponsePromise, 200, "Add page");
  const addedDocument = JSON.parse(addedPage.document_json);
  assert.equal(addedDocument.pages.length, 2);
  const addedPageId = addedDocument.pages[1].id;
  assert.equal(addedDocument.pages[1].title, "Step 2");
  assert.equal(addedDocument.pages[1].slug, "step-2");
  assert.ok(
    addedDocument.pages[0].sections.some((section) =>
      section.blocks.some(
        (block) =>
          block.kind === "button" &&
          block.action.kind === "navigate_page" &&
          block.action.page_id === addedPageId,
      ),
    ),
    "Adding a page must make it reachable with a button from the selected page",
  );

  const pageSelector = page.getByRole("combobox", { name: "Current page" });
  await pageSelector.waitFor();
  assert.equal(await pageSelector.inputValue(), addedPageId);
  const slugInput = page.getByRole("textbox", { name: "Page slug" });
  await slugInput.fill("Invalid slug");
  await page.getByText("Use a lowercase page slug with letters, numbers, and hyphens.").waitFor();
  assert.equal(await pageTitle.getAttribute("aria-invalid"), "false");
  assert.equal(await slugInput.getAttribute("aria-invalid"), "true");
  assert.equal(await page.getByRole("button", { name: "Save revision" }).isDisabled(), true);
  assert.equal(await page.getByRole("button", { name: "Publish saved revision" }).isDisabled(), true);

  await pageTitle.fill("Checkout");
  await slugInput.fill("checkout");
  const updatePageResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith(`/api/funnels/${funnelId}/edit-v2`),
  );
  await page.getByRole("button", { name: "Apply page settings" }).click();
  const updatedPage = await jsonResponse(await updatePageResponsePromise, 200, "Update page settings");
  const updatedDocument = JSON.parse(updatedPage.document_json);
  assert.equal(updatedDocument.pages[1].title, "Checkout");
  assert.equal(updatedDocument.pages[1].slug, "checkout");
  await assertNoSeriousAxeFindings(page, "Page editor");

  const saveRevisionResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith(`/api/funnels/${funnelId}/revisions-v2`),
  );
  await page.getByRole("button", { name: "Save revision" }).click();
  await jsonResponse(await saveRevisionResponsePromise, 201, "Save page revision");
  await page.getByText("All changes saved").waitFor();

  const publishResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith(`/api/funnels/${funnelId}/publish`),
  );
  await page.getByRole("button", { name: "Publish saved revision" }).click();
  await jsonResponse(await publishResponsePromise, 200, "Publish multi-page funnel");
  const publicPage = await fetch(`${worker.origin}/f/${funnelSlug}/checkout`);
  assert.equal(publicPage.status, 200);
  assert.match(await publicPage.text(), /<title>Checkout<\/title>/);

  await page.getByRole("button", { name: "Delete page Checkout" }).click();
  const deleteDialog = page.getByRole("dialog", { name: "Delete Checkout?" });
  await deleteDialog.waitFor({ state: "visible" });
  const deletePageResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith(`/api/funnels/${funnelId}/edit-v2`),
  );
  await deleteDialog.getByRole("button", { name: "Delete page", exact: true }).click();
  const deletedPage = await jsonResponse(await deletePageResponsePromise, 200, "Delete page");
  const deletedDocument = JSON.parse(deletedPage.document_json);
  assert.equal(deletedDocument.pages.length, 1);
  assert.ok(
    !deletedDocument.pages.some((candidate) => candidate.id === addedPageId),
    "Deleted page must be absent from the draft",
  );
  assert.ok(
    !deletedDocument.pages.some((candidate) =>
      candidate.sections.some((section) =>
        section.blocks.some(
          (block) =>
            block.kind === "button" &&
            block.action.kind === "navigate_page" &&
            block.action.page_id === addedPageId,
        ),
      ),
    ),
    "Deleting a page must remove inbound navigation buttons",
  );

  const saveDeletedRevisionPromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith(`/api/funnels/${funnelId}/revisions-v2`),
  );
  await page.getByRole("button", { name: "Save revision" }).click();
  await jsonResponse(await saveDeletedRevisionPromise, 201, "Save deleted page revision");
  await page.getByText("All changes saved").waitFor();

  const republishResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().endsWith(`/api/funnels/${funnelId}/publish`),
  );
  await page.getByRole("button", { name: "Publish saved revision" }).click();
  await jsonResponse(await republishResponsePromise, 200, "Publish page deletion");
  const removedPageResponse = await fetch(`${worker.origin}/f/${funnelSlug}/checkout`);
  assert.equal(removedPageResponse.status, 404);
  assert.deepEqual(pageErrors, [], `Browser errors: ${pageErrors.join("\n")}`);
  console.log("PR-035 multi-page editor workflow passed");
} finally {
  await browser?.close();
  await worker?.stop();
}
