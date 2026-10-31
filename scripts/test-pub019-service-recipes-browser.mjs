import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { chromium } from "@playwright/test";
import { startPr019Worker } from "./pr019-local-worker.mjs";
import { finalizePr019Browser } from "./pr019-browser-finalizer.mjs";

const scenarios = [
  {
    recipe: "contact",
    name: "PUB-019 contact recipe",
    personalizedHeading: "A clear service offer for local teams",
    purpose: "Service inquiry",
    prompt: "Describe your project",
    submit: "Send request",
    confirmation: "Your request has been received.",
    value: "I would like to discuss a service project.",
  },
  {
    recipe: "quote_request",
    name: "PUB-019 estimate recipe",
    personalizedHeading: "Request a scoped estimate for local teams",
    purpose: "Estimate request",
    prompt: "Describe the scope for an estimate",
    optional: "Preferred timeline (optional)",
    submit: "Request an estimate",
    confirmation: "Your estimate request has been received.",
    value: "Planning and delivery for a two-room workspace.",
  },
  {
    recipe: "diagnostic_application",
    name: "PUB-019 diagnostic recipe",
    personalizedHeading: "Request a diagnostic discussion for local teams",
    purpose: "Diagnostic request",
    prompt: "Describe the current situation",
    secondaryPrompt: "What would you like to clarify?",
    submit: "Send diagnostic request",
    confirmation: "Your request has been received for review.",
    value: "The team has several disconnected project intake processes.",
    secondaryValue: "Which process should be standardized first?",
  },
];

const diagnostics = [];
let browser;
let worker;

async function closeBrowser() {
  const activeBrowser = browser;
  if (activeBrowser) {
    await activeBrowser.close();
    browser = undefined;
  }
}

async function responseJson(response, expectedStatus, label) {
  const text = await response.text();
  const status = typeof response.status === "function" ? response.status() : response.status;
  assert.equal(status, expectedStatus, `${label}: ${text}`);
  return JSON.parse(text);
}

try {
  worker = await startPr019Worker("service-recipes", { onSignalCleanup: closeBrowser });
  const executablePath = await access("/usr/bin/chromium").then(
    () => "/usr/bin/chromium",
    () => undefined,
  );
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
  const dashboard = await browser.newPage({ viewport: { width: 1360, height: 1000 } });
  dashboard.on("pageerror", (error) => diagnostics.push(error.stack ?? error.message));

  for (const scenario of scenarios) {
    await dashboard.goto(`${worker.origin}/funnels`, { waitUntil: "networkidle" });
    await dashboard.getByRole("heading", { name: "Your funnels" }).waitFor();
    await dashboard.getByRole("button", { name: "Create funnel" }).click();
    const createDialog = dashboard.getByRole("dialog", { name: "Create a funnel" });
    await createDialog.waitFor({ state: "visible" });
    await dashboard.getByLabel("Funnel name").fill(scenario.name);
    await dashboard.locator('input[name="blueprint"][value="lead_gen"]').check();
    const recipeInput = dashboard.locator(
      `input[name="service_recipe"][value="${scenario.recipe}"]`,
    );
    await recipeInput.waitFor();
    await recipeInput.check();

    const createRequestPromise = dashboard.waitForRequest(
      (request) => request.method() === "POST" && request.url().endsWith("/api/funnels/v2"),
    );
    const createResponsePromise = dashboard.waitForResponse(
      (response) => response.request().method() === "POST" && response.url().endsWith("/api/funnels/v2"),
    );
    await createDialog.getByRole("button", { name: "Create funnel" }).click();
    const [createRequest, createResponse] = await Promise.all([
      createRequestPromise,
      createResponsePromise,
    ]);
    assert.equal(createRequest.postDataJSON()?.blueprint, "lead_gen");
    assert.equal(createRequest.postDataJSON()?.service_recipe, scenario.recipe);
    const created = await responseJson(createResponse, 201, `Create ${scenario.recipe} funnel`);
    assert.equal(typeof created.funnel?.id, "string");
    assert.equal(typeof created.initial_revision?.id, "string");
    await createDialog.waitFor({ state: "hidden" });
    await dashboard.getByRole("link", { name: scenario.name }).waitFor();

    await dashboard.goto(`${worker.origin}/funnels/${created.funnel.id}`, { waitUntil: "networkidle" });
    await dashboard.getByRole("heading", { name: scenario.name }).waitFor();
    await dashboard.getByRole("region", { name: "Typed visual editor" }).waitFor();
    await dashboard.getByRole("button", { name: "Hero", exact: true }).click();
    const heroTitle = dashboard.getByLabel("Heading content for block-hero-title");
    const previewResponse = dashboard.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/api/funnels/${created.funnel.id}/edit-v2`),
    );
    await heroTitle.fill(scenario.personalizedHeading);
    await heroTitle.blur();
    assert.equal((await previewResponse).status(), 200);
    await dashboard.frameLocator("iframe")
      .getByRole("heading", { name: scenario.personalizedHeading }).waitFor();

    const saveResponsePromise = dashboard.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/api/funnels/${created.funnel.id}/revisions-v2`),
    );
    await dashboard.getByRole("button", { name: "Save revision" }).click();
    const savedRevision = await responseJson(
      await saveResponsePromise,
      201,
      `Save personalized ${scenario.recipe} funnel`,
    );
    assert.equal(typeof savedRevision.revision_id, "string");
    await dashboard.getByText("Revision saved.").waitFor();

    const publishResponsePromise = dashboard.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/api/funnels/${created.funnel.id}/publish`),
    );
    await dashboard.getByRole("button", { name: "Publish saved revision" }).click();
    await responseJson(await publishResponsePromise, 200, `Publish ${scenario.recipe} funnel`);
    await dashboard.getByText("Revision published.").waitFor();

    const visitorContext = await browser.newContext({
      viewport: { width: 390, height: 844 },
      javaScriptEnabled: false,
    });
    const visitor = await visitorContext.newPage();
    const publicResponse = await visitor.goto(`${worker.origin}/f/${created.funnel.slug}`, {
      waitUntil: "networkidle",
    });
    assert.equal(publicResponse?.status(), 200);
    await visitor.getByRole("heading", { name: scenario.personalizedHeading }).waitFor();
    const form = visitor.locator("form.v2-form").filter({ has: visitor.getByLabel("Email address") });
    await form.getByLabel("Your name").fill("Morgan Example");
    const email = form.getByLabel("Email address");
    await email.evaluate((element) => {
      element.type = "text";
      element.required = false;
    });
    await form.evaluate((element) => {
      element.noValidate = true;
    });
    await email.fill("not-an-email");
    await form.getByLabel(scenario.prompt).fill(scenario.value);
    if (scenario.optional) {
      await form.getByLabel(scenario.optional).fill("Before the next planning cycle");
    }
    if (scenario.secondaryPrompt) {
      await form.getByLabel(scenario.secondaryPrompt).fill(scenario.secondaryValue);
    }
    await form
      .getByLabel("I have read and understood the information about this request.")
      .check();

    const initialAction = new URL(await form.getAttribute("action"), visitor.url());
    const submissionKey = initialAction.searchParams.get("key");
    assert.ok(submissionKey);
    const validationResponsePromise = visitor.waitForResponse(
      (response) => response.request().method() === "POST" && response.url().includes("/submissions"),
    );
    await form.getByRole("button", { name: scenario.submit }).click();
    const validationResponse = await validationResponsePromise;
    assert.equal(validationResponse.status(), 422);
    const formId = initialAction.pathname.split("/").at(-2);
    await visitor.waitForFunction((id) => document.activeElement?.id === id, `form-${formId}-errors`);
    const errorSummary = visitor.getByRole("alert");
    await errorSummary.waitFor();
    assert.equal(await form.getByLabel("Your name").inputValue(), "Morgan Example");
    assert.equal(await email.inputValue(), "not-an-email");
    await visitor.getByRole("link", { name: /Email address:/ }).click();
    await email.fill(`${scenario.recipe}@example.test`);

    const retryAction = new URL(await form.getAttribute("action"), visitor.url());
    assert.equal(retryAction.searchParams.get("key"), submissionKey);
    assert.equal(retryAction.searchParams.has("email"), false);
    const submissionResponsePromise = visitor.waitForResponse(
      (response) => response.request().method() === "POST" && response.url().includes("/submissions"),
    );
    await form.getByRole("button", { name: scenario.submit }).click();
    const submissionResponse = await submissionResponsePromise;
    assert.equal(submissionResponse.status(), 303);
    await visitor.waitForURL((url) => url.pathname.endsWith("/request-received"));
    await visitor.getByRole("heading", { name: scenario.confirmation }).waitFor();

    const inboxResponse = await fetch(
      `${worker.origin}/api/submissions?funnel_id=${encodeURIComponent(created.funnel.id)}`,
    );
    const inbox = await responseJson(inboxResponse, 200, `Read ${scenario.recipe} inbox`);
    assert.equal(inbox.items.length, 1);
    const submission = inbox.items[0];
    assert.equal(submission.form_purpose, scenario.purpose);
    assert.equal(submission.request_consent_acknowledged, true);
    assert.ok(submission.fields.some((field) => field.value === scenario.value));

    const treatmentResponse = await fetch(
      `${worker.origin}/api/submissions/${submission.submission_id}/status`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ status: "contacted" }),
      },
    );
    const treatment = await responseJson(treatmentResponse, 200, `Record ${scenario.recipe} follow-up`);
    assert.equal(treatment.status, "contacted");
    await visitorContext.close();
  }

  assert.deepEqual(diagnostics, [], `Browser errors: ${diagnostics.join("\n")}`);
  console.log(
    JSON.stringify({
      recipes: scenarios.map(({ recipe }) => recipe),
      workflow: "dashboard recipe selection -> personalization/save/publish -> HTML validation recovery -> public submission -> confirmation -> inbox -> owner follow-up",
      submissions: scenarios.length,
      trackedAsBlueprint: "lead_gen",
      diagnostics: diagnostics.length,
    }),
  );
} finally {
  await finalizePr019Browser(closeBrowser, worker);
}
