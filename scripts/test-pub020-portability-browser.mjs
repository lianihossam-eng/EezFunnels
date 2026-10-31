import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { createRequire } from "node:module";
import { deflateSync } from "node:zlib";
import { chromium } from "@playwright/test";
import { startPr019Worker } from "./pr019-local-worker.mjs";
import { finalizePr019Browser } from "./pr019-browser-finalizer.mjs";

const diagnostics = [];
const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
let browser;
let worker;

async function closeBrowser() {
  if (browser) {
    await browser.close();
    browser = undefined;
  }
}

async function jsonResponse(response, expectedStatus, label) {
  const text = await response.text();
  const status = typeof response.status === "function" ? response.status() : response.status;
  assert.equal(status, expectedStatus, `${label}: ${text}`);
  return JSON.parse(text);
}

async function createFunnel(origin) {
  return jsonResponse(
    await fetch(`${origin}/api/funnels/v2`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": `pub020-create-${randomUUID()}`,
      },
      body: JSON.stringify({ name: "PUB-020 portability source", blueprint: "lead_gen" }),
    }),
    201,
    "Create source V2 funnel",
  );
}

function crc32(bytes) {
  let crc = 0xffff_ffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    }
  }
  return (crc ^ 0xffff_ffff) >>> 0;
}

function pngChunk(type, data) {
  const typeBytes = Buffer.from(type);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const checksum = Buffer.alloc(4);
  checksum.writeUInt32BE(crc32(Buffer.concat([typeBytes, data])));
  return Buffer.concat([length, typeBytes, data, checksum]);
}

function tinyPng(seed) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(1, 0);
  header.writeUInt32BE(1, 4);
  header[8] = 8;
  header[9] = 6;
  const pixels = Buffer.from([0, seed & 0xff, (seed * 37) & 0xff, (seed * 71) & 0xff, 0xff]);
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk("IHDR", header),
    pngChunk("IDAT", deflateSync(pixels)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

try {
  worker = await startPr019Worker("document-portability", { onSignalCleanup: closeBrowser });
  const executablePath = await access("/usr/bin/chromium").then(
    () => "/usr/bin/chromium",
    () => undefined,
  );
  browser = await chromium.launch({
    headless: true,
    ...(executablePath ? { executablePath } : {}),
  });
  const page = await browser.newPage({ viewport: { width: 1360, height: 1000 } });
  page.on("pageerror", (error) => diagnostics.push(error.stack ?? error.message));

  const source = await createFunnel(worker.origin);
  const { id: sourceId } = source.funnel;
  const exportResponse = await fetch(`${worker.origin}/api/funnels/${sourceId}/template`);
  assert.match(exportResponse.headers.get("content-disposition") ?? "", /attachment/);
  assert.match(exportResponse.headers.get("cache-control") ?? "", /private/);
  assert.match(exportResponse.headers.get("cache-control") ?? "", /no-store/);
  const exportedTemplate = await jsonResponse(exportResponse, 200, "Export saved template");
  assert.equal(exportedTemplate.format, "eezfunnel.funnel-template");
  assert.equal(exportedTemplate.format_version, 1);
  assert.deepEqual(Object.keys(exportedTemplate).sort(), [
    "document",
    "format",
    "format_version",
    "media",
    "name",
  ]);
  assert.deepEqual(exportedTemplate.media, []);
  assert.equal("brief" in exportedTemplate, false);
  assert.equal("submissions" in exportedTemplate, false);
  assert.equal("secrets" in exportedTemplate, false);

  await page.goto(`${worker.origin}/funnels`, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Your funnels" }).waitFor();
  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("link", { name: "Export saved template" }).click();
  const download = await downloadPromise;
  assert.equal(download.suggestedFilename(), "funnel-template.json");
  const downloadPath = await download.path();
  assert.ok(downloadPath);
  const downloaded = JSON.parse(await readFile(downloadPath, "utf8"));
  assert.deepEqual(downloaded, exportedTemplate);

  const imageBytes = tinyPng(0);
  const uploaded = await jsonResponse(
    await fetch(`${worker.origin}/api/media`, {
      method: "POST",
      headers: { "Content-Type": "image/png" },
      body: imageBytes,
    }),
    201,
    "Add a replacement image to the target library",
  );
  const targetAsset = uploaded.asset;
  assert.equal(targetAsset.id, targetAsset.sha256);
  const paginatedAssets = [targetAsset];
  for (let index = 1; index <= 50; index += 1) {
    paginatedAssets.push((await jsonResponse(
      await fetch(`${worker.origin}/api/media`, {
        method: "POST",
        headers: { "Content-Type": "image/png" },
        body: tinyPng(index),
      }),
      201,
      `Upload media asset ${index} for paginated inspection`,
    )).asset);
  }
  const mutationHeaders = {
    "Content-Type": "application/json",
    Origin: worker.origin,
  };

  const paginatedTemplate = structuredClone(exportedTemplate);
  const paginatedManifest = [];
  for (let sectionIndex = 0; sectionIndex < Math.ceil(paginatedAssets.length / 50); sectionIndex += 1) {
    const blocks = [];
    const firstAssetIndex = sectionIndex * 50;
    const lastAssetIndex = Math.min(firstAssetIndex + 50, paginatedAssets.length);
    for (let assetIndex = firstAssetIndex; assetIndex < lastAssetIndex; assetIndex += 1) {
      const asset = paginatedAssets[assetIndex];
      blocks.push({
        id: `block-pub020-paginated-${assetIndex}`,
        kind: "image",
        asset_id: asset.id,
        alt: { kind: "decorative" },
      });
      paginatedManifest.push({
        asset_id: asset.id,
        sha256: asset.sha256,
        mime: asset.mime,
        size_bytes: asset.size_bytes,
      });
    }
    paginatedTemplate.document.pages[0].sections.push({
      id: `section-pub020-paginated-${sectionIndex}`,
      type: "hero",
      blocks,
    });
  }
  paginatedTemplate.media = paginatedManifest;
  const paginatedInspection = await jsonResponse(
    await fetch(`${worker.origin}/api/funnel-templates/inspect`, {
      method: "POST",
      headers: mutationHeaders,
      body: JSON.stringify(paginatedTemplate),
    }),
    200,
    "Inspect media across multiple R2 listing pages",
  );
  assert.equal(paginatedInspection.media.length, 51);
  assert.equal(paginatedInspection.media.filter(({ available }) => available).length, 51);

  const maximumManifestTemplate = structuredClone(exportedTemplate);
  const maximumManifestAssets = [{
    asset_id: targetAsset.id,
    sha256: targetAsset.sha256,
    mime: targetAsset.mime,
    size_bytes: targetAsset.size_bytes,
  }];
  for (let index = 1; index < 1_000; index += 1) {
    const assetId = index.toString(16).padStart(64, "0");
    maximumManifestAssets.push({
      asset_id: assetId,
      sha256: assetId,
      mime: "image/png",
      size_bytes: 1,
    });
  }
  for (let sectionIndex = 0; sectionIndex < 20; sectionIndex += 1) {
    const blocks = [];
    for (let blockIndex = 0; blockIndex < 50; blockIndex += 1) {
      const assetIndex = sectionIndex * 50 + blockIndex;
      const assetId = assetIndex === 0
        ? targetAsset.id
        : assetIndex.toString(16).padStart(64, "0");
      blocks.push({
        id: `block-pub020-capacity-${assetIndex}`,
        kind: "image",
        asset_id: assetId,
        alt: { kind: "decorative" },
      });
    }
    maximumManifestTemplate.document.pages[0].sections.push({
      id: `section-pub020-capacity-${sectionIndex}`,
      type: "hero",
      blocks,
    });
  }
  maximumManifestTemplate.media = maximumManifestAssets;
  const maximumManifestInspection = await jsonResponse(
    await fetch(`${worker.origin}/api/funnel-templates/inspect`, {
      method: "POST",
      headers: mutationHeaders,
      body: JSON.stringify(maximumManifestTemplate),
    }),
    200,
    "Inspect the maximum-size media manifest",
  );
  assert.equal(maximumManifestInspection.media.length, 1_000);
  assert.equal(
    maximumManifestInspection.media.filter(({ available }) => available).length,
    1,
  );

  const portableWithAvailableImage = structuredClone(exportedTemplate);
  portableWithAvailableImage.document.pages[0].sections[0].blocks.push({
    id: "block-pub020-available-image",
    kind: "image",
    asset_id: targetAsset.id,
    alt: { kind: "decorative" },
  });
  portableWithAvailableImage.media = [{
    asset_id: targetAsset.id,
    sha256: targetAsset.sha256,
    mime: targetAsset.mime,
    size_bytes: targetAsset.size_bytes,
  }];
  const availableInspection = await jsonResponse(
    await fetch(`${worker.origin}/api/funnel-templates/inspect`, {
      method: "POST",
      headers: mutationHeaders,
      body: JSON.stringify(portableWithAvailableImage),
    }),
    200,
    "Inspect a template with media already available",
  );
  assert.equal(availableInspection.media[0].available, true);
  const availableImportBody = JSON.stringify({
    name: "PUB-020 available-media round trip",
    template: portableWithAvailableImage,
    asset_replacements: {},
  });
  const availableImport = await jsonResponse(
    await fetch(`${worker.origin}/api/funnel-templates/import`, {
      method: "POST",
      headers: {
        ...mutationHeaders,
        "Idempotency-Key": `pub020-available-${randomUUID()}`,
      },
      body: availableImportBody,
    }),
    201,
    "Import with media already available",
  );
  const availableDocument = await jsonResponse(
    await fetch(`${worker.origin}/api/funnels/${availableImport.funnel_id}/document`),
    200,
    "Read available-media imported document",
  );
  const availableImage = availableDocument.pages
    .flatMap((item) => item.sections)
    .flatMap((section) => section.blocks)
    .find((block) => block.kind === "image");
  assert.equal(availableImage?.asset_id, targetAsset.id);
  assert.equal(availableImport.published, false);

  const missingAssetId = "a".repeat(64);
  const portableWithMissingImage = structuredClone(exportedTemplate);
  const originalPageId = portableWithMissingImage.document.pages[0].id;
  portableWithMissingImage.document.pages[0].sections[0].blocks.push({
    id: "block-pub020-image",
    kind: "image",
    asset_id: missingAssetId,
    alt: { kind: "decorative" },
  });
  portableWithMissingImage.media = [{
    asset_id: missingAssetId,
    sha256: missingAssetId,
    mime: "image/png",
    size_bytes: imageBytes.length,
  }];

  const inspection = await jsonResponse(
    await fetch(`${worker.origin}/api/funnel-templates/inspect`, {
      method: "POST",
      headers: mutationHeaders,
      body: JSON.stringify(portableWithMissingImage),
    }),
    200,
    "Inspect template media availability",
  );
  assert.equal(inspection.page_count, portableWithMissingImage.document.pages.length);
  assert.equal(inspection.form_count, portableWithMissingImage.document.forms.length);
  assert.equal(inspection.media[0].asset.asset_id, missingAssetId);
  assert.equal(inspection.media[0].available, false);

  const funnelsBeforeRejectedImport = await jsonResponse(
    await fetch(`${worker.origin}/api/funnels`),
    200,
    "List funnels before rejected import",
  );
  const blockedImport = await jsonResponse(
    await fetch(`${worker.origin}/api/funnel-templates/import`, {
      method: "POST",
      headers: {
        ...mutationHeaders,
        "Idempotency-Key": `pub020-missing-${randomUUID()}`,
      },
      body: JSON.stringify({
        name: "PUB-020 must replace missing image",
        template: portableWithMissingImage,
        asset_replacements: {},
      }),
    }),
    409,
    "Reject import while media is missing",
  );
  assert.equal(blockedImport.code, "conflict/template_media_missing");
  const funnelsAfterRejectedImport = await jsonResponse(
    await fetch(`${worker.origin}/api/funnels`),
    200,
    "List funnels after rejected import",
  );
  assert.equal(funnelsAfterRejectedImport.items.length, funnelsBeforeRejectedImport.items.length);

  let capturedImportBody;
  let capturedImportKey;
  page.on("request", (request) => {
    if (request.method() === "POST" && new URL(request.url()).pathname === "/api/funnel-templates/import") {
      capturedImportBody = request.postData();
      capturedImportKey = request.headers()["idempotency-key"];
    }
  });
  await page.getByRole("button", { name: "Import template" }).click();
  const dialog = page.getByRole("dialog", { name: "Import a funnel template" });
  await dialog.waitFor({ state: "visible" });
  await page.addScriptTag({ content: axeSource });
  const dialogViolations = await page.evaluate(async () => {
    const result = await window.axe.run(document.querySelector("#import-funnel-template-dialog"));
    return result.violations.map(({ id, impact }) => ({ id, impact }));
  });
  assert.deepEqual(dialogViolations, []);
  await dialog.locator("#funnel-template-file").setInputFiles({
    name: "service-template.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(portableWithMissingImage)),
  });
  const replacement = dialog.getByLabel(new RegExp(`Image ${missingAssetId.slice(0, 12)} is unavailable`));
  await replacement.waitFor();
  await replacement.selectOption(targetAsset.id);
  await dialog.getByLabel("Name for the new funnel").fill("PUB-020 imported draft");
  const importResponsePromise = page.waitForResponse(
    (response) => response.request().method() === "POST"
      && new URL(response.url()).pathname === "/api/funnel-templates/import",
  );
  await dialog.getByRole("button", { name: "Import as draft" }).click();
  const imported = await jsonResponse(await importResponsePromise, 201, "Import as an unpublished draft");
  assert.equal(imported.published, false);
  assert.ok(imported.funnel_id);
  assert.match(
    imported.funnel_id,
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
  );
  assert.ok(capturedImportBody);
  assert.ok(capturedImportKey);
  await dialog.waitFor({ state: "hidden" });
  await page.getByText("Template imported as an unpublished draft.").waitFor();
  await page.getByRole("link", { name: "Open draft" }).waitFor();

  const editorDocumentResponse = await fetch(
    `${worker.origin}/api/funnels/${imported.funnel_id}/document`,
  );
  const importedDocumentJson = await editorDocumentResponse.text();
  assert.equal(editorDocumentResponse.status, 200, importedDocumentJson);
  const importedDocument = JSON.parse(importedDocumentJson);
  assert.notEqual(importedDocument.funnel_id, portableWithMissingImage.document.funnel_id);
  assert.notEqual(importedDocument.pages[0].id, originalPageId);
  const importedImage = importedDocument.pages
    .flatMap((item) => item.sections)
    .flatMap((section) => section.blocks)
    .find((block) => block.kind === "image");
  assert.equal(importedImage.asset_id, targetAsset.id);
  await jsonResponse(
    await fetch(`${worker.origin}/api/submissions?funnel_id=${imported.funnel_id}`),
    200,
    "Filter the inbox with the imported funnel ID",
  );

  const importedList = await jsonResponse(
    await fetch(`${worker.origin}/api/funnels`),
    200,
    "Verify the new funnel is a draft",
  );
  const importedCard = importedList.items.find((item) => item.id === imported.funnel_id);
  assert.ok(importedCard);
  assert.equal(importedCard.published_revision_id, null);

  const exactRetry = await jsonResponse(
    await fetch(`${worker.origin}/api/funnel-templates/import`, {
      method: "POST",
      headers: { ...mutationHeaders, "Idempotency-Key": capturedImportKey },
      body: capturedImportBody,
    }),
    201,
    "Retry the same import after a response",
  );
  assert.equal(exactRetry.funnel_id, imported.funnel_id);
  const mismatchedRetry = await fetch(`${worker.origin}/api/funnel-templates/import`, {
    method: "POST",
    headers: { ...mutationHeaders, "Idempotency-Key": capturedImportKey },
    body: JSON.stringify({
      name: "Different payload",
      template: portableWithMissingImage,
      asset_replacements: { [missingAssetId]: targetAsset.id },
    }),
  });
  assert.equal(mismatchedRetry.status, 409);
  assert.equal((await mismatchedRetry.json()).code, "conflict/idempotency_payload_mismatch");
  const finalList = await jsonResponse(
    await fetch(`${worker.origin}/api/funnels`),
    200,
    "Verify retries did not create a second funnel",
  );
  assert.equal(finalList.items.filter((item) => item.id === imported.funnel_id).length, 1);
  await page.setViewportSize({ width: 390, height: 844 });
  assert.equal(
    await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
    false,
  );
  assert.deepEqual(diagnostics, [], `Browser errors: ${diagnostics.join("\n")}`);
  console.log(JSON.stringify({
    workflow: "versioned export -> download -> inspect -> block missing media -> replace from target library -> create unpublished import -> idempotent retry",
    pages: importedDocument.pages.length,
    forms: importedDocument.forms.length,
    replacementVerified: true,
    duplicateFunnelCount: 0,
    browserErrors: diagnostics.length,
  }));
} finally {
  await finalizePr019Browser(closeBrowser, worker);
}
