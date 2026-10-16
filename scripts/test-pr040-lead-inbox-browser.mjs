import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { chromium } from "@playwright/test";

const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
const inboxUrl = process.argv[2];
const submissionId = process.argv[3];
assert(inboxUrl, "Pass the local funnel leads URL");
assert(submissionId, "Pass a local submission id for the detail check");

const executablePath = await access("/usr/bin/chromium").then(
  () => "/usr/bin/chromium",
  () => undefined,
);
const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
try {
  const page = await browser.newPage();
  await page.goto(inboxUrl, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "PR-033 local form smoke leads" }).waitFor();

  const rows = page.locator(".lead-table tbody tr");
  await page.getByRole("table", { name: "Recent form submissions" }).waitFor();
  assert.equal(await rows.count(), 50);
  assert.equal(await rows.first().getByRole("link").count(), 1);
  const firstRow = rows.first();
  assert.equal(await firstRow.locator("td").nth(1).innerText(), "Home");
  assert.ok((await firstRow.locator("td").nth(2).innerText()).trim().length > 0);

  const detailLink = page.locator(`a[href$="/leads/${submissionId}"]`).first();
  assert.equal(await detailLink.count(), 1);
  await detailLink.click();
  await page.getByRole("heading", { name: "Submission details" }).waitFor();
  await page.getByRole("heading", { name: "Consent evidence" }).waitFor();
  assert.equal(await page.getByText("I agree to be contacted about this request.", { exact: true }).count(), 1);
  assert.equal(await page.getByText("Send me occasional product news.", { exact: true }).count(), 1);
  assert.equal(await page.getByText("Not opted in", { exact: true }).count(), 1);
  assert.equal(await page.getByText("<img src=x onerror=alert(1)>", { exact: true }).count(), 1);
  assert.equal(await page.locator(".lead-detail-panel img").count(), 0);

  await page.addScriptTag({ content: axeSource });
  const violations = await page.evaluate(async () => {
    const result = await window.axe.run(document, {
      runOnly: {
        type: "tag",
        values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"],
      },
    });
    return result.violations.filter(({ impact }) => impact === "critical" || impact === "serious");
  });
  assert.deepEqual(violations, [], JSON.stringify(violations));

  await page.getByRole("link", { name: "Back to leads" }).click();
  await page.getByRole("table", { name: "Recent form submissions" }).waitFor();
  const loadMore = page.getByRole("button", { name: "Load more leads" });
  const buttonHandle = await loadMore.elementHandle();
  assert(buttonHandle);
  await loadMore.focus();
  assert.equal(await page.evaluate((element) => document.activeElement === element, buttonHandle), true);
  await loadMore.click();
  await page.waitForFunction(() => document.querySelectorAll(".lead-table tbody tr").length === 100);
  assert.equal(await page.evaluate(() => document.activeElement?.id), "lead-load-more-button");
  const loadFinalPage = page.getByRole("button", { name: "Load more leads" });
  assert.equal(await loadFinalPage.isDisabled(), false);
  await loadFinalPage.click();
  await page.waitForFunction(() => document.querySelectorAll(".lead-table tbody tr").length === 105);
  await page.getByText("You’ve reached the end of recent leads.", { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => document.activeElement?.id), "lead-end-status");
  assert.equal(await page.getByRole("button", { name: "All recent leads loaded" }).isDisabled(), true);

  console.log("PR-040 browser lead inbox, detail, safe text rendering, paging focus, and accessibility passed");
} finally {
  await browser.close();
}
