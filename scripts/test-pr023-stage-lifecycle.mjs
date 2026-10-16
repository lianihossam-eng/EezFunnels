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

const consumedMint = run(["MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "SAVE", "CLEANUP", "REPLAY_MINT", "REPLAY_SAVE"]);
assert.equal(consumedMint.revisionCount, 8);
assert.equal(consumedMint.saveReceipt.revisionId, "rev-8");
assert.match(consumedMint.lastEvent, /Receipt-first replay/);

const staleHead = run(["MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "CHANGE_HEAD", "SAVE"]);
assert.equal(staleHead.revisionCount, 8);
assert.equal(staleHead.editorHead, "rev-8");
assert.equal(staleHead.saveReceipt, null);
assert.match(staleHead.lastEvent, /no success receipt or revision/);

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
assert.equal(interrupted.revisionCount, 7);
assert.equal(interrupted.saveReceipt, null);

const uploadRetry = run(["MINT", "BEGIN_UPLOAD", "FINISH_UPLOAD", "RETRY_SAME_UPLOAD", "UPLOAD_DIFFERENT"]);
assert.equal(uploadRetry.stage.status, "uploaded");
assert.equal(uploadRetry.stage.objectPresent, true);
assert.equal(uploadRetry.revisionCount, 7);

process.stdout.write("PR-023 stage lifecycle assertions passed: expiry, receipt replay, stale head, interrupted upload cleanup, and retry identity.\n");
