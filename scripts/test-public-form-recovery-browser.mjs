import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { access } from "node:fs/promises";
import { chromium } from "@playwright/test";

const fixturePath = process.argv[2];
assert(fixturePath, "Pass the HTML file returned by the rate-limit recovery endpoint");
const markup = await readFile(fixturePath, "utf8");
const executablePath = await access("/usr/bin/chromium").then(
  () => "/usr/bin/chromium",
  () => undefined,
);
const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
try {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.setContent(markup, { waitUntil: "load" });
  assert.equal(await page.getByLabel("Name").inputValue(), "\nAda Lovelace");
  console.log("Rate-limit recovery preserves the initial textarea line break");
} finally {
  await browser.close();
}
