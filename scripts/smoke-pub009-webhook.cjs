const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const net = require("node:net");
const path = require("node:path");
const { unstable_splitSqlQuery } = require("wrangler");
const { Miniflare, convertV4MiniflareOptions } = require("miniflare");

const ROOT = path.resolve(__dirname, "..");
const BUILD = path.join(ROOT, "apps/api/build");
const DELIVERY_ID_HEADER = "x-eezfunnels-delivery-id";
const TIMESTAMP_HEADER = "x-eezfunnels-timestamp";
const SIGNATURE_HEADER = "x-eezfunnels-signature";
const SIGNING_SECRET = "PUB-009 fictional webhook signing secret with 32 bytes";
const DESTINATION = "https://hooks.public-site.dev/receive";

function query(db, sql, ...parameters) {
  return db.prepare(sql).bind(...parameters);
}

async function availablePort() {
  const server = net.createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const { port } = server.address();
  await new Promise((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
  return port;
}

async function applyMigrations(db) {
  const migrationDirectory = path.join(ROOT, "migrations");
  const migrations = fs
    .readdirSync(migrationDirectory)
    .filter((name) => name.endsWith(".sql"))
    .sort();
  for (const name of migrations) {
    const text = fs.readFileSync(path.join(migrationDirectory, name), "utf8");
    const statements = unstable_splitSqlQuery(text);
    await db.batch(statements.map((statement) => db.prepare(statement)));
  }
}

function verifySignedRequest(request, expectedId) {
  assert.equal(request.method, "POST");
  assert.equal(request.headers.get("content-type"), "application/json");
  const deliveryId = request.headers.get(DELIVERY_ID_HEADER);
  const timestamp = request.headers.get(TIMESTAMP_HEADER);
  const signature = request.headers.get(SIGNATURE_HEADER);
  assert.equal(deliveryId, expectedId);
  assert.match(timestamp, /^[0-9]{1,12}$/);
  assert.match(signature, /^v1=[0-9a-f]{64}$/);
  return request.text().then((body) => {
    const expectedSignature = crypto
      .createHmac("sha256", SIGNING_SECRET)
      .update(`eezfunnel:webhook:v1\n${timestamp}\n${deliveryId}\n`)
      .update(body)
      .digest("hex");
    assert.equal(signature, `v1=${expectedSignature}`);
    const payload = JSON.parse(body);
    assert.equal(payload.event, "submission.received");
    assert.equal(payload.version, 1);
    assert.equal(payload.delivery_id, deliveryId);
    return { body, payload, deliveryId };
  });
}

async function main() {
  for (const file of ["index.js", "index_bg.wasm"]) {
    assert.ok(fs.existsSync(path.join(BUILD, file)), `missing built Worker module ${file}`);
  }

  const port = await availablePort();
  const outboundRequests = [];
  const receiverBodies = new Map();
  const receiverEffects = new Map();
  const responsePlans = [];
  const mf = new Miniflare(
    convertV4MiniflareOptions({
      rootPath: ROOT,
      name: "eezfunnel",
      modulesRoot: BUILD,
      modules: [
        { type: "ESModule", path: path.join(BUILD, "index.js") },
        { type: "CompiledWasm", path: path.join(BUILD, "index_bg.wasm") },
      ],
      bindings: {
        ENVIRONMENT: "development",
        ALLOWED_HOSTS: `127.0.0.1:${port}`,
        PUBLIC_BASE_URL: "https://funnels.example",
        PUBLIC_FUNNEL_HOSTS: "",
        PUBLIC_FORM_RETENTION_DAYS: "90",
        PUBLIC_FORM_SUBMISSIONS_PER_MINUTE: "100",
        PUBLIC_INTERACTION_RETENTION_DAYS: "30",
        PUBLIC_INTERACTION_EVENTS_PER_MINUTE: "100",
        WEBHOOK_SIGNING_SECRET: SIGNING_SECRET,
      },
      d1Databases: ["DB"],
      r2Buckets: ["R2"],
      kvNamespaces: ["KV"],
      compatibilityDate: "2026-09-01",
      compatibilityFlags: ["nodejs_compat"],
      unsafeTriggerHandlers: true,
      cf: false,
      port,
      logRequests: false,
      outboundService: async (request) => {
        assert.equal(new URL(request.url).origin, "https://hooks.public-site.dev");
        const expectedId = request.headers.get(DELIVERY_ID_HEADER);
        assert.ok(expectedId);
        const signed = await verifySignedRequest(request, expectedId);
        outboundRequests.push({ ...signed, headers: request.headers, url: request.url });
        const previousBody = receiverBodies.get(expectedId);
        if (previousBody === undefined) {
          receiverBodies.set(expectedId, signed.body);
        } else {
          assert.equal(previousBody, signed.body, "a logical delivery must keep deterministic bytes");
        }
        const plan = responsePlans.shift() ?? { status: 202 };
        if (plan.accept || (plan.status >= 200 && plan.status < 300)) {
          receiverEffects.set(expectedId, (receiverEffects.get(expectedId) ?? 0) + (receiverBodies.get(expectedId) === signed.body && !receiverEffects.has(expectedId) ? 1 : 0));
        }
        if (plan.delayMs) {
          await new Promise((resolve) => setTimeout(resolve, plan.delayMs));
        }
        if (plan.throw) {
          throw new Error("PUB-009 simulated response loss after receiver acceptance");
        }
        const headers = plan.location ? { location: plan.location } : undefined;
        return new Response(null, { status: plan.status, headers });
      },
    }),
  );

  try {
    await mf.ready;
    const db = await mf.getD1Database("DB");
    await applyMigrations(db);

    const createResponse = await mf.dispatchFetch(
      `http://127.0.0.1:${port}/api/funnels/v2`,
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "idempotency-key": "pub009-worker-smoke-create",
        },
        body: JSON.stringify({ name: "PUB-009 Worker transport smoke", blueprint: "lead_gen" }),
      },
    );
    assert.equal(createResponse.status, 201, await createResponse.clone().text());
    const created = await createResponse.json();
    const funnelId = created.funnel.id;
    const revisionId = created.initial_revision.id;
    const documentResponse = await mf.dispatchFetch(
      `http://127.0.0.1:${port}/api/funnels/${funnelId}/document`,
    );
    assert.equal(documentResponse.status, 200, await documentResponse.clone().text());
    const document = await documentResponse.json();
    const page = document.pages.find((candidate) =>
      candidate.sections.some((section) =>
        section.blocks.some((block) => block.kind === "form"),
      ),
    );
    const formBlock = page.sections.flatMap((section) => section.blocks).find((block) =>
      block.kind === "form",
    );
    const form = document.forms.find((candidate) => candidate.id === formBlock.form_id);
    const emailField = form.fields.find((field) => field.kind === "email");
    assert.ok(page && form && emailField, "the lead generation starter must include its public form");

    const now = Math.floor(Date.now() / 1000);
    await query(db, "UPDATE funnels SET published_revision_id = ? WHERE id = ?", revisionId, funnelId).run();
    const destinationId = crypto.randomUUID();
    const mappingId = crypto.randomUUID();
    await query(
      db,
      "INSERT INTO webhook_destination_versions (destination_version_id, destination_url, created_at) VALUES (?, ?, ?)",
      destinationId,
      DESTINATION,
      now,
    ).run();
    await query(
      db,
      "INSERT INTO public_form_delivery_mapping_versions (mapping_version_id, funnel_id, form_id, destination_version_id, selected_fields_json, created_at) VALUES (?, ?, ?, ?, ?, ?)",
      mappingId,
      funnelId,
      form.id,
      destinationId,
      JSON.stringify([emailField.id]),
      now,
    ).run();

    async function addTask(email) {
      const submissionId = crypto.randomUUID();
      const fingerprint = crypto.createHash("sha256").update(submissionId).digest("hex");
      const values = JSON.stringify({ [emailField.name]: email });
      await query(
        db,
        "INSERT INTO public_form_submissions (submission_id, funnel_id, revision_id, page_id, form_id, submission_key, request_fingerprint, field_values_json, request_consent_acknowledged, request_consent_copy_version, request_consent_copy, request_consent_recorded_at, submitted_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, 'request-v1', 'I agree to be contacted.', ?, ?, ?)",
        submissionId,
        funnelId,
        revisionId,
        page.id,
        form.id,
        submissionId,
        fingerprint,
        values,
        now,
        now,
        now + 86_400,
      ).run();
      await query(
        db,
        "INSERT INTO public_submission_delivery_outbox (task_id, source_submission_id, funnel_id, revision_id, page_id, form_id, destination_version_id, mapping_version_id, state, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?)",
        submissionId,
        submissionId,
        funnelId,
        revisionId,
        page.id,
        form.id,
        destinationId,
        mappingId,
        now,
      ).run();
      return submissionId;
    }

    async function triggerCron() {
      const response = await mf.dispatchFetch(
        `http://127.0.0.1:${port}/cdn-cgi/local/scheduled?cron=*+*+*+*+*`,
      );
      assert.equal(response.status, 200, await response.text());
    }

    async function stateFor(taskId) {
      return query(
        db,
        "SELECT status, attempt_count, next_attempt_at, last_http_status, last_outcome_code, completed_at FROM public_submission_delivery_state WHERE task_id = ?",
        taskId,
      ).first();
    }

    async function databaseNow() {
      return Number((await query(db, "SELECT unixepoch() AS now").first()).now);
    }

    const acceptedId = await addTask("accepted@example.test");
    responsePlans.push({ status: 202 });
    await triggerCron();
    const acceptedState = await stateFor(acceptedId);
    assert.equal(acceptedState.status, "accepted");
    assert.equal(acceptedState.attempt_count, 1);
    assert.equal(acceptedState.last_http_status, 202);
    assert.equal(acceptedState.last_outcome_code, "receiver_accepted");
    assert.ok(acceptedState.completed_at > 0);
    const acceptedRequest = outboundRequests.find((request) => request.deliveryId === acceptedId);
    assert.ok(acceptedRequest);
    assert.deepEqual(acceptedRequest.payload.submission.fields, { [emailField.id]: "accepted@example.test" });

    const retryId = await addTask("retry@example.test");
    responsePlans.push({ status: 429 });
    const retryBefore = await databaseNow();
    await triggerCron();
    const retryAfter = await databaseNow();
    const waitingRetry = await stateFor(retryId);
    assert.equal(waitingRetry.status, "pending");
    assert.equal(waitingRetry.attempt_count, 1);
    assert.equal(waitingRetry.last_http_status, 429);
    assert.equal(waitingRetry.last_outcome_code, "transport_retryable");
    assert.ok(
      waitingRetry.next_attempt_at >= retryBefore + 60 &&
        waitingRetry.next_attempt_at <= retryAfter + 60,
      waitingRetry,
    );
    await query(
      db,
      "UPDATE public_submission_delivery_state SET next_attempt_at = unixepoch() - 1 WHERE task_id = ?",
      retryId,
    ).run();
    responsePlans.push({ status: 202 });
    await triggerCron();
    assert.equal((await stateFor(retryId)).status, "accepted");
    const retryRequests = outboundRequests.filter((request) => request.deliveryId === retryId);
    assert.equal(retryRequests.length, 2);
    assert.equal(retryRequests[0].body, retryRequests[1].body);

    const serverErrorId = await addTask("server-error@example.test");
    responsePlans.push({ status: 503 });
    await triggerCron();
    const serverErrorState = await stateFor(serverErrorId);
    assert.equal(serverErrorState.status, "pending");
    assert.equal(serverErrorState.attempt_count, 1);
    assert.equal(serverErrorState.last_http_status, 503);
    assert.equal(serverErrorState.last_outcome_code, "transport_retryable");

    const redirectId = await addTask("redirect@example.test");
    const callsBeforeRedirect = outboundRequests.length;
    responsePlans.push({ status: 302, location: "https://redirected.public-site.dev/receive" });
    await triggerCron();
    const redirectState = await stateFor(redirectId);
    assert.equal(redirectState.status, "failed");
    assert.equal(redirectState.last_http_status, 302);
    assert.equal(redirectState.last_outcome_code, "redirect_refused");
    assert.equal(outboundRequests.length, callsBeforeRedirect + 1, "manual redirects must not be followed");
    assert.equal(new URL(outboundRequests.at(-1).url).hostname, "hooks.public-site.dev");

    const timeoutId = await addTask("timeout@example.test");
    responsePlans.push({ status: 202, accept: true, delayMs: 11_000 });
    const timeoutStartedAt = Date.now();
    await triggerCron();
    const timeoutElapsed = Date.now() - timeoutStartedAt;
    const timeoutState = await stateFor(timeoutId);
    assert.ok(timeoutElapsed >= 9_000 && timeoutElapsed <= 15_000, timeoutElapsed);
    assert.equal(timeoutState.status, "pending");
    assert.equal(timeoutState.attempt_count, 1);
    assert.equal(timeoutState.last_http_status, null);
    assert.equal(timeoutState.last_outcome_code, "transport_retryable");
    await query(
      db,
      "UPDATE public_submission_delivery_state SET next_attempt_at = unixepoch() - 1 WHERE task_id = ?",
      timeoutId,
    ).run();
    responsePlans.push({ status: 202 });
    await triggerCron();
    assert.equal((await stateFor(timeoutId)).status, "accepted");

    const lostResponseId = await addTask("response-lost@example.test");
    const callsBeforeLostResponse = outboundRequests.length;
    responsePlans.push({ status: 202, accept: true, throw: true });
    await triggerCron();
    const uncertainState = await stateFor(lostResponseId);
    assert.equal(uncertainState.status, "pending");
    assert.equal(uncertainState.last_http_status, 500);
    assert.equal(uncertainState.last_outcome_code, "transport_retryable");
    await query(
      db,
      "UPDATE public_submission_delivery_state SET next_attempt_at = unixepoch() - 1 WHERE task_id = ?",
      lostResponseId,
    ).run();
    responsePlans.push({ status: 202 });
    await triggerCron();
    assert.equal((await stateFor(lostResponseId)).status, "accepted");
    const lostResponseRequests = outboundRequests.slice(callsBeforeLostResponse);
    assert.equal(lostResponseRequests.length, 2);
    assert.equal(lostResponseRequests[0].deliveryId, lostResponseRequests[1].deliveryId);
    assert.equal(lostResponseRequests[0].body, lostResponseRequests[1].body);
    assert.equal(receiverEffects.get(lostResponseId), 1);

    const concurrentId = await addTask("concurrent@example.test");
    responsePlans.push({ status: 202 }, { status: 202 });
    const callsBeforeConcurrent = outboundRequests.length;
    await Promise.all([triggerCron(), triggerCron()]);
    assert.equal((await stateFor(concurrentId)).status, "accepted");
    assert.equal((await stateFor(concurrentId)).attempt_count, 1);
    assert.equal(
      outboundRequests.filter((request) => request.deliveryId === concurrentId).length,
      1,
    );
    assert.ok(outboundRequests.length >= callsBeforeConcurrent + 1);

    const recipeScenarios = [
      {
        recipe: "contact",
        fieldName: "project_details",
        value: "Discuss a service project",
        fields: { project_details: "Discuss a service project" },
      },
      {
        recipe: "quote_request",
        fieldName: "quote_scope",
        value: "Estimate a workspace project",
        fields: {
          quote_scope: "Estimate a workspace project",
          quote_timeline: "Before the next planning cycle",
        },
      },
      {
        recipe: "diagnostic_application",
        fieldName: "diagnostic_goal",
        value: "Clarify which intake process to improve first",
        fields: {
          current_situation: "Several intake processes are disconnected",
          diagnostic_goal: "Clarify which intake process to improve first",
        },
      },
    ];

    const destinationReadResponse = await mf.dispatchFetch(
      `http://127.0.0.1:${port}/api/webhook-delivery`,
    );
    assert.equal(destinationReadResponse.status, 200, await destinationReadResponse.clone().text());
    const destinationBefore = await destinationReadResponse.json();
    const destinationSaveKey = crypto.randomUUID();
    const destinationSaveResponse = await mf.dispatchFetch(
      `http://127.0.0.1:${port}/api/webhook-delivery`,
      {
        method: "PUT",
        headers: {
          "content-type": "application/json",
          "idempotency-key": destinationSaveKey,
        },
        body: JSON.stringify({
          expected_version_id: destinationBefore.destination_version_id,
          expected_enabled: destinationBefore.enabled,
          destination_url: DESTINATION,
          enabled: true,
        }),
      },
    );
    assert.equal(destinationSaveResponse.status, 200, await destinationSaveResponse.clone().text());
    const destination = await destinationSaveResponse.json();
    assert.equal(destination.enabled, true);
    assert.equal(destination.destination_version_id, destinationSaveKey);
    assert.equal("signing_secret" in destination, false);

    const recipeTasks = [];
    for (const scenario of recipeScenarios) {
      const recipeCreateResponse = await mf.dispatchFetch(
        `http://127.0.0.1:${port}/api/funnels/v2`,
        {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "idempotency-key": `pub019-recipe-${scenario.recipe}`,
          },
          body: JSON.stringify({
            name: `PUB-019 ${scenario.recipe}`,
            blueprint: "lead_gen",
            service_recipe: scenario.recipe,
          }),
        },
      );
      assert.equal(recipeCreateResponse.status, 201, await recipeCreateResponse.clone().text());
      const recipeCreated = await recipeCreateResponse.json();
      const recipeFunnelId = recipeCreated.funnel.id;
      const recipeRevisionId = recipeCreated.initial_revision.id;
      const publishResponse = await mf.dispatchFetch(
        `http://127.0.0.1:${port}/api/funnels/${recipeFunnelId}/publish`,
        {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "idempotency-key": crypto.randomUUID(),
          },
          body: JSON.stringify({
            revision_id: recipeRevisionId,
            expected_published_revision_id: null,
          }),
        },
      );
      assert.equal(publishResponse.status, 200, await publishResponse.clone().text());

      const recipeDocumentResponse = await mf.dispatchFetch(
        `http://127.0.0.1:${port}/api/funnels/${recipeFunnelId}/document`,
      );
      assert.equal(recipeDocumentResponse.status, 200, await recipeDocumentResponse.clone().text());
      const recipeDocument = await recipeDocumentResponse.json();
      const recipePage = recipeDocument.pages.find((candidate) =>
        candidate.sections.some((section) =>
          section.blocks.some((block) => block.kind === "form"),
        ),
      );
      const recipeFormBlock = recipePage.sections.flatMap((section) => section.blocks).find(
        (block) => block.kind === "form",
      );
      const recipeForm = recipeDocument.forms.find((candidate) => candidate.id === recipeFormBlock.form_id);
      const selectedField = recipeForm.fields.find((field) => field.name === scenario.fieldName);
      assert.ok(selectedField, `${scenario.recipe} must expose its selected delivery field`);

      const recipeDeliveryRead = await mf.dispatchFetch(
        `http://127.0.0.1:${port}/api/funnels/${recipeFunnelId}/webhook-delivery`,
      );
      assert.equal(recipeDeliveryRead.status, 200, await recipeDeliveryRead.clone().text());
      const recipeDelivery = await recipeDeliveryRead.json();
      const mappedForm = recipeDelivery.forms.find((candidate) => candidate.form_id === recipeForm.id);
      assert.ok(mappedForm, `${scenario.recipe} must be available for delivery configuration`);
      const emailField = recipeForm.fields.find((field) => field.kind === "email");
      assert.ok(emailField, `${scenario.recipe} must require an email field`);
      const selectedFieldPairs = [
        { id: emailField.id, name: emailField.name },
        { id: selectedField.id, name: selectedField.name },
      ].sort((left, right) => left.id.localeCompare(right.id));
      const selectedFields = selectedFieldPairs.map(({ id }) => id);
      const recipeMappingKey = crypto.randomUUID();
      const recipeMappingResponse = await mf.dispatchFetch(
        `http://127.0.0.1:${port}/api/funnels/${recipeFunnelId}/webhook-delivery`,
        {
          method: "PUT",
          headers: {
            "content-type": "application/json",
            "idempotency-key": recipeMappingKey,
          },
          body: JSON.stringify({
            form_id: recipeForm.id,
            expected_published_revision_id: recipeRevisionId,
            destination_version_id: destination.destination_version_id,
            expected_mapping_version_id: mappedForm.mapping_version_id,
            enabled: true,
            selected_field_ids: selectedFields,
          }),
        },
      );
      assert.equal(recipeMappingResponse.status, 200, await recipeMappingResponse.clone().text());
      const savedMapping = await recipeMappingResponse.json();
      const savedForm = savedMapping.forms.find((candidate) => candidate.form_id === recipeForm.id);
      assert.equal(savedForm.enabled, true);
      assert.equal(savedForm.mapping_compatible, true);
      assert.deepEqual(savedForm.selected_field_ids, selectedFields);

      const submissionKey = crypto.randomUUID();
      const actionQuery = new URLSearchParams({
        key: submissionKey,
        revision_id: recipeRevisionId,
        channel: "unattributed",
      });
      const submissionPath =
        `/api/public/funnels/${recipeCreated.funnel.slug}/pages/${recipePage.id}` +
        `/forms/${recipeForm.id}/submissions?${actionQuery}`;
      const recipeValues = Object.fromEntries(
        recipeForm.fields.map((field) => [
          field.name,
          field.name === "name"
            ? "Example service request"
            : field.name === "email"
              ? `${scenario.recipe}@example.test`
              : scenario.fields[field.name] ?? "Additional fictional context",
        ]),
      );
      const submitResponse = await mf.dispatchFetch(
        `http://127.0.0.1:${port}${submissionPath}`,
        {
          method: "POST",
          redirect: "manual",
          headers: {
            "content-type": "application/x-www-form-urlencoded",
            accept: "text/html",
            origin: `http://127.0.0.1:${port}`,
          },
          body: new URLSearchParams({ ...recipeValues, _request_consent: "on" }).toString(),
        },
      );
      assert.equal(submitResponse.status, 303, await submitResponse.clone().text());
      assert.match(submitResponse.headers.get("location") ?? "", /\/request-received$/);

      const storedSubmission = await query(
        db,
        "SELECT submission_id FROM public_form_submissions WHERE funnel_id = ? AND json_extract(field_values_json, '$.email') = ?",
        recipeFunnelId,
        `${scenario.recipe}@example.test`,
      ).first();
      assert.ok(storedSubmission, `${scenario.recipe} public POST must create an inbox submission`);
      const recipeSubmissionId = storedSubmission.submission_id;
      const recipeOutbox = await query(
        db,
        "SELECT task_id, source_submission_id, revision_id, destination_version_id, mapping_version_id FROM public_submission_delivery_outbox WHERE source_submission_id = ?",
        recipeSubmissionId,
      ).first();
      assert.ok(recipeOutbox, `${scenario.recipe} public POST must create a delivery task`);
      assert.equal(recipeOutbox.source_submission_id, recipeSubmissionId);
      assert.equal(recipeOutbox.revision_id, recipeRevisionId);
      assert.equal(recipeOutbox.destination_version_id, destination.destination_version_id);
      assert.equal(recipeOutbox.mapping_version_id, recipeMappingKey);
      recipeTasks.push({
        recipe: scenario.recipe,
        taskId: recipeOutbox.task_id,
        selectedFieldPairs,
        fieldValues: recipeValues,
      });
    }

    responsePlans.push({ status: 202 }, { status: 202 }, { status: 202 });
    await triggerCron();
    for (const recipeTask of recipeTasks) {
      assert.equal((await stateFor(recipeTask.taskId)).status, "accepted", recipeTask.recipe);
      const deliveredRequest = outboundRequests.find(
        (request) => request.deliveryId === recipeTask.taskId,
      );
      assert.ok(deliveredRequest, `${recipeTask.recipe} must reach the local receiver`);
      const expectedFields = Object.fromEntries(
        recipeTask.selectedFieldPairs.map(({ id, name }) => [id, recipeTask.fieldValues[name]]),
      );
      assert.deepEqual(deliveredRequest.payload.submission.fields, expectedFields);
      assert.equal(receiverEffects.get(recipeTask.taskId), 1, `${recipeTask.recipe} has one accepted receiver effect`);
    }

    console.log(
      "PUB-009 Worker/D1: HMAC, selected payload, 429/503/timeout retry, manual redirect, uncertain response dedupe, and concurrent claim passed",
    );
    console.log("PUB-019 recipes: contact, estimate, and diagnostic requests were signed and delivered locally");
  } finally {
    await mf.dispose();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
