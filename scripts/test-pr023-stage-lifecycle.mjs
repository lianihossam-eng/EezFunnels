import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import vm from "node:vm";

const html = readFileSync(resolve("plan/verification/PR-023/prototypes/stage-lifecycle.html"), "utf8");
const script = html.split("<script>", 2)[1]?.split("</script>", 1)[0];
assert(script, "stage lifecycle prototype script is missing");
const pureReducer = script.slice(script.indexOf("const scenarios = ["), script.indexOf("let state = initialState();"));
assert(pureReducer.length > 0, "stage lifecycle reducer is missing");
const context = vm.createContext({ structuredClone: globalThis.structuredClone });
vm.runInContext(`${pureReducer}\nglobalThis.testApi = { scenarios, initialState, reduce };`, context);
const { scenarios, initialState, reduce } = context.testApi;

function run(actions) {
  let state = initialState();
  for (const type of actions) state = reduce(state, { type });
  return state;
}

assert(scenarios.some((scenario) => scenario.id === "interrupted-upload"));

const expiredMint = run(["MINT", "ADVANCE_16", "CLEANUP", "REPLAY_MINT", "BEGIN_UPLOAD"]);
assert.equal(expiredMint.stage.status, "expired");
assert.equal(expiredMint.stage.metadataRemoved, true);
assert.match(expiredMint.lastEvent, /Upload denied/);

const consumedMint = run([
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "SAVE", "SAVE", "CLEANUP", "REPLAY_MINT", "REPLAY_SAVE",
]);
assert.equal(consumedMint.revisionCount, 8);
assert.equal(consumedMint.saveReceipts.at(-1).revisionId, "rev-8");
assert.equal(consumedMint.saveReceipts.length, 1);
assert.match(consumedMint.lastEvent, /Receipt-first replay/);

const consecutiveSaves = run([
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "SAVE",
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "SAVE", "REPLAY_PREVIOUS_SAVE",
]);
assert.equal(consecutiveSaves.revisionCount, 9);
assert.equal(consecutiveSaves.editorHead, "rev-9");
assert.equal(consecutiveSaves.stage.status, "consumed");
assert.equal(consecutiveSaves.saveReceipts.length, 2);
assert.notEqual(consecutiveSaves.saveReceipts[0].key, consecutiveSaves.saveReceipts[1].key);
assert.equal(consecutiveSaves.saveReceipts[0].revisionId, "rev-8");
assert.equal(consecutiveSaves.saveReceipts[1].revisionId, "rev-9");
assert.match(consecutiveSaves.lastEvent, /Receipt-first replay returned rev-8/);

const changedSavePayload = run([
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "SAVE",
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "SAVE_CHANGED",
]);
assert.equal(changedSavePayload.revisionCount, 8);
assert.equal(changedSavePayload.stage.status, "uploaded");
assert.equal(changedSavePayload.saveReceipts.length, 1);
assert.match(changedSavePayload.lastEvent, /Idempotency mismatch/);

const changedSaveWithoutPriorReceipt = run([
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "SAVE_CHANGED",
]);
assert.equal(changedSaveWithoutPriorReceipt.revisionCount, 7);
assert.equal(changedSaveWithoutPriorReceipt.stage.status, "uploaded");
assert.equal(changedSaveWithoutPriorReceipt.saveReceipts.length, 0);
assert.match(changedSaveWithoutPriorReceipt.lastEvent, /no active prior save key/);

const expiredSaveReceipt = run([
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "SAVE", "ADVANCE_1440", "REPLAY_SAVE",
]);
assert.match(expiredSaveReceipt.lastEvent, /no active matching receipt/);
const saveAfterReceiptExpiry = run([
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "SAVE", "ADVANCE_1440",
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "SAVE",
]);
assert.equal(saveAfterReceiptExpiry.revisionCount, 9);
assert.equal(saveAfterReceiptExpiry.saveReceipts.length, 1);

const staleHead = run(["MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "CHANGE_HEAD", "SAVE"]);
assert.equal(staleHead.revisionCount, 8);
assert.equal(staleHead.editorHead, "rev-8");
assert.equal(staleHead.saveReceipts.length, 0);
assert.match(staleHead.lastEvent, /no success receipt or revision/);

const staleHeadRecovery = run(["MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "CHANGE_HEAD", "SAVE", "MINT"]);
assert.equal(staleHeadRecovery.stage.id, "stage-demo-2");
assert.equal(staleHeadRecovery.stage.baseHead, "rev-8");
assert.equal(staleHeadRecovery.saveReceipts.length, 0);
assert.equal(staleHeadRecovery.retiredStages[0].stage.id, "stage-demo-1");
assert.equal(staleHeadRecovery.retiredStages[0].stage.status, "discarded");
assert.equal(staleHeadRecovery.retiredStages[0].stage.objectPresent, true);
assert.equal(staleHeadRecovery.mintReceipts.length, 2);
assert.notEqual(staleHeadRecovery.mintReceipts[0].key, staleHeadRecovery.mintReceipts[1].key);

const staleMintReplay = run([
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "CHANGE_HEAD", "SAVE", "MINT", "REPLAY_PREVIOUS_MINT",
]);
assert.match(staleMintReplay.lastEvent, /original stage stage-demo-1/);

const staleHeadCleanup = run([
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "CHANGE_HEAD", "SAVE", "MINT", "CLEANUP",
]);
assert.equal(staleHeadCleanup.retiredStages[0].stage.metadataRemoved, true);
assert.equal(staleHeadCleanup.retiredStages[0].stage.objectPresent, false);
assert.equal(staleHeadCleanup.retiredStages[0].attempt.objectPresent, false);

const staleCleanupAfterReplacementSave = run([
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "CHANGE_HEAD", "SAVE", "MINT",
  "BEGIN_UPLOAD", "FINISH_UPLOAD", "SAVE", "CLEANUP", "CLEANUP",
]);
assert.equal(staleCleanupAfterReplacementSave.stage.metadataRemoved, true);
assert.equal(staleCleanupAfterReplacementSave.retiredStages[0].stage.metadataRemoved, true);
assert.equal(staleCleanupAfterReplacementSave.retiredStages[0].stage.objectPresent, false);

const staleCleanupAfterReplacementExpiry = run([
  "MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "CHANGE_HEAD", "SAVE", "MINT",
  "ADVANCE_16", "CLEANUP", "CLEANUP",
]);
assert.equal(staleCleanupAfterReplacementExpiry.stage.metadataRemoved, true);
assert.equal(staleCleanupAfterReplacementExpiry.retiredStages[0].stage.metadataRemoved, true);
assert.equal(staleCleanupAfterReplacementExpiry.retiredStages[0].stage.objectPresent, false);

const expiredStageRecovery = run(["MINT", "ADVANCE_16", "MINT"]);
assert.equal(expiredStageRecovery.stage.id, "stage-demo-2");
assert.equal(expiredStageRecovery.stage.baseHead, "rev-7");
assert.equal(expiredStageRecovery.retiredStages[0].stage.status, "expired");
assert.match(run(["MINT", "ADVANCE_16", "MINT", "REPLAY_PREVIOUS_MINT"]).lastEvent, /original stage stage-demo-1/);

const expiredUploadedStage = run(["MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "ADVANCE_16", "MINT", "CLEANUP"]);
assert.equal(expiredUploadedStage.retiredStages[0].stage.metadataRemoved, true);
assert.equal(expiredUploadedStage.retiredStages[0].stage.objectPresent, false);

const lateRetiredUpload = run([
  "MINT", "BEGIN_UPLOAD", "ADVANCE_16", "MINT", "FINISH_RETIRED_UPLOAD", "CLEANUP", "CLEANUP",
]);
assert.equal(lateRetiredUpload.retiredStages[0].attempt.status, "abandoned");
assert.equal(lateRetiredUpload.retiredStages[0].attempt.objectPresent, false);
assert.equal(lateRetiredUpload.retiredStages[0].stage.metadataRemoved, true);

const retriedLateWriter = run([
  "MINT", "BEGIN_UPLOAD", "ADVANCE_5", "CLEANUP", "BEGIN_UPLOAD", "ADVANCE_16", "MINT",
  "FINISH_RETIRED_UPLOAD", "CLEANUP", "FINISH_RETIRED_UPLOAD", "CLEANUP",
]);
assert.equal(retriedLateWriter.retiredAttempts[0].id, "attempt-1");
assert.equal(retriedLateWriter.retiredAttempts[0].status, "abandoned");
assert.equal(retriedLateWriter.retiredAttempts[0].writerQuiescent, true);
assert.equal(retriedLateWriter.retiredAttempts[0].cleanupConfirmed, true);
assert.equal(retriedLateWriter.retiredAttempts[0].objectPresent, false);
assert.equal(retriedLateWriter.retiredStages[0].attempt.id, "attempt-2");
assert.equal(retriedLateWriter.retiredStages[0].attempt.writerQuiescent, true);
assert.equal(retriedLateWriter.retiredStages[0].attempt.cleanupConfirmed, true);
assert.equal(retriedLateWriter.retiredStages[0].attempt.objectPresent, false);
assert.ok(retriedLateWriter.events.some((event) => /Late R2 write for attempt-1/.test(event)));
assert.ok(retriedLateWriter.events.some((event) => /Late R2 write for attempt-2/.test(event)));

const writerQuiescence = run([
  "MINT", "BEGIN_UPLOAD", "ADVANCE_5", "CLEANUP", "CLEANUP",
]);
assert.equal(writerQuiescence.attempt.status, "abandoned");
assert.equal(writerQuiescence.attempt.writerQuiescent, false);
assert.equal(writerQuiescence.attempt.cleanupConfirmed, false);
assert.match(writerQuiescence.lastEvent, /not confirmed quiescent/);
const writerQuiescenceConfirmed = run([
  "MINT", "BEGIN_UPLOAD", "ADVANCE_5", "CLEANUP", "CLEANUP", "CONFIRM_WRITER_QUIESCENT", "CLEANUP",
]);
assert.equal(writerQuiescenceConfirmed.attempt.writerQuiescent, true);
assert.equal(writerQuiescenceConfirmed.attempt.cleanupConfirmed, true);
assert.match(writerQuiescenceConfirmed.lastEvent, /post-quiescence absence\/delete check/);

const interrupted = run([
  "MINT",
  "BEGIN_UPLOAD",
  "INTERRUPT_UPLOAD",
  "ADVANCE_5",
  "CLEANUP",
  "FINISH_UPLOAD",
  "SAVE",
  "CLEANUP",
]);
assert.equal(interrupted.stage.status, "issued");
assert.equal(interrupted.stage.objectOrphaned, true);
assert.equal(interrupted.attempt.status, "abandoned");
assert.equal(interrupted.attempt.objectPresent, false);
assert.equal(interrupted.attempt.writerQuiescent, true);
assert.equal(interrupted.attempt.cleanupConfirmed, true);
assert.equal(interrupted.revisionCount, 7);
assert.equal(interrupted.saveReceipts.length, 0);

const uploadRetry = run(["MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "RETRY_SAME_UPLOAD", "UPLOAD_DIFFERENT"]);
assert.equal(uploadRetry.stage.status, "uploaded");
assert.equal(uploadRetry.stage.objectPresent, true);
assert.equal(uploadRetry.revisionCount, 7);

process.stdout.write("PR-023 stage lifecycle assertions passed: mint/save receipt replay and expiry, distinct consecutive save keys, changed-payload rejection, stale-head recovery, replacement cleanup, late upload tombstones, retry-before-remint cleanup, interrupted upload cleanup, and retry identity.\n");
