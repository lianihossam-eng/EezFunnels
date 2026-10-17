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
  const page = await browser.newPage();
  await page.goto(pageUrl, { waitUntil: "networkidle" });

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
  await page.keyboard.press("Tab");
  assert.equal(await name.evaluate((element) => element === document.activeElement), true);
  assert.equal(await name.evaluate((element) => getComputedStyle(element).outlineWidth), "3px");

  await name.fill("Grace Hopper");
  await email.fill("not-an-email");
  assert.equal(await form.evaluate((element) => element.checkValidity()), false);
  await email.fill("grace@example.test");
  await page.locator('input[name="_request_consent"]').check();
  assert.equal(await form.evaluate((element) => element.checkValidity()), true);

  await page.getByRole("button", { name: "Send" }).click();
  await page.waitForURL((url) => url.hash === `#form-success-${formId}`);
  const status = page.getByRole("status");
  await status.waitFor({ state: "visible" });
  assert.equal(await status.evaluate((element) => element === document.activeElement), true);
  assert.equal(await marketingOptIn.count(), 1);
  const nameLabel = page.locator(`label[for="${await name.getAttribute("id")}"]`);
  assert.equal(await nameLabel.evaluate((element) => getComputedStyle(element).textAlign), "left");
  const [formBox, statusBox] = await Promise.all([
    form.evaluate((element) => element.getBoundingClientRect().toJSON()),
    status.evaluate((element) => element.getBoundingClientRect().toJSON()),
  ]);
  assert.ok(Math.abs(formBox.width - statusBox.width) < 1, { formBox, statusBox });
  assert.ok(statusBox.top >= formBox.bottom + 16, { formBox, statusBox });
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
  if (process.env.PR033_SCREENSHOT) {
    await page.screenshot({ path: process.env.PR033_SCREENSHOT, fullPage: true });
  }
  console.log("PR-033 browser form labels, native validation, consent defaults, and success state passed");
} finally {
  await browser.close();
}
