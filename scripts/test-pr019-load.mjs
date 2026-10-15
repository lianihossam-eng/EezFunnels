import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { startPr019Worker } from "./pr019-local-worker.mjs";

const funnelCount = 100;
const requestCount = 1000;
let worker;

async function jsonResponse(response, expectedStatus, label) {
  const text = await response.text();
  assert.equal(response.status, expectedStatus, `${label}: ${text}`);
  return JSON.parse(text);
}

try {
  worker = await startPr019Worker("load");
  const funnels = [];
  for (let index = 0; index < funnelCount; index += 1) {
    const suffix = String(index).padStart(3, "0");
    const createResponse = await fetch(`${worker.origin}/api/funnels`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": `pr019-load-create-${suffix}-${randomUUID()}`,
      },
      body: JSON.stringify({ name: `PR-019 load ${suffix}`, blueprint: "tripwire" }),
    });
    const created = await jsonResponse(createResponse, 201, `Create funnel ${suffix}`);
    const funnel = {
      id: created.funnel?.id,
      slug: created.funnel?.slug,
      revisionId: created.initial_revision?.id,
    };
    assert.equal(typeof funnel.id, "string");
    assert.equal(typeof funnel.slug, "string");
    assert.equal(typeof funnel.revisionId, "string");

    const publishResponse = await fetch(`${worker.origin}/api/funnels/${funnel.id}/publish`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": `pr019-load-publish-${suffix}-${randomUUID()}`,
      },
      body: JSON.stringify({
        revision_id: funnel.revisionId,
        expected_published_revision_id: null,
      }),
    });
    await jsonResponse(publishResponse, 200, `Publish funnel ${suffix}`);
    funnels.push(funnel);
  }

  assert.equal(funnels.length, funnelCount);
  const latencies = [];
  for (let index = 0; index < requestCount; index += 1) {
    const funnel = funnels[index % funnelCount];
    const startedAt = performance.now();
    const response = await fetch(`${worker.origin}/f/${funnel.slug}`);
    const body = await response.text();
    const elapsedMs = performance.now() - startedAt;
    assert.equal(response.status, 200, `GET /f/${funnel.slug} failed: ${body.slice(0, 300)}`);
    assert.match(response.headers.get("content-type") ?? "", /text\/html/);
    assert(body.length > 0, `GET /f/${funnel.slug} returned an empty body`);
    latencies.push(elapsedMs);
  }

  assert.equal(latencies.length, requestCount);
  const sorted = [...latencies].sort((left, right) => left - right);
  const p95Ms = sorted[Math.ceil(sorted.length * 0.95) - 1];
  const maxMs = sorted[sorted.length - 1];
  assert(p95Ms < 100, `Local GET /f/:slug p95 was ${p95Ms.toFixed(2)} ms (limit: <100 ms)`);
  console.log(
    JSON.stringify({
      localOnly: true,
      publishedFunnels: funnels.length,
      requests: latencies.length,
      p95Ms: Number(p95Ms.toFixed(2)),
      maxMs: Number(maxMs.toFixed(2)),
      thresholdMs: 100,
    }),
  );
} finally {
  if (worker) await worker.stop();
}
