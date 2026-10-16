import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { performance } from "node:perf_hooks";

const PAGE_COUNTS = [1, 10, 25, 50];
const TARGET_BYTES = [16_384, 131_072, 524_288, 1_048_575];
const WARMUP_PAIRS = 5;
const MEASURED_PAIRS = 100;
const MAX_TEXT_BYTES = 8_192;
const MAX_DOCUMENT_BYTES = 1_048_576;
const MAX_PAGES = 50;
const MAX_SECTIONS = 100;
const MAX_BLOCKS = 5_000;
const OUTPUT_PATH = resolve("plan/verification/PR-023/results.json");
const PROFILE_ONLY = process.argv.includes("--profile-only");
const OUTPUT_ARG = process.argv.find((value) => value.startsWith("--output="));

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function canonicalize(value) {
  if (value === null || typeof value === "boolean" || typeof value === "string") {
    return JSON.stringify(value);
  }
  if (typeof value === "number") {
    assert(Number.isSafeInteger(value), "canonical fixture has a non-safe integer");
    return String(value);
  }
  if (Array.isArray(value)) return `[${value.map(canonicalize).join(",")}]`;
  assert(typeof value === "object", "canonical fixture contains an unsupported value");
  const keys = Object.keys(value).sort();
  return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalize(value[key])}`).join(",")}}`;
}

function sha256(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

function utf8Bytes(text) {
  return Buffer.byteLength(text, "utf8");
}

function assertJsonNestingDepth(value, maxDepth = 8) {
  const pending = [{ value, depth: 1 }];
  while (pending.length > 0) {
    const current = pending.pop();
    if (current.value === null || typeof current.value !== "object") continue;
    assert(current.depth <= maxDepth, `document exceeds maximum JSON nesting depth ${maxDepth}`);
    for (const child of Object.values(current.value)) {
      pending.push({ value: child, depth: current.depth + 1 });
    }
  }
}

function emptyDocument(pageCount, textBlockCount) {
  const textCounts = Array.from({ length: pageCount }, (_, index) =>
    Math.floor(textBlockCount / pageCount) + (index < textBlockCount % pageCount ? 1 : 0),
  );
  const pages = [];
  let nextBlock = 0;
  let nextSection = 0;

  for (let pageIndex = 0; pageIndex < pageCount; pageIndex += 1) {
    const pageBlocks = Array.from({ length: textCounts[pageIndex] }, () => ({
      id: `block_${String(nextBlock++).padStart(5, "0")}`,
      kind: "text",
      text: "",
    }));
    if (pageIndex + 1 < pageCount) {
      pageBlocks.push({
        id: `block_${String(nextBlock++).padStart(5, "0")}`,
        kind: "button",
        label: "Continue",
        action: {
          kind: "navigate_page",
          page_id: `page_${String(pageIndex + 1).padStart(2, "0")}`,
        },
      });
    }

    const sectionCount = Math.max(1, Math.ceil(pageBlocks.length / 50));
    const sections = [];
    for (let sectionIndex = 0; sectionIndex < sectionCount; sectionIndex += 1) {
      sections.push({
        id: `section_${String(nextSection++).padStart(3, "0")}`,
        type: pageIndex === 0 && sectionIndex === 0 ? "hero" : "features",
        blocks: pageBlocks.slice(sectionIndex * 50, (sectionIndex + 1) * 50),
      });
    }

    pages.push({
      id: `page_${String(pageIndex).padStart(2, "0")}`,
      slug: pageIndex === 0 ? "" : `page-${String(pageIndex).padStart(2, "0")}`,
      title: `Page ${pageIndex + 1}`,
      sections,
    });
  }

  return {
    document_type: "eezfunnel.funnel-document",
    document_version: 2,
    schema_version: 2,
    funnel_id: "funnel-pr023",
    blueprint_id: "lead_gen",
    theme: { preset_id: "minimal", overrides: {} },
    entry_page_id: "page_00",
    pages,
    forms: [],
  };
}

function textBlocks(document) {
  return document.pages.flatMap((page) =>
    page.sections.flatMap((section) => section.blocks.filter((block) => block.kind === "text")),
  );
}

function buildExactDocument(pageCount, targetBytes) {
  assert(PAGE_COUNTS.includes(pageCount), "unsupported page-count fixture");
  assert(targetBytes <= MAX_DOCUMENT_BYTES + 1, "fixture exceeds boundary probe limit");
  let blockCount = Math.max(pageCount, Math.ceil(targetBytes / 7_900));

  while (blockCount <= MAX_BLOCKS) {
    const document = emptyDocument(pageCount, blockCount);
    const currentBytes = utf8Bytes(canonicalize(document));
    const remaining = targetBytes - currentBytes;
    const each = Math.floor(remaining / blockCount);
    const extra = remaining % blockCount;
    if (remaining >= 0 && each + (extra > 0 ? 1 : 0) <= MAX_TEXT_BYTES) {
      let textIndex = 0;
      for (const block of textBlocks(document)) {
        const length = each + (textIndex < extra ? 1 : 0);
        block.text = "x".repeat(length);
        textIndex += 1;
      }
      const encoded = canonicalize(document);
      assert(utf8Bytes(encoded) === targetBytes, `fixture size mismatch: ${pageCount}/${targetBytes}`);
      return { document, encoded, textBlockCount: blockCount };
    }
    blockCount += 1;
  }

  throw new Error(`cannot build ${pageCount}-page fixture at ${targetBytes} bytes`);
}

function exactKeys(value, expected, label) {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  assert(JSON.stringify(actual) === JSON.stringify(wanted), `${label} has unexpected fields`);
}

function validateDocument(document, encoded) {
  assertJsonNestingDepth(document);
  exactKeys(
    document,
    [
      "document_type",
      "document_version",
      "schema_version",
      "funnel_id",
      "blueprint_id",
      "theme",
      "entry_page_id",
      "pages",
      "forms",
    ],
    "document",
  );
  assert(document.document_type === "eezfunnel.funnel-document", "wrong document type");
  assert(document.document_version === 2 && document.schema_version === 2, "wrong version");
  assert(/^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/.test(document.funnel_id), "invalid funnel ID");
  assert(["tripwire", "vsl", "webinar", "launch", "lead_gen"].includes(document.blueprint_id), "invalid blueprint");
  exactKeys(document.theme, ["preset_id", "overrides"], "theme");
  assert(["minimal", "bold", "elegant", "playful", "corporate"].includes(document.theme.preset_id), "invalid theme");
  assert(JSON.stringify(document.theme.overrides) === "{}", "benchmark fixture has unexpected theme overrides");
  assert(Array.isArray(document.pages) && document.pages.length >= 1 && document.pages.length <= MAX_PAGES, "invalid page count");
  assert(Array.isArray(document.forms) && document.forms.length === 0, "unexpected form fixture");
  assert(utf8Bytes(encoded) <= MAX_DOCUMENT_BYTES, "canonical document exceeds the byte cap");

  const pageIds = new Set();
  const slugs = new Set();
  const blockIds = new Set();
  const sectionIds = new Set();
  const adjacency = new Map();
  let sectionCount = 0;
  let blockCount = 0;
  for (const page of document.pages) {
    exactKeys(page, ["id", "slug", "title", "sections"], "page");
    assert(/^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/.test(page.id), "invalid page ID");
    assert(!pageIds.has(page.id), "duplicate page ID");
    pageIds.add(page.id);
    assert(typeof page.title === "string" && utf8Bytes(page.title) > 0 && utf8Bytes(page.title) <= MAX_TEXT_BYTES, "invalid page title");
    assert(!slugs.has(page.slug), "duplicate page slug");
    slugs.add(page.slug);
    assert(Array.isArray(page.sections) && page.sections.length <= 100, "invalid page section count");
    adjacency.set(page.id, []);
    for (const section of page.sections) {
      exactKeys(section, ["id", "type", "blocks"], "section");
      assert(/^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/.test(section.id), "invalid section ID");
      assert(!sectionIds.has(section.id), "duplicate section ID");
      sectionIds.add(section.id);
      assert(["hero", "features", "testimonials", "cta", "footer"].includes(section.type), "invalid section type");
      assert(Array.isArray(section.blocks) && section.blocks.length <= 50, "invalid block count");
      sectionCount += 1;
      for (const block of section.blocks) {
        assert(/^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/.test(block.id), "invalid block ID");
        assert(!blockIds.has(block.id), "duplicate block ID");
        blockIds.add(block.id);
        blockCount += 1;
        if (block.kind === "text") {
          exactKeys(block, ["id", "kind", "text"], "text block");
          assert(typeof block.text === "string" && utf8Bytes(block.text) <= MAX_TEXT_BYTES, "invalid text value");
        } else {
          exactKeys(block, ["id", "kind", "label", "action"], "button block");
          assert(block.kind === "button" && block.label === "Continue", "invalid button block");
          exactKeys(block.action, ["kind", "page_id"], "navigate action");
          assert(block.action.kind === "navigate_page", "invalid fixture action");
          adjacency.get(page.id).push(block.action.page_id);
        }
      }
    }
  }
  assert(pageIds.has(document.entry_page_id), "entry page is missing");
  assert(document.pages.find((page) => page.id === document.entry_page_id)?.slug === "", "entry page route is invalid");
  assert(sectionCount <= MAX_SECTIONS && blockCount <= MAX_BLOCKS, "document exceeds node limits");
  for (const page of document.pages) {
    if (page.id !== document.entry_page_id) assert(/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(page.slug), "invalid non-entry slug");
  }
  for (const edges of adjacency.values()) {
    for (const target of edges) assert(pageIds.has(target), "navigation target is missing");
  }
  const reached = new Set();
  const pending = [document.entry_page_id];
  while (pending.length > 0) {
    const current = pending.pop();
    if (reached.has(current)) continue;
    reached.add(current);
    const targets = adjacency.get(current) ?? [];
    pending.push(...targets);
  }
  assert(reached.size === pageIds.size, "page graph has an unreachable page");
  for (const [pageId, edges] of adjacency) {
    assert(edges.length <= 1, "fixture has unexpected graph shape");
    const path = new Set();
    let current = pageId;
    while ((adjacency.get(current) ?? []).length > 0) {
      assert(!path.has(current), "page graph contains a cycle");
      path.add(current);
      current = adjacency.get(current)[0];
    }
  }
}

function replaceText(document, blockId, value) {
  for (const block of textBlocks(document)) {
    if (block.id === blockId) {
      block.text = value;
      return;
    }
  }
  throw new Error(`patch target does not exist: ${blockId}`);
}

function buildPatch(base, editKind) {
  const targets = textBlocks(base);
  const count = editKind === "sparse" ? 1 : Math.ceil(targets.length * 0.6);
  const operations = targets.slice(0, count).map((block) => ({
    op: "replace_text",
    block_id: block.id,
    value: "y".repeat(block.text.length),
  }));
  operations.sort((left, right) => left.block_id < right.block_id ? -1 : left.block_id > right.block_id ? 1 : 0);
  return {
    patch_type: "eezfunnel.document-patch",
    patch_version: 1,
    base_document_sha256: sha256(canonicalize(base)),
    operations,
  };
}

function applyPatch(base, patch) {
  exactKeys(patch, ["patch_type", "patch_version", "base_document_sha256", "operations"], "patch");
  assert(patch.patch_type === "eezfunnel.document-patch" && patch.patch_version === 1, "wrong patch discriminator");
  assert(patch.base_document_sha256 === sha256(canonicalize(base)), "wrong patch base digest");
  assert(Array.isArray(patch.operations) && patch.operations.length > 0, "patch has no operations");
  const seen = new Set();
  let previousId = "";
  for (const operation of patch.operations) {
    exactKeys(operation, ["op", "block_id", "value"], "patch operation");
    assert(operation.op === "replace_text", "unsupported patch operation");
    assert(!seen.has(operation.block_id), "duplicate patch target");
    assert(operation.block_id > previousId, "patch operations are not in ASCII ID order");
    assert(typeof operation.value === "string" && utf8Bytes(operation.value) <= MAX_TEXT_BYTES, "invalid patch value");
    seen.add(operation.block_id);
    previousId = operation.block_id;
    replaceText(base, operation.block_id, operation.value);
  }
  return base;
}

function makeFixture(pageCount, targetBytes, editKind) {
  const built = buildExactDocument(pageCount, targetBytes);
  validateDocument(built.document, built.encoded);
  const patch = buildPatch(built.document, editKind);
  const patchRaw = canonicalize(patch);
  const expected = JSON.parse(canonicalize(built.document));
  applyPatch(expected, patch);
  const expectedRaw = canonicalize(expected);
  assert(utf8Bytes(expectedRaw) === targetBytes, "edit changed target fixture length");
  validateDocument(expected, expectedRaw);
  assert(patchRaw !== expectedRaw, "patch wire equals full snapshot unexpectedly");
  return {
    pageCount,
    targetBytes,
    editKind,
    textBlockCount: built.textBlockCount,
    baseRaw: built.encoded,
    snapshotRaw: expectedRaw,
    patchRaw,
    expectedRaw,
    expectedDigest: sha256(expectedRaw),
  };
}

function runSnapshot(fixture) {
  const start = performance.now();
  const rawBytes = utf8Bytes(fixture.snapshotRaw);
  assert(rawBytes <= MAX_DOCUMENT_BYTES, "snapshot upload exceeds the raw cap");
  const document = JSON.parse(fixture.snapshotRaw);
  validateDocument(document, fixture.snapshotRaw);
  const encoded = canonicalize(document);
  const digest = sha256(encoded);
  const elapsedMs = performance.now() - start;
  return { elapsedMs, encoded, digest, rawBytes };
}

function runPatch(fixture) {
  const start = performance.now();
  const rawBytes = utf8Bytes(fixture.patchRaw);
  assert(rawBytes <= MAX_DOCUMENT_BYTES, "patch upload exceeds the raw cap");
  const base = JSON.parse(fixture.baseRaw);
  const patch = JSON.parse(fixture.patchRaw);
  const document = applyPatch(base, patch);
  const encoded = canonicalize(document);
  validateDocument(document, encoded);
  const digest = sha256(encoded);
  const elapsedMs = performance.now() - start;
  return { elapsedMs, encoded, digest, rawBytes };
}

function sorted(values) {
  return [...values].sort((left, right) => left - right);
}

function stats(values) {
  const ordered = sorted(values);
  const percentile = (fraction) => ordered[Math.max(0, Math.ceil(ordered.length * fraction) - 1)];
  return {
    samples: values.length,
    min: ordered[0],
    p50: percentile(0.5),
    p95: percentile(0.95),
    max: ordered.at(-1),
  };
}

function seededRandom(seed) {
  let state = seed >>> 0;
  return () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) / 0x1_0000_0000;
  };
}

function runOne(mode, fixture) {
  try {
    const result = mode === "snapshot" ? runSnapshot(fixture) : runPatch(fixture);
    assert(result.encoded === fixture.expectedRaw, `${mode} output differs from target bytes`);
    assert(result.digest === fixture.expectedDigest, `${mode} output digest differs from target`);
    return { mode, ...result, ok: true };
  } catch (error) {
    return { mode, ok: false, error: String(error?.message ?? error) };
  }
}

function runProfileFixture() {
  const fixture = makeFixture(50, 1_048_575, "broad");
  for (let index = 0; index < 10; index += 1) {
    const result = runOne(index % 2 === 0 ? "snapshot" : "patch", fixture);
    assert(result.ok, `profile-only ${result.mode} failed: ${result.error}`);
  }
  process.stdout.write("profile-only run complete; timings were discarded\n");
}

function runBoundaries() {
  const checks = [];
  for (const targetBytes of [1_048_575, 1_048_576, 1_048_577]) {
    const built = buildExactDocument(1, targetBytes);
    const actual = utf8Bytes(built.encoded);
    assert(actual === targetBytes, "boundary fixture has wrong encoded length");
    let canonicalAccepted = true;
    try {
      validateDocument(built.document, built.encoded);
    } catch {
      canonicalAccepted = false;
    }
    const rawAccepted = actual <= MAX_DOCUMENT_BYTES;
    checks.push({ target_bytes: targetBytes, canonical_accepted: canonicalAccepted, raw_upload_accepted: rawAccepted });
  }
  assert(checks[0].canonical_accepted && checks[0].raw_upload_accepted, "1,048,575-byte boundary was rejected");
  assert(checks[1].canonical_accepted && checks[1].raw_upload_accepted, "1,048,576-byte boundary was rejected");
  assert(!checks[2].canonical_accepted && !checks[2].raw_upload_accepted, "1,048,577-byte boundary was accepted");
  return checks;
}

function runContractRejectionChecks() {
  const valid = emptyDocument(2, 2);
  const checks = [];
  const expectRejected = (name, operation) => {
    try {
      operation();
    } catch (error) {
      checks.push({ name, rejected: true, reason: String(error?.message ?? error) });
      return;
    }
    throw new Error(`negative fixture was accepted: ${name}`);
  };
  const rejectMutatedDocument = (name, mutate) => {
    const document = structuredClone(valid);
    mutate(document);
    expectRejected(name, () => validateDocument(document, canonicalize(document)));
  };

  rejectMutatedDocument("unknown_document_version", (document) => {
    document.document_version = 3;
  });
  rejectMutatedDocument("duplicate_page_id", (document) => {
    document.pages[1].id = document.pages[0].id;
  });
  rejectMutatedDocument("duplicate_block_id", (document) => {
    document.pages[1].sections[0].blocks[0].id = document.pages[0].sections[0].blocks[0].id;
  });
  rejectMutatedDocument("colliding_page_slug", (document) => {
    document.pages[1].slug = "";
  });
  rejectMutatedDocument("invalid_navigation_target", (document) => {
    const button = document.pages[0].sections[0].blocks.find((block) => block.kind === "button");
    button.action.page_id = "missing_page";
  });
  rejectMutatedDocument("cyclic_page_graph", (document) => {
    document.pages[1].sections[0].blocks.push({
      id: "cycle_button",
      kind: "button",
      label: "Continue",
      action: { kind: "navigate_page", page_id: document.pages[0].id },
    });
  });
  expectRejected("malformed_json", () => JSON.parse("{"));
  expectRejected("excessive_nesting_depth", () => {
    let value = {};
    for (let depth = 0; depth < 8; depth += 1) value = { child: value };
    assertJsonNestingDepth(value);
  });
  const wrongDigestPatch = buildPatch(valid, "sparse");
  wrongDigestPatch.base_document_sha256 = "0".repeat(64);
  expectRejected("wrong_patch_base_digest", () => applyPatch(structuredClone(valid), wrongDigestPatch));

  return checks;
}

function median(values) {
  const ordered = sorted(values);
  const center = Math.floor(ordered.length / 2);
  return ordered.length % 2 === 0 ? (ordered[center - 1] + ordered[center]) / 2 : ordered[center];
}

function balancedOrderSchedule(random, pairCount) {
  assert(pairCount % 2 === 0, "balanced pair count must be even");
  const firstModes = Array.from({ length: pairCount }, (_, index) =>
    index < pairCount / 2 ? "snapshot" : "patch",
  );
  for (let index = firstModes.length - 1; index > 0; index -= 1) {
    const swapIndex = Math.floor(random() * (index + 1));
    [firstModes[index], firstModes[swapIndex]] = [firstModes[swapIndex], firstModes[index]];
  }
  return firstModes;
}

function runBenchmark() {
  const random = seededRandom(0x023f00d);
  const fixtures = [];
  const errors = [];
  const contractRejectionChecks = runContractRejectionChecks();
  for (const pageCount of PAGE_COUNTS) {
    for (const targetBytes of TARGET_BYTES) {
      for (const editKind of ["sparse", "broad"]) {
        fixtures.push(makeFixture(pageCount, targetBytes, editKind));
      }
    }
  }

  for (const fixture of fixtures) {
    for (let warmup = 0; warmup < WARMUP_PAIRS; warmup += 1) {
      const order = warmup % 2 === 0 ? ["snapshot", "patch"] : ["patch", "snapshot"];
      for (const mode of order) {
        const result = runOne(mode, fixture);
        if (!result.ok) errors.push({ fixture: `${fixture.pageCount}/${fixture.targetBytes}/${fixture.editKind}`, ...result, phase: "warmup" });
      }
    }
  }

  const measured = [];
  for (const fixture of fixtures) {
    const perMode = { snapshot: [], patch: [] };
    const pairOrders = [];
    const firstModes = balancedOrderSchedule(random, MEASURED_PAIRS);
    for (let pair = 0; pair < MEASURED_PAIRS; pair += 1) {
      const first = firstModes[pair];
      const order = [first, first === "snapshot" ? "patch" : "snapshot"];
      pairOrders.push(order);
      for (const mode of order) {
        const result = runOne(mode, fixture);
        if (!result.ok) errors.push({ fixture: `${fixture.pageCount}/${fixture.targetBytes}/${fixture.editKind}`, ...result, phase: "measured", pair });
        else perMode[mode].push(result);
      }
    }

    const snapshotTimes = perMode.snapshot.map((entry) => entry.elapsedMs);
    const patchTimes = perMode.patch.map((entry) => entry.elapsedMs);
    const snapshotBytes = utf8Bytes(fixture.snapshotRaw);
    const patchBytes = utf8Bytes(fixture.patchRaw);
    const fixtureResult = {
      id: `${fixture.pageCount}p-${fixture.targetBytes}b-${fixture.editKind}`,
      pages: fixture.pageCount,
      canonical_document_bytes: fixture.targetBytes,
      edit_kind: fixture.editKind,
      text_block_count: fixture.textBlockCount,
      pair_count: MEASURED_PAIRS,
      pair_order: pairOrders,
      candidates: {
        snapshot: {
          upload_bytes: snapshotBytes,
          transform_ms: stats(snapshotTimes),
          paired_elapsed_ms: snapshotTimes,
        },
        typed_patch: {
          upload_bytes: patchBytes,
          transform_ms: stats(patchTimes),
          paired_elapsed_ms: patchTimes,
        },
      },
      outputs_equal: perMode.snapshot.every((entry) => entry.encoded === fixture.expectedRaw)
        && perMode.patch.every((entry) => entry.encoded === fixture.expectedRaw),
      modeled_client_round_trips_per_save: {
        both: 3,
        components: ["mint command", "binary stage upload", "final save command"],
      },
      modeled_operations_per_save: {
        both: {
          d1_batches: 4,
          r2_puts: 1,
          r2_finalize_reads: 1,
          persisted_d1_document_bytes: fixture.targetBytes,
        },
        snapshot_additional_base_document_reads: 0,
        typed_patch_additional_base_document_reads: 1,
      },
    };
    if (perMode.snapshot.length !== MEASURED_PAIRS || perMode.patch.length !== MEASURED_PAIRS) {
      errors.push({ fixture: fixtureResult.id, phase: "sample-count", expected: MEASURED_PAIRS, snapshot: perMode.snapshot.length, patch: perMode.patch.length });
    }
    measured.push(fixtureResult);
  }

  const familySavings = {};
  for (const editKind of ["sparse", "broad"]) {
    const fixtureSavings = measured
      .filter((fixture) => fixture.edit_kind === editKind)
      .map((fixture) => 1 - fixture.candidates.typed_patch.upload_bytes / fixture.candidates.snapshot.upload_bytes);
    familySavings[editKind] = {
      fixture_count: fixtureSavings.length,
      median_upload_byte_reduction_fraction: median(fixtureSavings),
      all_fixture_reductions_fraction: fixtureSavings,
    };
  }

  const latencyRatios = {};
  for (const targetBytes of TARGET_BYTES) {
    const rows = measured.filter((fixture) => fixture.canonical_document_bytes === targetBytes);
    const snapshotP95 = stats(rows.flatMap((fixture) => fixture.candidates.snapshot.paired_elapsed_ms)).p95;
    const patchP95 = stats(rows.flatMap((fixture) => fixture.candidates.typed_patch.paired_elapsed_ms)).p95;
    latencyRatios[targetBytes] = {
      snapshot_p95_ms: snapshotP95,
      typed_patch_p95_ms: patchP95,
      p95_ratio: snapshotP95 === 0 ? null : patchP95 / snapshotP95,
      within_two_times: patchP95 <= snapshotP95 * 2,
    };
  }

  const outputsEqual = measured.every((fixture) => fixture.outputs_equal);
  const noErrors = errors.length === 0;
  const sparsePass = familySavings.sparse.median_upload_byte_reduction_fraction >= 0.25;
  const broadPass = familySavings.broad.median_upload_byte_reduction_fraction >= 0.25;
  const latencyPass = Object.values(latencyRatios).every((row) => row.within_two_times);
  const choice = noErrors && outputsEqual && sparsePass && broadPass && latencyPass
    ? "typed_patch_wire"
    : "complete_snapshot_wire";
  const selectedStrategy = choice === "typed_patch_wire"
    ? "typed_patch_for_eligible_replace_text_with_full_snapshot_fallback"
    : "complete_snapshot_only";

  const output = {
    task_id: "PR-023",
    generated_at_utc: new Date().toISOString(),
    node_version: process.version,
    claim_under_test: "Measure whether the typed-patch candidate for eligible replace_text edits passes local fixture correctness, byte-saving, and p95 gates. This Node-only result does not select a runtime wire without equivalent Worker/D1/R2 evidence.",
    measurement_scope: "Node in-memory parse/expand/validate/canonicalize/hash and encoded upload bytes only; no Worker, D1, R2, network, browser, or production latency was measured.",
    validation_scope: "Generated V2 fixture subset: exact top-level/theme/page/section/text/button shapes, IDs, section/block/page/byte caps, entry route, slug uniqueness, and reachable acyclic navigation chain. This is not a production schema validator.",
    canonicalization_scope: "The deterministic serializer matches the JCS rules exercised by this fixture subset (ASCII property names and safe-integer numbers); it is not a complete RFC 8785 implementation or parity test.",
    trial_design: {
      page_counts: PAGE_COUNTS,
      canonical_document_bytes: TARGET_BYTES,
      edit_families: ["sparse", "broad"],
      broad_edit_fraction: 0.6,
      warmup_pairs_per_fixture: WARMUP_PAIRS,
      measured_pairs_per_fixture: MEASURED_PAIRS,
      order: `each fixture has exactly ${MEASURED_PAIRS / 2} snapshot-first and ${MEASURED_PAIRS / 2} patch-first measured pairs, deterministically shuffled after alternating warmups`,
      seed: "0x023f00d",
      fixture_count: fixtures.length,
    },
    boundary_checks: runBoundaries(),
    contract_rejection_checks: contractRejectionChecks,
    errors,
    correctness: { all_outputs_equal: outputsEqual, all_samples_error_free: noErrors },
    fixture_family_upload_savings: familySavings,
    transform_latency_by_size: latencyRatios,
    modeled_provider_operations: {
      client_round_trips_per_save: {
        both: 3,
        components: ["mint command", "binary stage upload", "final save command"],
      },
      pricing_inputs_ref: "plan/verification/PR-023/research.md; official Cloudflare pricing accessed 2026-10-16",
      values_are_model_inputs_not_account_billing: true,
      common_batches_per_save: 4,
      common_r2_puts_per_save: 1,
      common_r2_finalize_reads_per_save: 1,
      additional_d1_base_revision_reads: { complete_snapshot: 0, typed_patch: 1 },
      d1_stored_document_bytes: "same full canonical document for both candidates",
      provider_cost: "not measured or calculated",
    },
    process_memory_snapshot_after_run: process.memoryUsage(),
    memory_measurement_note: "One whole-process snapshot after all fixture objects and results were retained; it is not attributable to either candidate and is not used in the decision.",
    decision: {
      local_gate_candidate: choice,
      local_gate_strategy: selectedStrategy,
      runtime_wire_selection: "complete_snapshot_only_pending_worker_d1_r2_validation",
      patch_eligibility: "Measured candidate only: closed replace_text grammar for existing text blocks, with exact base digest and canonical operation order.",
      candidate_fallback_rule_if_reconsidered: "Use the complete canonical snapshot for any ineligible edit, when patch wire is not strictly smaller than the snapshot, or when patch wire exceeds the 1,048,576-byte upload cap.",
      full_snapshot_persistence: "selected independently; both candidates persist complete snapshots",
      criteria: { sparse_upload_savings_pass: sparsePass, broad_upload_savings_pass: broadPass, p95_latency_pass: latencyPass, correctness_pass: outputsEqual && noErrors },
    },
    fixtures: measured,
  };

  const path = resolve(OUTPUT_ARG?.slice("--output=".length) ?? OUTPUT_PATH);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(output, null, 2)}\n`);
  process.stdout.write(`local candidate gate: ${selectedStrategy} (${choice}); runtime wire: complete_snapshot_only\n`);
  process.stdout.write(`fixtures: ${fixtures.length}; measured candidate samples: ${fixtures.length * MEASURED_PAIRS * 2}; errors: ${errors.length}\n`);
  process.stdout.write(`results: ${path}\n`);
  if (!noErrors || !outputsEqual) process.exitCode = 1;
}

if (PROFILE_ONLY) runProfileFixture();
else runBenchmark();
