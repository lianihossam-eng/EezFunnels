import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { chromium } from "@playwright/test";

const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
const pageUrl = process.argv[2];
assert(pageUrl, "Pass the local submissions dashboard URL");

const executablePath = await access("/usr/bin/chromium").then(
  () => "/usr/bin/chromium",
  () => undefined,
);
const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  await page.goto(pageUrl, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Submissions", level: 1 }).waitFor({ state: "visible" });
  const entries = page.locator("details.submission-entry");
  await entries.first().waitFor({ state: "visible" });
  assert.equal(await entries.count(), 25);

  const graceEntry = entries.filter({ hasText: "Grace Hopper" });
  assert.equal(await graceEntry.count(), 1);
  await graceEntry.locator("summary").click();
  await page.getByText("Grace Hopper", { exact: true }).waitFor({ state: "visible" });
  await page.getByRole("heading", { name: "Consent record" }).first().waitFor({ state: "visible" });
  const consentCopy = graceEntry.getByText("I agree to be contacted about this updated request.", { exact: true });
  await consentCopy.waitFor({ state: "visible" });
  assert.equal(await consentCopy.count(), 1);
  assert.equal(await graceEntry.getByText("Available until", { exact: false }).count(), 1);

  const graceStatus = graceEntry.getByRole("combobox", {
    name: /^Follow-up status for/,
  });
  assert.equal(await graceStatus.count(), 1);
  const statusUpdate = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      response.url().includes("/api/submissions/") &&
      response.url().endsWith("/status"),
  );
  await graceStatus.selectOption("qualified");
  const statusResponse = await statusUpdate;
  const statusBody = await statusResponse.text();
  assert.equal(statusResponse.status(), 200, statusBody);

  const statusFilter = page.getByRole("combobox", { name: "Filter submissions by status" });
  const contactedList = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().includes("/api/submissions?status=contacted"),
  );
  await statusFilter.selectOption("contacted");
  await contactedList;
  await page.getByRole("heading", { name: "No contacted submissions" }).waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 0);
  assert.equal(await page.getByText("Grace Hopper", { exact: true }).count(), 0);

  const qualifiedList = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().includes("/api/submissions?status=qualified"),
  );
  await statusFilter.selectOption("qualified");
  await qualifiedList;
  await page.locator("details.submission-entry .submission-status-badge").getByText("Qualified", { exact: true }).waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 1);
  const qualifiedEntry = page.locator("details.submission-entry").filter({
    has: page.locator(".submission-status-badge").getByText("Qualified", { exact: true }),
  });
  await qualifiedEntry.locator("summary").click();
  await qualifiedEntry.getByText("Grace Hopper", { exact: true }).waitFor({ state: "visible" });

  const closedList = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().includes("/api/submissions?status=closed"),
  );
  await statusFilter.selectOption("closed");
  await closedList;
  await page.getByRole("heading", { name: "No closed submissions" }).waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 0);

  const newList = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().includes("/api/submissions?status=new"),
  );
  await statusFilter.selectOption("new");
  await newList;
  await page.getByRole("button", { name: "Load older submissions" }).waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 25);
  await page.getByRole("button", { name: "Load older submissions" }).click();
  await page.getByText("All retained submissions are shown.").waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 27);

  const allList = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().endsWith("/api/submissions"),
  );
  await statusFilter.selectOption("all");
  await allList;
  await page.getByRole("button", { name: "Load older submissions" }).waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 25);

  await page.getByRole("button", { name: "Load older submissions" }).click();
  await page.getByText("All retained submissions are shown.").waitFor({ state: "visible" });
  assert.equal(await entries.count(), 28);
  assert.equal(await page.getByText("Inbox fixture 01", { exact: true }).count(), 1);

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
  if (process.env.PR050_SCREENSHOT) {
    await page.screenshot({ path: process.env.PR050_SCREENSHOT, fullPage: true });
  }
  console.log("PR-050 submissions browser triage, filtering, pagination, consent, responsive layout, and accessibility passed");
} finally {
  await browser.close();
}
