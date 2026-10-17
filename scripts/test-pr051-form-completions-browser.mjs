import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { chromium } from "@playwright/test";

const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
const pageUrl = process.argv[2];
const formId = process.argv[3];
const emptyPageUrl = process.argv[4];
assert(pageUrl, "Pass the local funnel completions dashboard URL");
assert(formId, "Pass the form ID expected in the report");
assert(emptyPageUrl, "Pass the empty funnel completions dashboard URL");

const executablePath = await access("/usr/bin/chromium").then(
  () => "/usr/bin/chromium",
  () => undefined,
);
const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  await page.goto(pageUrl, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Form completions", level: 1 }).waitFor({ state: "visible" });
  const dailyTable = page.locator("table.completion-table:not(.form-completion-table)");
  const dailyRows = dailyTable.locator("tbody tr");
  await dailyRows.first().waitFor({ state: "visible" });
  assert.equal(await dailyRows.count(), 30);
  assert.equal(await page.locator(".completion-total strong").textContent(), "28");
  assert.equal(await page.getByRole("button", { name: "30 days" }).getAttribute("aria-pressed"), "true");
  assert.equal(await page.getByText(`Form ID ${formId}`, { exact: true }).count(), 1);
  assert.equal(await page.getByText("Today (partial) · ", { exact: false }).count(), 1);

  await page.getByRole("button", { name: "7 days" }).click();
  await page.waitForFunction(() => document.querySelectorAll("table.completion-table:not(.form-completion-table) tbody tr").length === 7);
  assert.equal(await page.locator(".completion-total strong").textContent(), "28");
  assert.equal(await page.getByRole("button", { name: "7 days" }).getAttribute("aria-pressed"), "true");

  await page.getByRole("button", { name: "90 days" }).click();
  await page.waitForFunction(() => document.querySelectorAll("table.completion-table:not(.form-completion-table) tbody tr").length === 90);
  assert.equal(await page.locator(".completion-total strong").textContent(), "28");
  assert.equal(await dailyRows.last().getByText("Today (partial) · ", { exact: false }).count(), 1);

  await page.setViewportSize({ width: 390, height: 844 });
  const width = await page.evaluate(() => ({ scroll: document.documentElement.scrollWidth, viewport: window.innerWidth }));
  assert.ok(width.scroll <= width.viewport, JSON.stringify(width));

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

  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto(emptyPageUrl, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Form completions", level: 1 }).waitFor({ state: "visible" });
  await page.getByText("No retained form submissions in this period.", { exact: true }).waitFor({ state: "visible" });
  assert.equal(await page.locator(".completion-total strong").textContent(), "0");
  const emptyDailyRows = page.locator("table.completion-table:not(.form-completion-table) tbody tr");
  await emptyDailyRows.first().waitFor({ state: "visible" });
  assert.equal(await emptyDailyRows.count(), 30);
  assert.equal(await page.locator(".completion-number").allTextContents().then((values) => values.every((value) => value.trim() === "0")), true);

  await page.addScriptTag({ content: axeSource });
  const emptyViolations = await page.evaluate(async () => {
    const result = await window.axe.run(document, {
      runOnly: {
        type: "tag",
        values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"],
      },
    });
    return result.violations.filter(({ impact }) => impact === "critical" || impact === "serious");
  });
  assert.deepEqual(emptyViolations, [], JSON.stringify(emptyViolations));
  if (process.env.PR051_SCREENSHOT) {
    await page.screenshot({ path: process.env.PR051_SCREENSHOT, fullPage: true });
  }
  console.log("PR-051 completion periods, totals, responsive layout, and accessibility passed");
} finally {
  await browser.close();
}
