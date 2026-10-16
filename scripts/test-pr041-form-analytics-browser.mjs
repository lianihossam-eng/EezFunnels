import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { chromium } from "@playwright/test";

const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
const analyticsUrl = process.argv[2];
assert(analyticsUrl, "Pass the local funnel analytics URL");

const executablePath = await access("/usr/bin/chromium").then(
  () => "/usr/bin/chromium",
  () => undefined,
);
const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
try {
  const page = await browser.newPage();
  page.on("response", async (response) => {
    if (response.url().includes("/analytics/forms")) {
      console.log(`Analytics response ${response.status()} ${response.url()}`);
    }
  });
  await page.goto(analyticsUrl, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Form completions" }).waitFor();
  await page.getByRole("table", { name: "Daily form completions (UTC)" }).waitFor();
  const period = page.getByRole("combobox", { name: "Reporting period" });
  const total = page.locator(".analytics-total-number");
  await assertEventually(async () => assert.equal(await total.innerText(), "105"));
  if (process.env.PR041_SCREENSHOT_PATH) {
    await page.screenshot({ path: process.env.PR041_SCREENSHOT_PATH, fullPage: true });
  }
  assert.equal(
    await page.getByRole("table", { name: "Daily form completions (UTC)" }).locator("tbody tr").count(),
    30,
  );
  const formTable = page.getByRole("table", { name: "By page and form purpose" });
  assert.equal(await formTable.locator("tbody tr").count(), 1);
  assert.equal(await formTable.locator("tbody tr td").nth(0).innerText(), "Home");
  assert.ok((await formTable.locator("tbody tr td").nth(1).innerText()).trim().length > 0);
  assert.equal(await page.getByText("A conversion rate", { exact: false }).count(), 0);

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

  await page.route(/\/analytics\/forms\?days=7$/, async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 300));
    await route.continue();
  });
  await period.focus();
  await period.selectOption("7");
  await period.selectOption("90");
  await page.waitForFunction(() => {
    const selected = document.querySelector("#form-analytics-period")?.value;
    const days = document.querySelector(".analytics-tables .lead-table-wrap table")
      ?.querySelectorAll("tbody tr").length;
    return selected === "90" && days === 90;
  }, { timeout: 10_000 }).catch(async (error) => {
    console.log({
      selected: await period.inputValue(),
      dayRows: await page.locator(".analytics-tables .lead-table-wrap table tbody tr").count(),
      total: await total.innerText().catch(() => "missing"),
      status: await page.locator("#form-analytics-live-status").innerText(),
      error: await page.locator('[role="alert"]').innerText().catch(() => "missing"),
    });
    throw error;
  });
  assert.equal(await page.evaluate(() => document.activeElement?.id), "form-analytics-period");
  await page.waitForTimeout(400);
  assert.equal(await period.inputValue(), "90");
  assert.equal(await total.innerText(), "105");

  await period.selectOption("7");
  await page.waitForFunction(() => {
    return document.querySelector("#form-analytics-period")?.value === "7"
      && document.querySelector(".analytics-tables .lead-table-wrap table")
        ?.querySelectorAll("tbody tr").length === 7;
  });
  assert.equal(await page.evaluate(() => document.activeElement?.id), "form-analytics-period");
  assert.equal(await total.innerText(), "105");

  console.log("PR-041 browser form analytics, period switching, stale-response handling, focus, and accessibility passed");
} finally {
  await browser.close();
}

async function assertEventually(assertion) {
  const deadline = Date.now() + 10_000;
  let lastError;
  while (Date.now() < deadline) {
    try {
      await assertion();
      return;
    } catch (error) {
      lastError = error;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
  }
  throw lastError;
}
