import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { chromium } from "@playwright/test";

const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
const pageUrl = process.argv[2];
const funnelId = process.argv[3];
const emptyFunnelId = process.argv[4];
assert(pageUrl, "Pass the local submissions dashboard URL");
assert(funnelId && emptyFunnelId, "Pass the matching and empty funnel IDs");

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

  const funnelFilter = page.getByRole("combobox", { name: "Filter submissions by funnel" });
  const scopedList = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?funnel_id=${funnelId}`),
  );
  await funnelFilter.selectOption(funnelId);
  await scopedList;
  await page.locator("details.submission-entry").first().waitFor({ state: "visible" });
  assert.equal(await entries.count(), 25);
  assert.ok((await entries.first().innerText()).includes("PR-033 local form smoke"));

  const combinedFilters = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?status=qualified&funnel_id=${funnelId}`),
  );
  await statusFilter.selectOption("qualified");
  await combinedFilters;
  assert.equal(await entries.count(), 1);
  await entries.filter({ hasText: "Grace Hopper" }).waitFor({ state: "visible" });

  const filteredPages = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?status=new&funnel_id=${funnelId}`),
  );
  await statusFilter.selectOption("new");
  await filteredPages;
  assert.equal(await entries.count(), 25);
  const filteredOlderPage = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?status=new&funnel_id=${funnelId}&cursor=`),
  );
  await page.getByRole("button", { name: "Load older submissions" }).click();
  await filteredOlderPage;
  await page.getByText("All retained submissions are shown.").waitFor({ state: "visible" });
  assert.equal(await entries.count(), 27);

  const resetToFunnel = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?funnel_id=${funnelId}`),
  );
  await statusFilter.selectOption("all");
  await resetToFunnel;
  await page.waitForFunction(() => document.querySelectorAll("details.submission-entry").length === 25);
  assert.equal(await entries.count(), 25);

  const emptyFunnelList = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?funnel_id=${emptyFunnelId}`),
  );
  await funnelFilter.selectOption(emptyFunnelId);
  await emptyFunnelList;
  await page.getByRole("heading", { name: "No submissions for this funnel" }).waitFor({ state: "visible" });
  assert.equal(await entries.count(), 0);

  const resetAllFunnels = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().endsWith("/api/submissions"),
  );
  await funnelFilter.selectOption("all");
  await resetAllFunnels;
  await page.locator("details.submission-entry").first().waitFor({ state: "visible" });
  assert.equal(await entries.count(), 25);

  await page.setViewportSize({ width: 320, height: 740 });
  const width = await page.evaluate(() => ({ scroll: document.documentElement.scrollWidth, viewport: window.innerWidth }));
  assert.ok(width.scroll <= width.viewport, JSON.stringify(width));
  const toolbarDisplay = await page.locator(".submission-toolbar").evaluate((toolbar) =>
    getComputedStyle(toolbar).display,
  );
  assert.equal(toolbarDisplay, "grid");
  const filterLayout = await page.locator(".submission-filter-group").evaluateAll((groups) =>
    groups.map((group) => ({
      top: group.getBoundingClientRect().top,
      groupWidth: group.getBoundingClientRect().width,
      columnCount: getComputedStyle(group).gridTemplateColumns.trim().split(/\s+/).length,
      selectWidth: group.querySelector("select").getBoundingClientRect().width,
      selectRight: group.querySelector("select").getBoundingClientRect().right,
    })),
  );
  assert.equal(filterLayout.length, 2, JSON.stringify(filterLayout));
  assert.ok(filterLayout[1].top > filterLayout[0].top, JSON.stringify(filterLayout));
  assert.ok(filterLayout.every(({ groupWidth, columnCount, selectWidth, selectRight }) =>
    columnCount === 1 && selectWidth >= groupWidth - 1 && selectRight <= width.viewport,
  ), JSON.stringify(filterLayout));

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

  const failurePage = await browser.newPage({ viewport: { width: 390, height: 844 } });
  let failFunnelList = true;
  await failurePage.route("**/api/funnels", async (route) => {
    if (failFunnelList) {
      await route.fulfill({ status: 503, contentType: "application/json", body: "{}" });
    } else {
      await route.continue();
    }
  });
  await failurePage.goto(pageUrl, { waitUntil: "networkidle" });
  await failurePage.getByText("Funnel filters could not be loaded.", { exact: false }).waitFor({ state: "visible" });
  assert.equal(await failurePage.getByRole("combobox", { name: "Filter submissions by funnel" }).isDisabled(), true);
  failFunnelList = false;
  const retriedFunnels = failurePage.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().endsWith("/api/funnels"),
  );
  await failurePage.getByRole("button", { name: "Retry funnel filters" }).click();
  const retryResponse = await retriedFunnels;
  assert.equal(retryResponse.status(), 200);
  await failurePage.waitForFunction(() => !document.getElementById("submission-funnel-filter")?.disabled);
  await failurePage.close();
  console.log("PR-050 submissions browser triage, filtering, pagination, consent, responsive layout, and accessibility passed");
} finally {
  await browser.close();
}
