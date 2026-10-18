import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { chromium } from "@playwright/test";

const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
const reportUrl = process.argv[2];
assert(reportUrl, "Pass the local funnel interaction report URL");

const executablePath = await access("/usr/bin/chromium").then(
  () => "/usr/bin/chromium",
  () => undefined,
);
const browser = await chromium.launch({
  headless: true,
  ...(executablePath ? { executablePath } : {}),
});

try {
  const page = await browser.newPage();
  const reportResponses = [];
  page.on("response", (response) => {
    if (new URL(response.url()).pathname.endsWith("/interaction-report")) {
      reportResponses.push(response);
    }
  });
  await page.goto(reportUrl, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Interaction counts" }).waitFor();
  await page.getByText(/retained interaction events in the selected period/).waitFor();
  await page.getByRole("columnheader", { name: "Pages viewed" }).waitFor();
  await page.getByRole("heading", { name: "By campaign ID" }).waitFor();
  await page.getByRole("rowheader", { name: "Unattributed" }).waitFor();
  assert.equal(await page.locator(".interaction-daily-table tbody tr").count(), 30);
  assert.equal(await page.getByRole("button", { name: "30 days" }).getAttribute("aria-pressed"), "true");
  assert.ok(reportResponses.some((response) => response.status() === 200));

  const sevenDayResponse = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname.endsWith("/interaction-report") &&
      new URL(response.url()).searchParams.get("period") === "7",
  );
  await page.getByRole("button", { name: "7 days" }).click();
  assert.equal((await sevenDayResponse).status(), 200);
  await page.getByRole("button", { name: "7 days" }).waitFor();
  await page.waitForFunction(
    () => document.querySelectorAll(".interaction-daily-table tbody tr").length === 7,
  );
  assert.equal(await page.getByRole("button", { name: "7 days" }).getAttribute("aria-pressed"), "true");

  const ninetyDayResponse = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname.endsWith("/interaction-report") &&
      new URL(response.url()).searchParams.get("period") === "90",
  );
  await page.getByRole("button", { name: "90 days" }).click();
  assert.equal((await ninetyDayResponse).status(), 200);
  await page.waitForFunction(
    () => document.querySelectorAll(".interaction-daily-table tbody tr").length === 90,
  );
  assert.equal(await page.getByRole("button", { name: "90 days" }).getAttribute("aria-pressed"), "true");

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
  console.log("PR-069 interaction report dashboard navigation and accessibility passed");
} finally {
  await browser.close();
}
