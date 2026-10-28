import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { chromium, expect } from "@playwright/test";
import { createHash, randomUUID } from "node:crypto";
import { startPr019Worker } from "./pr019-local-worker.mjs";
import { finalizePr019Browser } from "./pr019-browser-finalizer.mjs";

const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
const errors = [];
let browser;
let worker;

async function closeBrowser() {
  if (browser) {
    await browser.close();
    browser = undefined;
  }
}

async function jsonResponse(response, expectedStatus, label) {
  const body = await response.text();
  const status = typeof response.status === "function" ? response.status() : response.status;
  assert.equal(status, expectedStatus, `${label}: ${body}`);
  return JSON.parse(body);
}

async function editDocument(workerOrigin, funnelId, baseRevisionId, pageId, document, edit) {
  const response = await fetch(`${workerOrigin}/api/funnels/${funnelId}/edit-v2`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      base_revision_id: baseRevisionId,
      page_id: pageId,
      document_json: JSON.stringify(document),
      operation_id: randomUUID().replaceAll("-", "").slice(0, 32),
      edit,
    }),
  });
  const result = await jsonResponse(response, 200, `Apply ${edit.operation} edit`);
  return JSON.parse(result.document_json);
}

async function saveDocument(workerOrigin, funnelId, baseRevisionId, document) {
  const documentBytes = Buffer.from(JSON.stringify(document));
  const documentSha256 = createHash("sha256").update(documentBytes).digest("hex");
  const stage = await jsonResponse(
    await fetch(`${workerOrigin}/api/funnels/${funnelId}/stages-v2`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": `media-stage-${randomUUID()}`,
      },
      body: JSON.stringify({
        base_revision_id: baseRevisionId,
        expected_length: documentBytes.length,
        expected_sha256: documentSha256,
      }),
    }),
    201,
    "Mint image-and-form revision stage",
  );
  const uploaded = await fetch(`${workerOrigin}/api/funnels/${funnelId}/stages-v2`, {
    method: "PUT",
    headers: {
      Authorization: `Bearer ${stage.stage_id}`,
      "Content-Type": "application/octet-stream",
    },
    body: documentBytes,
  });
  assert.equal(uploaded.status, 204, await uploaded.text());
  const saved = await jsonResponse(
    await fetch(`${workerOrigin}/api/funnels/${funnelId}/revisions-v2`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": `media-save-${randomUUID()}`,
      },
      body: JSON.stringify({
        base_revision_id: baseRevisionId,
        stage_id: stage.stage_id,
        expected_length: documentBytes.length,
        expected_sha256: documentSha256,
      }),
    }),
    201,
    "Save image-and-form revision",
  );
  return saved.revision_id;
}

try {
  worker = await startPr019Worker("media-library", { onSignalCleanup: closeBrowser });
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1360, height: 1000 } });
  page.on("pageerror", (error) => errors.push(error.stack ?? error.message));

  const created = await jsonResponse(
    await fetch(`${worker.origin}/api/funnels/v2`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": `media-create-${randomUUID()}`,
      },
      body: JSON.stringify({ name: "Media library smoke", blueprint: "lead_gen" }),
    }),
    201,
    "Create V2 funnel",
  );
  const { id: funnelId, slug } = created.funnel;

  const imageBytes = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
    "base64",
  );
  const uploadResponse = page.waitForResponse(
    (response) => response.url().endsWith("/api/media") && response.request().method() === "POST",
  );
  await page.goto(`${worker.origin}/funnels/${funnelId}`, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Media library smoke" }).waitFor();
  await page.getByText("Media library", { exact: true }).waitFor();
  await page.locator("#v2-media-upload").setInputFiles({
    name: "one-pixel.png",
    mimeType: "image/png",
    buffer: imageBytes,
  });
  const uploadedResponse = await uploadResponse;
  const uploaded = await jsonResponse(uploadedResponse, 201, "Upload image from editor");
  const asset = uploaded.asset;
  assert.equal(asset.id, asset.sha256);
  assert.equal(asset.mime, "image/png");
  assert.equal(asset.size_bytes, imageBytes.length);

  const selectImage = page.getByRole("button", { name: `Select image ${asset.id.slice(0, 8)}` });
  await selectImage.waitFor();
  await selectImage.click();
  assert.equal(await selectImage.getAttribute("aria-pressed"), "true");
  await page.getByRole("button", { name: "Add selected image to section" }).click();

  const previewImage = page
    .frameLocator("iframe")
    .locator(`img.v2-image[src="/media/${asset.id}"]`);
  await previewImage.waitFor();
  const altKind = page.getByLabel(/^Alternative text type for /);
  await altKind.selectOption("text");
  const altInput = page.getByLabel(/^Alternative text for /);
  await altInput.fill("A one-pixel test image");
  await altInput.press("Tab");
  await expect(previewImage).toHaveAttribute("alt", "A one-pixel test image");

  if (!(await page.evaluate(() => Boolean(window.axe)))) {
    await page.addScriptTag({ content: axeSource });
  }
  const critical = await page.evaluate(async () => {
    const findings = await window.axe.run(document, {
      runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"] },
    });
    return findings.violations
      .filter((finding) => finding.impact === "critical")
      .map((finding) => ({ id: finding.id, help: finding.help }));
  });
  assert.deepEqual(critical, [], `Media editor critical accessibility findings: ${JSON.stringify(critical)}`);

  await page.getByRole("button", { name: "Save revision" }).click();
  await page.getByText("Revision saved. The previous revisions remain available in history.").waitFor();
  await page.getByRole("button", { name: "Publish saved revision" }).click();
  await page.getByText(/Revision published/).waitFor();

  const documentResponse = await fetch(`${worker.origin}/api/funnels/${funnelId}/document`);
  assert.equal(documentResponse.status, 200);
  const baseRevisionId = documentResponse.headers.get("x-funnel-revision-id");
  assert.ok(baseRevisionId);
  let document = JSON.parse(await documentResponse.text());
  const entryPage = document.pages.find((candidate) => candidate.id === document.entry_page_id);
  assert.ok(entryPage);
  const imageLocation = entryPage.sections
    .map((section) => ({ section, block: section.blocks.find((block) => block.asset_id === asset.id) }))
    .find((location) => location.block);
  assert.ok(imageLocation, "saved image block should be in the entry page");
  document = await editDocument(
    worker.origin,
    funnelId,
    baseRevisionId,
    entryPage.id,
    document,
    {
      operation: "create_form",
      section: imageLocation.section.id,
      after: imageLocation.block.id,
    },
  );
  const form = document.forms[0];
  assert.ok(form);
  const nameField = form.fields.find((field) => field.name === "name");
  const emailField = form.fields.find((field) => field.name === "email");
  assert.ok(nameField);
  assert.ok(emailField);
  document = await editDocument(worker.origin, funnelId, baseRevisionId, entryPage.id, document, {
    operation: "set_form_field",
    form: form.id,
    field: nameField.id,
    value: {
      kind: "text",
      name: "name",
      label: "Name",
      required: true,
      validation: { min_length: 2, max_length: 40 },
    },
  });
  document = await editDocument(worker.origin, funnelId, baseRevisionId, entryPage.id, document, {
    operation: "set_form_field",
    form: form.id,
    field: emailField.id,
    value: { kind: "email", name: "email", label: "Email", required: true },
  });
  document = await editDocument(worker.origin, funnelId, baseRevisionId, entryPage.id, document, {
    operation: "set_form_request_consent",
    form: form.id,
    copy: "I agree to be contacted about this request.",
  });
  const imageAndFormRevisionId = await saveDocument(
    worker.origin,
    funnelId,
    baseRevisionId,
    document,
  );
  const publication = await jsonResponse(
    await fetch(`${worker.origin}/api/funnels/${funnelId}`),
    200,
    "Read current publication before compare-and-set",
  );
  const publishedRevisionId = publication.published_revision_id;
  assert.ok(publishedRevisionId);
  await jsonResponse(
    await fetch(`${worker.origin}/api/funnels/${funnelId}/publish`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": `media-publish-${randomUUID()}`,
      },
      body: JSON.stringify({
        revision_id: imageAndFormRevisionId,
        expected_published_revision_id: publishedRevisionId,
      }),
    }),
    200,
    "Publish image-and-form revision",
  );

  const publicResponse = await page.goto(`${worker.origin}/f/${slug}`, { waitUntil: "networkidle" });
  assert.equal(publicResponse?.status(), 200);
  const publicImage = page.locator(`img[src="/media/${asset.id}"]`);
  await publicImage.waitFor();
  assert.equal(await publicImage.getAttribute("alt"), "A one-pixel test image");
  const publicForm = page.locator("form.v2-form");
  await publicForm.waitFor();
  const formAction = await publicForm.getAttribute("action");
  assert.ok(formAction);
  assert.equal(new URL(formAction, worker.origin).searchParams.get("revision_id"), imageAndFormRevisionId);
  const formSubmission = await fetch(new URL(formAction, worker.origin), {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
      Origin: worker.origin,
    },
    body: new URLSearchParams({ name: "Ada Lovelace", email: "ada@example.test", _request_consent: "on" }),
    redirect: "manual",
  });
  assert.equal(formSubmission.status, 303, await formSubmission.text());

  const mediaResponse = await fetch(`${worker.origin}/media/${asset.id}`);
  assert.equal(mediaResponse.status, 200);
  assert.deepEqual(Buffer.from(await mediaResponse.arrayBuffer()), imageBytes);
  assert.equal(mediaResponse.headers.get("content-type"), "image/png");
  assert.equal(mediaResponse.headers.get("x-content-type-options"), "nosniff");
  assert.equal(mediaResponse.headers.get("cache-control"), "public, max-age=31536000, immutable");
  assert.equal(mediaResponse.headers.get("etag"), `"${asset.sha256}"`);

  const headResponse = await fetch(`${worker.origin}/media/${asset.id}`, { method: "HEAD" });
  assert.equal(headResponse.status, 200);
  assert.equal(headResponse.headers.get("content-length"), String(imageBytes.length));
  assert.equal(await headResponse.text(), "");

  const duplicateUpload = await fetch(`${worker.origin}/api/media`, {
    method: "POST",
    headers: { "Content-Type": "image/png" },
    body: imageBytes,
  });
  const duplicate = await jsonResponse(duplicateUpload, 201, "Idempotent content upload");
  assert.equal(duplicate.asset.id, asset.id);

  const library = await jsonResponse(await fetch(`${worker.origin}/api/media`), 200, "Read media library");
  assert.equal(library.assets.length, 1);
  assert.equal(library.assets[0].id, asset.id);
  assert.equal(await libraryResponseCacheControl(await fetch(`${worker.origin}/api/media`)), "no-store");

  const mismatchedType = await fetch(`${worker.origin}/api/media`, {
    method: "POST",
    headers: { "Content-Type": "image/jpeg" },
    body: imageBytes,
  });
  const invalid = await jsonResponse(mismatchedType, 400, "Reject MIME mismatch");
  assert.equal(invalid.code, "validation/media_asset_invalid");

  const malformedPng = await fetch(`${worker.origin}/api/media`, {
    method: "POST",
    headers: { "Content-Type": "image/png" },
    body: Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  });
  const malformed = await jsonResponse(malformedPng, 400, "Reject malformed PNG data");
  assert.equal(malformed.code, "validation/media_asset_invalid");

  const tooLarge = await fetch(`${worker.origin}/api/media`, {
    method: "POST",
    headers: { "Content-Type": "image/png" },
    body: Buffer.alloc(8 * 1024 * 1024 + 1),
  });
  const oversized = await jsonResponse(tooLarge, 413, "Reject oversized image");
  assert.equal(oversized.code, "validation/media_asset_too_large");

  const missing = await jsonResponse(await fetch(`${worker.origin}/media/not-a-hash`), 404, "Reject invalid asset path");
  assert.equal(missing.code, "not_found/asset");

  const repositoryRoot = resolve(import.meta.dirname, "..");
  const wrangler = resolve(repositoryRoot, "node_modules/.bin/wrangler");
  const deletedAsset = spawnSync(
    wrangler,
    [
      "d1",
      "execute",
      "DB",
      "--local",
      "--env",
      "dev",
      "--persist-to",
      worker.stateDir,
      "--command",
      `DELETE FROM assets WHERE id = '${asset.id}'`,
    ],
    { cwd: repositoryRoot, encoding: "utf8", timeout: 30000 },
  );
  assert.equal(
    deletedAsset.status,
    0,
    `Could not remove the published image metadata:\n${deletedAsset.stdout}\n${deletedAsset.stderr}`,
  );
  const unavailableSubmission = await fetch(new URL(formAction, worker.origin), {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
      Origin: worker.origin,
    },
    body: new URLSearchParams({ name: "Ada Lovelace", email: "ada@example.test", _request_consent: "on" }),
    redirect: "manual",
  });
  assert.equal(unavailableSubmission.status, 404);
  const unavailableSubmissionError = await unavailableSubmission.json();
  assert.equal(unavailableSubmissionError.code, "not_found/slug");
  const unavailableMedia = await fetch(`${worker.origin}/media/${asset.id}`);
  assert.equal(unavailableMedia.status, 404);
  assert.deepEqual(errors, [], `Browser errors: ${errors.join("\n")}`);
  console.log(JSON.stringify({ workflow: "upload -> select -> edit alt -> preview -> save -> publish -> submit image-backed form -> reject submission after image metadata removal", assetId: asset.id, bytes: imageBytes.length, criticalAxeFindings: 0 }));
} finally {
  await finalizePr019Browser(closeBrowser, worker);
}

async function libraryResponseCacheControl(response) {
  assert.equal(response.status, 200);
  return response.headers.get("cache-control");
}
