import assert from "node:assert/strict";
import { access } from "node:fs/promises";
import { createRequire } from "node:module";
import { chromium } from "@playwright/test";

const require = createRequire(import.meta.url);
const axeSource = require("axe-core").source;
const pageUrl = process.argv[2];
const funnelId = process.argv[3];
const emptyFunnelId = process.argv[4];
assert(pageUrl, "Pass the local submissions dashboard URL");
assert(funnelId && emptyFunnelId, "Pass the matching and empty funnel IDs");

const executablePath = await access("/usr/bin/chromium").then(
  () => "/usr/bin/chromium",
  () => undefined,
);
const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.goto(pageUrl, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Submissions", level: 1 }).waitFor({ state: "visible" });
  await page.getByLabel("Receiver HTTPS URL").waitFor({ state: "visible" });
  await page.locator("#webhook-funnel-select").waitFor({ state: "visible" });
  assert.deepEqual(pageErrors, []);
  const entries = page.locator("details.submission-entry");
  await entries.first().waitFor({ state: "visible" });
  assert.equal(await entries.count(), 25);

  const graceEntry = entries.filter({ hasText: "Grace Hopper" });
  assert.equal(await graceEntry.count(), 1);
  await graceEntry.locator(":scope > summary").click();
  await page.getByText("Grace Hopper", { exact: true }).waitFor({ state: "visible" });
  await page.getByRole("heading", { name: "Consent record" }).first().waitFor({ state: "visible" });
  const consentCopy = graceEntry.getByText("I agree to be contacted about this updated request.", { exact: true });
  await consentCopy.waitFor({ state: "visible" });
  assert.equal(await consentCopy.count(), 1);
  assert.equal(await graceEntry.getByText("Available until", { exact: false }).count(), 1);

  const graceStatus = graceEntry.getByRole("combobox", {
    name: /^Follow-up status for/,
  });
  assert.equal(await graceStatus.count(), 1);
  const statusUpdate = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" &&
      response.url().includes("/api/submissions/") &&
      response.url().endsWith("/follow-up"),
  );
  await graceStatus.selectOption("qualified");
  await graceEntry.getByLabel("Next action").fill("Send the qualification email");
  await graceEntry.getByLabel("Due date (optional)").fill("2028-03-15");
  await graceEntry.getByRole("button", { name: "Save follow-up" }).click();
  const statusResponse = await statusUpdate;
  const statusBody = await statusResponse.text();
  assert.equal(statusResponse.status(), 200, statusBody);
  const statusResult = JSON.parse(statusBody);
  const submissionActionControl = page.locator(`#submission-next-action-${statusResult.submission_id}`);
  await submissionActionControl.waitFor({ state: "attached", timeout: 5000 }).catch(async (error) => {
    const visibleContent = await page.locator("main").innerText().catch(() => "<main unavailable>");
    throw new Error(`${error.message}; inbox count=${await entries.count()}; content=${visibleContent}`);
  });
  const submissionEntry = submissionActionControl.locator("xpath=ancestor::details[contains(concat(' ', normalize-space(@class), ' '), ' submission-entry ')]");
  await submissionEntry.getByRole("button", { name: "Save follow-up" }).waitFor({ state: "visible" });
  const originalFollowUpBody = JSON.parse(statusResponse.request().postData());
  assert.equal(statusResult.version, originalFollowUpBody.expected_version + 1);
  assert.equal(statusResult.replayed, false);
  const followUpIdempotencyKey = statusResponse.request().headers()["idempotency-key"];
  assert.ok(followUpIdempotencyKey);
  const replayedUpdate = await page.evaluate(async ({ key, body, submissionId }) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": key },
      body: JSON.stringify(body),
    });
    return { status: response.status, body: await response.json() };
  }, { key: followUpIdempotencyKey, body: originalFollowUpBody, submissionId: statusResult.submission_id });
  assert.equal(replayedUpdate.status, 200, JSON.stringify(replayedUpdate.body));
  assert.equal(replayedUpdate.body.version, statusResult.version);
  assert.equal(replayedUpdate.body.replayed, true);
  const reusedKey = await page.evaluate(async ({ key, body, submissionId }) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": key },
      body: JSON.stringify({ ...body, status: "closed" }),
    });
    return { status: response.status, body: await response.json() };
  }, { key: followUpIdempotencyKey, body: originalFollowUpBody, submissionId: statusResult.submission_id });
  assert.equal(reusedKey.status, 409);
  assert.equal(reusedKey.body.code, "conflict/idempotency_payload_mismatch");
  const otherSubmissionId = await page.evaluate(async (currentSubmissionId) => {
    const response = await fetch("/api/submissions");
    const list = await response.json();
    return list.items.find(({ submission_id }) => submission_id !== currentSubmissionId)?.submission_id;
  }, statusResult.submission_id);
  assert.ok(otherSubmissionId);
  const reusedKeyAcrossSubmissions = await page.evaluate(async ({ key, submissionId }) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": key },
      body: JSON.stringify({
        expected_version: 0,
        status: "new",
        next_action: null,
        due_on: null,
        close_reason: null,
      }),
    });
    return { status: response.status, body: await response.json() };
  }, { key: followUpIdempotencyKey, submissionId: otherSubmissionId });
  assert.equal(reusedKeyAcrossSubmissions.status, 409);
  assert.equal(reusedKeyAcrossSubmissions.body.code, "conflict/idempotency_payload_mismatch");
  const staleUpdate = await page.evaluate(async ({ body, submissionId }) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": "pub011-stale-version" },
      body: JSON.stringify({ ...body, expected_version: body.expected_version - 1, status: "contacted" }),
    });
    return { status: response.status, body: await response.json() };
  }, { body: originalFollowUpBody, submissionId: statusResult.submission_id });
  assert.equal(staleUpdate.status, 409);
  assert.equal(staleUpdate.body.code, "conflict/submission_follow_up_stale");
  const noOpUpdate = await page.evaluate(async ({ submissionId, currentVersion }) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": "pub011-no-op" },
      body: JSON.stringify({
        expected_version: currentVersion,
        status: "qualified",
        next_action: "Send the qualification email",
        due_on: "2028-03-15",
        close_reason: null,
      }),
    });
    return { status: response.status, body: await response.json() };
  }, { submissionId: statusResult.submission_id, currentVersion: statusResult.version });
  assert.equal(noOpUpdate.status, 200, JSON.stringify(noOpUpdate.body));
  assert.equal(noOpUpdate.body.version, statusResult.version);
  assert.equal(noOpUpdate.body.replayed, false);
  const concurrentUpdates = await page.evaluate(async ({ submissionId, expectedVersion }) => {
    const update = async (key, nextAction) => {
      const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
        method: "PUT",
        headers: { "Content-Type": "application/json", "Idempotency-Key": key },
        body: JSON.stringify({
          expected_version: expectedVersion,
          status: "qualified",
          next_action: nextAction,
          due_on: "2028-03-15",
          close_reason: null,
        }),
      });
      return { status: response.status, body: await response.json() };
    };
    return await Promise.all([
      update("pub011-concurrent-a", "Call the prospect"),
      update("pub011-concurrent-b", "Send the revised proposal"),
    ]);
  }, { submissionId: statusResult.submission_id, expectedVersion: statusResult.version });
  assert.deepEqual(concurrentUpdates.map(({ status }) => status).sort(), [200, 409]);
  const concurrentWinner = concurrentUpdates.find(({ status }) => status === 200);
  const concurrentLoser = concurrentUpdates.find(({ status }) => status === 409);
  assert.equal(concurrentWinner.body.version, statusResult.version + 1);
  assert.equal(concurrentLoser.body.code, "conflict/submission_follow_up_stale");
  if (!(await submissionEntry.evaluate((entry) => entry.open))) {
    await submissionEntry.locator(":scope > summary").click();
  }
  const historyToggle = submissionEntry.locator(".submission-follow-up-history > button");
  if ((await historyToggle.innerText()) === "Show follow-up history") {
    await historyToggle.click();
  }
  await submissionEntry.getByRole("button", { name: "Hide history" }).waitFor({ state: "visible" });
  await submissionEntry.getByText(`Version ${statusResult.version}: Qualified`, { exact: true }).waitFor({ state: "visible" });
  await submissionEntry.getByText("Next action: Send the qualification email", { exact: true }).waitFor({ state: "visible" });
  await submissionEntry.getByLabel("Next action").fill("Keep this unsaved note");
  await submissionEntry.getByRole("button", { name: "Hide history" }).click();
  await historyToggle.click();
  await submissionEntry.getByText(`Version ${statusResult.version + 1}: Qualified`, { exact: true }).waitFor({ state: "visible" });
  assert.equal(await submissionEntry.getByLabel("Next action").inputValue(), "Keep this unsaved note");
  const correctableEntry = submissionEntry.locator(".submission-follow-up-timeline li").filter({
    hasText: `Version ${statusResult.version}: Qualified`,
  });
  await correctableEntry.getByRole("button", { name: "Correct this entry" }).click();
  await submissionEntry.getByText(`This records a correction to version ${statusResult.version}.`, { exact: false }).waitFor({ state: "visible" });
  assert.equal(await submissionEntry.getByLabel("Next action").inputValue(), "Keep this unsaved note");
  await submissionEntry.getByRole("button", { name: "Cancel correction" }).click();
  const changedAfterDraft = await page.evaluate(async ({ submissionId, expectedVersion }) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": "pub011-changed-after-draft" },
      body: JSON.stringify({
        expected_version: expectedVersion,
        status: "qualified",
        next_action: "Record the follow-up call",
        due_on: "2028-03-15",
        close_reason: null,
      }),
    });
    return { status: response.status, body: await response.json() };
  }, { submissionId: statusResult.submission_id, expectedVersion: statusResult.version + 1 });
  assert.equal(changedAfterDraft.status, 200, JSON.stringify(changedAfterDraft.body));
  assert.equal(changedAfterDraft.body.version, statusResult.version + 2);
  const staleDraftSave = page.waitForResponse((response) =>
    response.request().method() === "PUT" && response.url().endsWith("/follow-up"),
  );
  await submissionEntry.getByRole("button", { name: "Save follow-up" }).click();
  const staleDraftResponse = await staleDraftSave;
  const staleDraftBody = await staleDraftResponse.text();
  const staleDraftRequestBody = JSON.parse(staleDraftResponse.request().postData());
  const latestAfterStaleSave = await page.evaluate(async (submissionId) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`);
    return await response.json();
  }, statusResult.submission_id);
  assert.equal(staleDraftResponse.status(), 409, JSON.stringify({
    response: staleDraftBody,
    request: staleDraftRequestBody,
    visibleDraft: await submissionEntry.getByLabel("Next action").inputValue(),
    latest: latestAfterStaleSave.current,
  }));
  await submissionEntry.getByRole("button", { name: "I reviewed the latest history" }).waitFor({ state: "visible" });
  assert.equal(await submissionEntry.getByLabel("Next action").inputValue(), "Keep this unsaved note");

  const statusFilter = page.getByRole("combobox", { name: "Filter submissions by status" });
  const contactedList = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().includes("/api/submissions?status=contacted"),
  );
  await statusFilter.selectOption("contacted");
  await contactedList;
  await page.getByRole("heading", { name: "No contacted submissions" }).waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 0);
  assert.equal(await page.getByText("Grace Hopper", { exact: true }).count(), 0);
  const pinnedDraft = page.locator(".submission-draft-workspace");
  await pinnedDraft.getByLabel("Next action").waitFor({ state: "visible" });
  assert.equal(await pinnedDraft.getByLabel("Next action").inputValue(), "Keep this unsaved note");
  await pinnedDraft.getByRole("button", { name: "I reviewed the latest history" }).click();
  const recoveredSave = page.waitForResponse((response) =>
    response.request().method() === "PUT" && response.url().endsWith("/follow-up"),
  );
  await pinnedDraft.getByRole("button", { name: "Save follow-up" }).click();
  const recoveredSaveResponse = await recoveredSave;
  const recoveredSaveResult = await recoveredSaveResponse.json();
  assert.equal(recoveredSaveResponse.status(), 200, JSON.stringify(recoveredSaveResult));
  assert.equal(recoveredSaveResult.version, statusResult.version + 3);
  const recoveredWorkspace = page.locator(".submission-draft-workspace");
  await recoveredWorkspace.getByRole("button", { name: "Hide history" }).waitFor({ state: "visible" });
  await recoveredWorkspace.getByRole("button", { name: "Hide history" }).click();
  await page.locator(".submission-draft-workspace").waitFor({ state: "detached" });

  const qualifiedList = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().includes("/api/submissions?status=qualified"),
  );
  await statusFilter.selectOption("qualified");
  await qualifiedList;
  await page.locator("details.submission-entry .submission-status-badge").getByText("Qualified", { exact: true }).waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 1);
  const qualifiedEntry = page.locator("details.submission-entry").filter({
    has: page.locator(".submission-status-badge").getByText("Qualified", { exact: true }),
  });
  await qualifiedEntry.locator(":scope > summary").click();
  await qualifiedEntry.getByText("Grace Hopper", { exact: true }).waitFor({ state: "visible" });

  const closedList = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().includes("/api/submissions?status=closed"),
  );
  await statusFilter.selectOption("closed");
  await closedList;
  await page.getByRole("heading", { name: "No closed submissions" }).waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 0);

  const newList = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().includes("/api/submissions?status=new"),
  );
  await statusFilter.selectOption("new");
  await newList;
  await page.getByRole("button", { name: "Load older submissions" }).waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 25);
  await page.getByRole("button", { name: "Load older submissions" }).click();
  await page.getByText("All retained submissions are shown.").waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 27);
  const secondPageEntry = page.locator("details.submission-entry").nth(26);
  await secondPageEntry.locator(":scope > summary").click();
  const secondPageDraft = `Keep page two draft ${statusResult.submission_id}`;
  await secondPageEntry.getByLabel("Next action").fill(secondPageDraft);
  const firstPageEntry = page.locator("details.submission-entry").first();
  await firstPageEntry.locator(":scope > summary").click();
  await firstPageEntry.getByLabel("Next action").fill("Confirm an uncertain save");
  let loseNextFollowUpResponse = true;
  let lostFollowUpRequest;
  let retriedFollowUpRequest;
  await page.route("**/api/submissions/*/follow-up", async (route) => {
    if (route.request().method() !== "PUT") {
      await route.continue();
      return;
    }
    const request = {
      body: route.request().postData(),
      key: route.request().headers()["idempotency-key"],
    };
    if (loseNextFollowUpResponse) {
      const committed = await route.fetch();
      assert.equal(committed.status(), 200);
      lostFollowUpRequest = request;
      loseNextFollowUpResponse = false;
      await route.abort();
    } else {
      retriedFollowUpRequest = request;
      await route.continue();
    }
  });
  await firstPageEntry.getByRole("button", { name: "Save follow-up" }).click();
  await firstPageEntry.getByText("The server may have saved this update.", { exact: false }).waitFor({ state: "visible" });
  const historyRequest = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().endsWith("/follow-up"),
  );
  await firstPageEntry.getByRole("button", { name: "Show follow-up history" }).click();
  await historyRequest;
  await firstPageEntry.getByRole("button", { name: "Hide history" }).waitFor({ state: "visible" });
  const retryButton = firstPageEntry.getByRole("button", { name: "Retry save" });
  await retryButton.waitFor({ state: "visible" });
  assert.equal(await retryButton.isDisabled(), false);
  const replayedSave = page.waitForResponse((response) =>
    response.request().method() === "PUT" && response.url().endsWith("/follow-up"),
  );
  await retryButton.click();
  const replayedSaveResponse = await replayedSave;
  const replayedSaveBody = await replayedSaveResponse.json();
  assert.equal(replayedSaveResponse.status(), 200, JSON.stringify(replayedSaveBody));
  assert.equal(replayedSaveBody.replayed, true);
  assert.deepEqual(retriedFollowUpRequest, lostFollowUpRequest);
  await page.unroute("**/api/submissions/*/follow-up");
  const pinnedPageTwoDraft = page.locator(".submission-draft-workspace");
  await pinnedPageTwoDraft.getByLabel("Next action").waitFor({ state: "visible" });
  assert.equal(await pinnedPageTwoDraft.getByLabel("Next action").inputValue(), secondPageDraft);
  await pinnedPageTwoDraft.getByRole("button", { name: "Discard draft" }).click();
  await page.locator(".submission-draft-workspace").waitFor({ state: "detached" });

  const allList = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().endsWith("/api/submissions"),
  );
  await statusFilter.selectOption("all");
  await allList;
  await page.getByRole("button", { name: "Load older submissions" }).waitFor({ state: "visible" });
  assert.equal(await page.locator("details.submission-entry").count(), 25);

  await page.getByRole("button", { name: "Load older submissions" }).click();
  await page.getByText("All retained submissions are shown.").waitFor({ state: "visible" });
  assert.equal(await entries.count(), 28);
  assert.equal(await page.getByText("Inbox fixture 01", { exact: true }).count(), 1);

  const funnelFilter = page.getByRole("combobox", { name: "Filter submissions by funnel" });
  const scopedList = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?funnel_id=${funnelId}`),
  );
  await funnelFilter.selectOption(funnelId);
  await scopedList;
  await page.locator("details.submission-entry").first().waitFor({ state: "visible" });
  assert.equal(await entries.count(), 25);
  assert.ok((await entries.first().innerText()).includes("PR-033 local form smoke"));

  const combinedFilters = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?status=qualified&funnel_id=${funnelId}`),
  );
  await statusFilter.selectOption("qualified");
  await combinedFilters;
  await entries.filter({ hasText: "Grace Hopper" }).waitFor({ state: "visible" });
  assert.equal(await entries.count(), 1);

  const filteredPages = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?status=new&funnel_id=${funnelId}`),
  );
  await statusFilter.selectOption("new");
  await filteredPages;
  await page.waitForFunction(() => document.querySelectorAll("details.submission-entry").length === 25);
  assert.equal(await entries.count(), 25);
  const filteredOlderPage = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?status=new&funnel_id=${funnelId}&cursor=`),
  );
  await page.getByRole("button", { name: "Load older submissions" }).click();
  await filteredOlderPage;
  await page.getByText("All retained submissions are shown.").waitFor({ state: "visible" });
  assert.equal(await entries.count(), 27);

  const resetToFunnel = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?funnel_id=${funnelId}`),
  );
  await statusFilter.selectOption("all");
  await resetToFunnel;
  await page.waitForFunction(() => document.querySelectorAll("details.submission-entry").length === 25);
  assert.equal(await entries.count(), 25);

  const emptyFunnelList = page.waitForResponse((response) =>
    response.request().method() === "GET" &&
    response.url().includes(`/api/submissions?funnel_id=${emptyFunnelId}`),
  );
  await funnelFilter.selectOption(emptyFunnelId);
  await emptyFunnelList;
  await page.getByRole("heading", { name: "No submissions for this funnel" }).waitFor({ state: "visible" });
  assert.equal(await entries.count(), 0);

  const resetAllFunnels = page.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().endsWith("/api/submissions"),
  );
  await funnelFilter.selectOption("all");
  await resetAllFunnels;
  await page.locator("details.submission-entry").first().waitFor({ state: "visible" });
  assert.equal(await entries.count(), 25);

  await page.setViewportSize({ width: 320, height: 740 });
  const width = await page.evaluate(() => ({ scroll: document.documentElement.scrollWidth, viewport: window.innerWidth }));
  assert.ok(width.scroll <= width.viewport, JSON.stringify(width));
  const toolbarDisplay = await page.locator(".submission-toolbar").evaluate((toolbar) =>
    getComputedStyle(toolbar).display,
  );
  assert.equal(toolbarDisplay, "grid");
  const filterLayout = await page.locator(".submission-filter-group").evaluateAll((groups) =>
    groups.map((group) => ({
      top: group.getBoundingClientRect().top,
      groupWidth: group.getBoundingClientRect().width,
      columnCount: getComputedStyle(group).gridTemplateColumns.trim().split(/\s+/).length,
      selectWidth: group.querySelector("select").getBoundingClientRect().width,
      selectRight: group.querySelector("select").getBoundingClientRect().right,
    })),
  );
  assert.equal(filterLayout.length, 2, JSON.stringify(filterLayout));
  assert.ok(filterLayout[1].top > filterLayout[0].top, JSON.stringify(filterLayout));
  assert.ok(filterLayout.every(({ groupWidth, columnCount, selectWidth, selectRight }) =>
    columnCount === 1 && selectWidth >= groupWidth - 1 && selectRight <= width.viewport,
  ), JSON.stringify(filterLayout));

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

  const legacyStatusUpdate = await page.evaluate(async (submissionId) => {
    const response = await fetch(`/api/submissions/${submissionId}/status`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ status: "closed", extra: true }),
    });
    const invalidStatus = { status: response.status, body: await response.json() };
    const accepted = await fetch(`/api/submissions/${submissionId}/status`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ status: "closed" }),
    });
    return {
      invalidStatus,
      acceptedStatus: accepted.status,
      accepted: await accepted.json(),
    };
  }, statusResult.submission_id);
  assert.equal(legacyStatusUpdate.invalidStatus.status, 400);
  assert.equal(legacyStatusUpdate.acceptedStatus, 200);
  assert.equal(legacyStatusUpdate.accepted.status, "closed");
  const correction = await page.evaluate(async ({ submissionId, expectedVersion, correctsVersion }) => {
    const body = {
      expected_version: expectedVersion,
      status: "closed",
      next_action: null,
      due_on: null,
      close_reason: null,
      corrects_version: correctsVersion,
    };
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": "pub011-correction-one" },
      body: JSON.stringify(body),
    });
    return { status: response.status, body: await response.json(), requestBody: body };
  }, {
    submissionId: statusResult.submission_id,
    expectedVersion: statusResult.version + 4,
    correctsVersion: statusResult.version + 4,
  });
  assert.equal(correction.status, 200, JSON.stringify(correction.body));
  assert.equal(correction.body.version, statusResult.version + 5);
  const correctionReplay = await page.evaluate(async ({ submissionId, body }) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": "pub011-correction-one" },
      body: JSON.stringify(body),
    });
    return { status: response.status, body: await response.json() };
  }, { submissionId: statusResult.submission_id, body: correction.requestBody });
  assert.equal(correctionReplay.status, 200, JSON.stringify(correctionReplay.body));
  assert.equal(correctionReplay.body.replayed, true);
  const alreadyCorrected = await page.evaluate(async ({ submissionId, expectedVersion, correctsVersion }) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": "pub011-correction-duplicate" },
      body: JSON.stringify({
        expected_version: expectedVersion,
        status: "closed",
        next_action: "Review correction",
        due_on: null,
        close_reason: null,
        corrects_version: correctsVersion,
      }),
    });
    return { status: response.status, body: await response.json() };
  }, {
    submissionId: statusResult.submission_id,
    expectedVersion: correction.body.version,
    correctsVersion: statusResult.version + 4,
  });
  assert.equal(alreadyCorrected.status, 409);
  assert.equal(alreadyCorrected.body.code, "conflict/submission_follow_up_correction_target");
  const laterOrdinaryEvent = await page.evaluate(async ({ submissionId, expectedVersion }) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": "pub011-after-correction" },
      body: JSON.stringify({
        expected_version: expectedVersion,
        status: "closed",
        next_action: "Review correction",
        due_on: null,
        close_reason: null,
        corrects_version: null,
      }),
    });
    return { status: response.status, body: await response.json() };
  }, { submissionId: statusResult.submission_id, expectedVersion: correction.body.version });
  assert.equal(laterOrdinaryEvent.status, 200, JSON.stringify(laterOrdinaryEvent.body));
  const correctionOfCorrection = await page.evaluate(async ({ submissionId, expectedVersion, targetVersion }) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`, {
      method: "PUT",
      headers: { "Content-Type": "application/json", "Idempotency-Key": "pub011-correction-of-correction" },
      body: JSON.stringify({
        expected_version: expectedVersion,
        status: "closed",
        next_action: "Keep the audit trail",
        due_on: null,
        close_reason: null,
        corrects_version: targetVersion,
      }),
    });
    return { status: response.status, body: await response.json() };
  }, {
    submissionId: statusResult.submission_id,
    expectedVersion: laterOrdinaryEvent.body.version,
    targetVersion: correction.body.version,
  });
  assert.equal(correctionOfCorrection.status, 409);
  assert.equal(correctionOfCorrection.body.code, "conflict/submission_follow_up_correction_target");
  const finalHistory = await page.evaluate(async (submissionId) => {
    const response = await fetch(`/api/submissions/${submissionId}/follow-up`);
    return { status: response.status, body: await response.json() };
  }, statusResult.submission_id);
  assert.equal(finalHistory.status, 200);
  assert.equal(finalHistory.body.events.length, statusResult.version + 6);
  const ownerEvent = finalHistory.body.events[statusResult.version - 1];
  const concurrentEvent = finalHistory.body.events[statusResult.version];
  const conflictEvent = finalHistory.body.events[statusResult.version + 1];
  const recoveredEvent = finalHistory.body.events[statusResult.version + 2];
  const legacyEvent = finalHistory.body.events[statusResult.version + 3];
  const correctionEvent = finalHistory.body.events[statusResult.version + 4];
  const laterOrdinaryEventRecord = finalHistory.body.events[statusResult.version + 5];
  assert.equal(ownerEvent.version, statusResult.version);
  assert.equal(ownerEvent.provenance, "owner_follow_up");
  assert.equal(ownerEvent.status, "qualified");
  assert.equal(concurrentEvent.version, statusResult.version + 1);
  assert.equal(concurrentEvent.supersedes_version, statusResult.version);
  assert.equal(conflictEvent.version, statusResult.version + 2);
  assert.equal(conflictEvent.next_action, "Record the follow-up call");
  assert.equal(recoveredEvent.version, statusResult.version + 3);
  assert.equal(recoveredEvent.next_action, "Keep this unsaved note");
  assert.equal(legacyEvent.version, statusResult.version + 4);
  assert.equal(legacyEvent.provenance, "legacy_status");
  assert.equal(legacyEvent.supersedes_version, statusResult.version + 3);
  assert.equal(legacyEvent.close_reason, null);
  assert.equal(correctionEvent.version, statusResult.version + 5);
  assert.equal(correctionEvent.corrects_version, statusResult.version + 4);
  assert.equal(correctionEvent.provenance, "owner_follow_up");
  assert.equal(laterOrdinaryEventRecord.version, statusResult.version + 6);
  if (process.env.PR050_SCREENSHOT) {
    await page.screenshot({ path: process.env.PR050_SCREENSHOT, fullPage: true });
  }

  const deletePage = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const deletePageErrors = [];
  deletePage.on("pageerror", (error) => deletePageErrors.push(error.message));
  await deletePage.goto(pageUrl, { waitUntil: "networkidle" });
  const deleteEntry = deletePage.locator("details.submission-entry").filter({ hasText: "Grace Hopper" });
  assert.equal(await deleteEntry.count(), 1);
  const deleteTargetHref = await deleteEntry.locator("a[href*='/export']").getAttribute("href");
  assert.ok(deleteTargetHref);
  await deleteEntry.locator("summary").first().click();
  await deleteEntry.locator("details.submission-delete-confirmation > summary").click();

  let markDeleteStarted;
  const deleteStarted = new Promise((resolve) => { markDeleteStarted = resolve; });
  let releaseDelete;
  const deletePaused = new Promise((resolve) => { releaseDelete = resolve; });
  await deletePage.route("**/api/submissions/*", async (route) => {
    if (route.request().method() === "DELETE") {
      markDeleteStarted();
      await deletePaused;
      await route.fulfill({ status: 204, body: "" });
    } else {
      await route.continue();
    }
  });
  const delayedDeleteResponse = deletePage.waitForResponse(
    (response) => response.request().method() === "DELETE",
  );
  await deletePage.getByRole("button", { name: "Delete permanently" }).click();
  await deleteStarted;
  await deletePage.getByText("Finishing the deletion…", { exact: false }).waitFor({ state: "visible" });
  assert.equal(await deletePage.locator(`a[href="${deleteTargetHref}"]`).count(), 0);
  const filteredDuringDelete = deletePage.waitForResponse(
    (response) => response.request().method() === "GET" && response.url().includes(`/api/submissions?funnel_id=${funnelId}`),
  );
  await deletePage.getByRole("combobox", { name: "Filter submissions by funnel" }).selectOption(funnelId);
  assert.equal((await filteredDuringDelete).status(), 200);
  assert.equal(await deletePage.locator(`a[href="${deleteTargetHref}"]`).count(), 0);
  await deletePage.getByText("Finishing the deletion…", { exact: false }).waitFor({ state: "visible" });
  const remainingDeleteEntry = deletePage.locator("details.submission-entry").first();
  await remainingDeleteEntry.locator("summary").first().click();
  await remainingDeleteEntry.locator("details.submission-delete-confirmation > summary").click();
  assert.equal(
    await remainingDeleteEntry.getByRole("button", { name: "Delete permanently" }).isDisabled(),
    true,
  );
  const refreshAfterDelete = deletePage.waitForResponse(
    (response) => response.request().method() === "GET" && response.url().includes(`/api/submissions?funnel_id=${funnelId}`),
  );
  releaseDelete();
  assert.equal((await delayedDeleteResponse).status(), 204);
  assert.equal((await refreshAfterDelete).status(), 200);
  await deletePage.getByText("Finishing the deletion…", { exact: false }).waitFor({ state: "detached" });
  assert.deepEqual(deletePageErrors, []);
  await deletePage.close();

  const deleteFailurePage = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  await deleteFailurePage.goto(pageUrl, { waitUntil: "networkidle" });
  const failedDeleteEntry = deleteFailurePage.locator("details.submission-entry").filter({ hasText: "Grace Hopper" });
  assert.equal(await failedDeleteEntry.count(), 1);
  const failedDeleteTargetHref = await failedDeleteEntry.locator("a[href*='/export']").getAttribute("href");
  assert.ok(failedDeleteTargetHref);
  await failedDeleteEntry.locator("summary").first().click();
  await failedDeleteEntry.locator("details.submission-delete-confirmation > summary").click();
  let markFailedDeleteStarted;
  const failedDeleteStarted = new Promise((resolve) => { markFailedDeleteStarted = resolve; });
  let releaseFailedDelete;
  const failedDeletePaused = new Promise((resolve) => { releaseFailedDelete = resolve; });
  let markFailedListStarted;
  const failedListStarted = new Promise((resolve) => { markFailedListStarted = resolve; });
  let releaseFailedList;
  const failedListPaused = new Promise((resolve) => { releaseFailedList = resolve; });
  await deleteFailurePage.route("**/api/submissions**", async (route) => {
    if (route.request().method() === "DELETE") {
      markFailedDeleteStarted();
      await failedDeletePaused;
      await route.fulfill({ status: 503, contentType: "application/json", body: "{}" });
    } else if (route.request().method() === "GET" && route.request().url().includes(`/api/submissions?funnel_id=${funnelId}`)) {
      markFailedListStarted();
      await failedListPaused;
      await route.continue();
    } else {
      await route.continue();
    }
  });
  const failedDeleteResponse = deleteFailurePage.waitForResponse(
    (response) => response.request().method() === "DELETE",
  );
  await deleteFailurePage.getByRole("button", { name: "Delete permanently" }).click();
  await failedDeleteStarted;
  await deleteFailurePage.getByText("Finishing the deletion…", { exact: false }).waitFor({ state: "visible" });
  assert.equal(await deleteFailurePage.locator(`a[href="${failedDeleteTargetHref}"]`).count(), 0);
  const delayedFailedListResponse = deleteFailurePage.waitForResponse(
    (response) => response.request().method() === "GET" && response.url().includes(`/api/submissions?funnel_id=${funnelId}`),
  );
  await deleteFailurePage.getByRole("combobox", { name: "Filter submissions by funnel" }).selectOption(funnelId);
  await failedListStarted;
  releaseFailedDelete();
  assert.equal((await failedDeleteResponse).status(), 503);
  await deleteFailurePage.getByRole("alert").filter({ hasText: "server could not confirm the deletion" }).waitFor({ state: "visible" });
  await deleteFailurePage.getByText("Finishing the deletion…", { exact: false }).waitFor({ state: "detached" });
  releaseFailedList();
  assert.equal((await delayedFailedListResponse).status(), 200);
  assert.equal(await deleteFailurePage.locator(`a[href="${failedDeleteTargetHref}"]`).count(), 0);
  await deleteFailurePage.getByRole("alert").filter({ hasText: "server could not confirm the deletion" }).waitFor({ state: "visible" });
  const reloadedAfterFailure = deleteFailurePage.waitForResponse(
    (response) => response.request().method() === "GET" && response.url().includes(`/api/submissions?funnel_id=${funnelId}`),
  );
  await deleteFailurePage.getByRole("button", { name: "Reload inbox" }).click();
  assert.equal((await reloadedAfterFailure).status(), 200);
  await deleteFailurePage.locator("details.submission-entry").first().waitFor({ state: "visible" });
  assert.equal(await deleteFailurePage.locator(`a[href="${failedDeleteTargetHref}"]`).count(), 1);
  assert.equal(await deleteFailurePage.getByRole("alert").filter({ hasText: "server could not confirm the deletion" }).count(), 0);
  await deleteFailurePage.close();

  const navigationDeletePage = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const navigationDeleteErrors = [];
  navigationDeletePage.on("pageerror", (error) => navigationDeleteErrors.push(error.message));
  await navigationDeletePage.goto(pageUrl, { waitUntil: "networkidle" });
  const navigationEntry = navigationDeletePage.locator("details.submission-entry").first();
  await navigationEntry.locator("summary").first().click();
  await navigationEntry.locator("details.submission-delete-confirmation > summary").click();
  let markNavigationDeleteStarted;
  const navigationDeleteStarted = new Promise((resolve) => { markNavigationDeleteStarted = resolve; });
  let releaseNavigationDelete;
  const navigationDeletePaused = new Promise((resolve) => { releaseNavigationDelete = resolve; });
  await navigationDeletePage.route("**/api/submissions/*", async (route) => {
    if (route.request().method() === "DELETE") {
      markNavigationDeleteStarted();
      await navigationDeletePaused;
      await route.fulfill({ status: 204, body: "" });
    } else {
      await route.continue();
    }
  });
  const delayedNavigationDeleteResponse = navigationDeletePage.waitForResponse(
    (response) => response.request().method() === "DELETE",
  );
  await navigationDeletePage.getByRole("button", { name: "Delete permanently" }).click();
  await navigationDeleteStarted;
  await navigationDeletePage.getByRole("link", { name: "Funnels", exact: true }).click();
  await navigationDeletePage.getByRole("heading", { name: "Funnels", level: 1 }).waitFor({ state: "visible" });
  releaseNavigationDelete();
  assert.equal((await delayedNavigationDeleteResponse).status(), 204);
  assert.deepEqual(navigationDeleteErrors, []);
  await navigationDeletePage.close();

  const failurePage = await browser.newPage({ viewport: { width: 390, height: 844 } });
  let failFunnelList = true;
  await failurePage.route("**/api/funnels", async (route) => {
    if (failFunnelList) {
      await route.fulfill({ status: 503, contentType: "application/json", body: "{}" });
    } else {
      await route.continue();
    }
  });
  await failurePage.goto(pageUrl, { waitUntil: "networkidle" });
  await failurePage.getByText("Funnel filters could not be loaded.", { exact: false }).waitFor({ state: "visible" });
  assert.equal(await failurePage.getByRole("combobox", { name: "Filter submissions by funnel" }).isDisabled(), true);
  failFunnelList = false;
  const retriedFunnels = failurePage.waitForResponse((response) =>
    response.request().method() === "GET" && response.url().endsWith("/api/funnels"),
  );
  await failurePage.getByRole("button", { name: "Retry funnel filters" }).click();
  const retryResponse = await retriedFunnels;
  assert.equal(retryResponse.status(), 200);
  await failurePage.waitForFunction(() => !document.getElementById("submission-funnel-filter")?.disabled);
  await failurePage.close();
  console.log("PR-050 submissions browser triage, filtering, pagination, consent, responsive layout, and accessibility passed");
} finally {
  await browser.close();
}
