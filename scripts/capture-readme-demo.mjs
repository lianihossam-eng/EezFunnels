import assert from "node:assert/strict";
import { access, mkdir, mkdtemp, rm, stat } from "node:fs/promises";
import { resolve, join } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { chromium } from "@playwright/test";
import { startPr019Worker } from "./pr019-local-worker.mjs";

const root = resolve(import.meta.dirname, "..");
const assets = join(root, "docs/assets");
const scratch = await mkdtemp(join(tmpdir(), "eezfunnel-readme-demo-"));
const heroTitle = "A clearer plan for your next launch";
let worker;
let browser;

async function captureEditor() {
  const videoDir = join(scratch, "editor-video");
  await mkdir(videoDir, { recursive: true });
  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    recordVideo: { dir: videoDir, size: { width: 1280, height: 800 } },
  });
  const page = await context.newPage();
  try {
    await page.goto(`${worker.origin}/funnels`, { waitUntil: "networkidle" });
    await page.getByRole("heading", { name: "Your funnels" }).waitFor();
    await page.getByText("No funnels yet").waitFor();
    await page.waitForTimeout(800);

    await page.getByRole("button", { name: "Create funnel" }).click();
    const dialog = page.getByRole("dialog", { name: "Create a funnel" });
    await dialog.waitFor({ state: "visible" });
    await page.getByLabel("Funnel name").fill("Launch Workshop");
    await page.locator('input[name="blueprint"][value="webinar"]').check();
    await page.waitForTimeout(1000);

    const createResponsePromise = page.waitForResponse(
      (response) => response.request().method() === "POST" && response.url().endsWith("/api/funnels/v2"),
    );
    await dialog.getByRole("button", { name: "Create funnel" }).click();
    const createResponse = await createResponsePromise;
    assert.equal(createResponse.status(), 201, await createResponse.text());
    const created = await createResponse.json();
    const { id, slug } = created.funnel;
    assert.equal(typeof id, "string");
    assert.equal(typeof slug, "string");

    const funnelLink = page.getByRole("link", { name: "Launch Workshop" });
    await funnelLink.waitFor();
    await funnelLink.click();
    await page.getByRole("heading", { name: "Launch Workshop" }).waitFor();
    await page.getByRole("list", { name: "Sections on this page" }).waitFor();
    await page.waitForTimeout(900);
    await page.getByRole("button", { name: "Hero", exact: true }).click();
    const title = page.getByRole("textbox", { name: "Heading content for block-hero-title" });
    await title.fill(heroTitle);
    await title.press("Tab");
    const copy = page.getByRole("textbox", { name: "Text content for block-hero-copy" });
    await copy.fill("A live session to shape your next launch into a clear, repeatable plan.");
    await copy.press("Tab");
    await page.frameLocator("iframe").getByRole("heading", { name: heroTitle }).waitFor();
    await page.waitForTimeout(700);

    const mobilePreview = page
      .getByRole("group", { name: "Preview viewport" })
      .getByRole("button", { name: "Mobile (375 px)" });
    await mobilePreview.click();
    await page.waitForFunction(() => document.querySelector("iframe")?.getAttribute("width") === "375");
    await page.locator(".editor-preview-canvas").scrollIntoViewIfNeeded();
    await page.waitForTimeout(1200);

    await page.getByRole("button", { name: "Save revision" }).click();
    await page.getByText("Revision saved.").waitFor();
    await page.waitForTimeout(500);
    await page.getByRole("button", { name: "Publish saved revision" }).click();
    await page.getByText("Revision published.").waitFor();
    await page.waitForTimeout(900);

    const publicUrl = `${worker.origin}/f/${slug}`;
    const response = await page.goto(publicUrl, { waitUntil: "networkidle" });
    assert.equal(response.status(), 200);
    await page.getByRole("heading", { name: heroTitle }).waitFor();
    await page.getByRole("heading", { name: "What makes this offer valuable" }).waitFor();
    await page.getByRole("heading", { name: "Trusted by customers" }).waitFor();
    await page.getByRole("heading", { name: "Ready to get started?" }).waitFor();
    await page.waitForTimeout(1600);
    return { video: page.video(), publicUrl };
  } catch (error) {
    const screen = await page.locator("body").innerText().catch(() => "<body unavailable>");
    throw new Error(`Editor demo capture failed at ${page.url()}: ${error}\n${screen.slice(0, 2500)}`);
  } finally {
    await context.close();
  }
}

async function capturePublishedPage(publicUrl) {
  const videoDir = join(scratch, "public-video");
  await mkdir(videoDir, { recursive: true });
  const context = await browser.newContext({
    viewport: { width: 430, height: 860 },
    recordVideo: { dir: videoDir, size: { width: 430, height: 860 } },
  });
  const page = await context.newPage();
  try {
    const response = await page.goto(publicUrl, { waitUntil: "networkidle" });
    assert.equal(response.status(), 200);
    await page.getByRole("heading", { name: heroTitle }).waitFor();
    await page.getByRole("heading", { name: "What makes this offer valuable" }).waitFor();
    await page.getByRole("heading", { name: "Trusted by customers" }).waitFor();
    await page.getByRole("heading", { name: "Ready to get started?" }).waitFor();
    await page.waitForTimeout(1000);
    await page.evaluate(() => window.scrollTo({ top: window.innerHeight * 0.7, behavior: "smooth" }));
    await page.waitForTimeout(1700);
    await page.evaluate(() => window.scrollTo({ top: window.innerHeight * 1.45, behavior: "smooth" }));
    await page.waitForTimeout(1700);
    await page.evaluate(() => window.scrollTo({ top: 0, behavior: "smooth" }));
    await page.waitForTimeout(1000);
    return page.video();
  } catch (error) {
    throw new Error(`Published page demo capture failed at ${page.url()}: ${error}`);
  } finally {
    await context.close();
  }
}

async function makeGif(video, filename, width, startAt) {
  const videoPath = await video.path();
  const outputPath = join(assets, filename);
  const filter = `fps=6,scale=${width}:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=112[p];[s1][p]paletteuse=dither=bayer`;
  const result = spawnSync(
    "ffmpeg",
    ["-y", "-ss", startAt, "-i", videoPath, "-vf", filter, "-loop", "0", outputPath],
    { encoding: "utf8", maxBuffer: 2 * 1024 * 1024 },
  );
  assert.equal(result.status, 0, `ffmpeg failed for ${filename}:\n${result.stderr}`);
  const file = await stat(outputPath);
  console.log(`${filename}: ${(file.size / 1024).toFixed(0)} KiB`);
}

try {
  worker = await startPr019Worker("readme-demo");
  const executablePath = await access("/usr/bin/chromium").then(
    () => "/usr/bin/chromium",
    () => undefined,
  );
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
  await mkdir(assets, { recursive: true });
  const editor = await captureEditor();
  const publicVideo = await capturePublishedPage(editor.publicUrl);
  await makeGif(editor.video, "dashboard-demo.gif", 960, "1");
  await makeGif(publicVideo, "published-page-demo.gif", 430, "0.5");
  console.log(`Captured local funnel at ${editor.publicUrl}`);
} finally {
  if (browser) await browser.close();
  if (worker) await worker.stop();
  await rm(scratch, { recursive: true, force: true });
}
