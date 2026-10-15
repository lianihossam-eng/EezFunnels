import assert from "node:assert/strict";
import { finalizePr019Browser } from "./pr019-browser-finalizer.mjs";

const events = [];
const closeError = new Error("browser close failed synchronously");

await assert.rejects(
  finalizePr019Browser(
    () => {
      events.push("browser-close");
      throw closeError;
    },
    {
      async stop() {
        events.push("worker-stop");
      },
    },
  ),
  (error) => error === closeError,
);
assert.deepEqual(events, ["browser-close", "worker-stop"]);
console.log("PR-019 browser finalizer: Worker stop runs after synchronous browser close failure");

const workerError = new Error("Worker stop failed");
const combinedEvents = [];
await assert.rejects(
  finalizePr019Browser(
    () => {
      combinedEvents.push("browser-close");
      throw closeError;
    },
    {
      async stop() {
        combinedEvents.push("worker-stop");
        throw workerError;
      },
    },
  ),
  (error) =>
    error instanceof AggregateError &&
    error.errors[0] === closeError &&
    error.errors[1] === workerError,
);
assert.deepEqual(combinedEvents, ["browser-close", "worker-stop"]);
