import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { chromium } from "@playwright/test";

const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
const pageUrl = process.argv[2];
assert(pageUrl, "Pass the local campaign links dashboard URL");

const executablePath = await access("/usr/bin/chromium").then(
  () => "/usr/bin/chromium",
  () => undefined,
);
const browser = await chromium.launch({
  headless: true,
  ...(executablePath ? { executablePath } : {}),
});

try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const pageErrors = [];
  const campaignApiResponses = [];
  page.on("pageerror", (error) => pageErrors.push(error.stack ?? error.message));
  page.on("response", async (response) => {
    if (new URL(response.url()).pathname.endsWith("/campaigns")) {
      campaignApiResponses.push({ status: response.status(), body: await response.text() });
    }
  });
  await page.goto(pageUrl, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Campaign links", level: 1 }).waitFor();
  await page.getByText("0 of 100 campaign IDs used.", { exact: true }).waitFor({ timeout: 10_000 }).catch(async (error) => {
    throw new Error(
      `${error.message}\nPanel: ${await page.locator(".campaign-registry-panel").innerText()}\nAPI: ${JSON.stringify(campaignApiResponses)}\nPage errors: ${JSON.stringify(pageErrors)}`,
    );
  });
  assert.deepEqual(pageErrors, []);

  const campaignId = "browser-campaign";
  const campaignInput = page.getByRole("textbox", { name: "Campaign ID" });
  await campaignInput.fill(campaignId);
  let hideNextPostResponse = true;
  const registryPath = `/api${new URL(pageUrl).pathname.replace(/\/campaigns$/, "")}`;
  let failNextRefresh = false;
  await page.route(`**${registryPath}/campaigns**`, async (route) => {
    if (route.request().method() === "POST" && hideNextPostResponse) {
      hideNextPostResponse = false;
      const response = await route.fetch();
      assert.equal(response.status(), 200);
      await route.abort();
      return;
    }
    if (route.request().method() === "DELETE") {
      const response = await route.fetch();
      assert.equal(response.status(), 200);
      failNextRefresh = true;
      await route.abort();
      return;
    }
    if (route.request().method() === "GET" && failNextRefresh) {
      failNextRefresh = false;
      await route.abort();
      return;
    }
    await route.continue();
  });
  await campaignInput.press("Enter");
  await page.getByText(`Campaign ${campaignId} is registered.`, { exact: true }).waitFor();
  assert.equal(hideNextPostResponse, false);
  const campaignRow = page.locator(".campaign-row").filter({ hasText: campaignId });
  await campaignRow.waitFor({ state: "visible" });
  const linkInput = campaignRow.getByRole("textbox", {
    name: `Generated link for ${campaignId}`,
  });
  const generatedUrl = await linkInput.inputValue();
  assert.match(generatedUrl, new RegExp(`^https://funnels\\.example/f/[a-z0-9-]+\\?campaign_id=${campaignId}$`));
  assert.equal(await linkInput.getAttribute("readonly"), "true");
  assert.equal(await campaignRow.getByRole("link", { name: "Open link" }).getAttribute("href"), generatedUrl);
  assert.deepEqual(pageErrors, []);

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

  await page.setViewportSize({ width: 390, height: 844 });
  const horizontalOverflow = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  assert.equal(horizontalOverflow, false);

  await campaignRow.getByRole("button", { name: `Remove campaign ${campaignId}` }).click();
  await page
    .getByText(
      "The change could not be confirmed, and the registry could not be refreshed. Try reloading the page.",
      { exact: true },
    )
    .waitFor();
  await page.getByRole("alert").getByRole("button", { name: "Reload campaign IDs" }).click();
  await page.getByText("0 of 100 campaign IDs used.", { exact: true }).waitFor();
  assert.equal(await page.locator(".campaign-row").count(), 0);

  await page.reload({ waitUntil: "networkidle" });
  await page.getByText("0 of 100 campaign IDs used.", { exact: true }).waitFor();
  assert.deepEqual(pageErrors, []);
  console.log("PUB-012 campaign links keyboard flow, uncertain-response recovery, and mobile layout passed");
} finally {
  await browser.close();
}
