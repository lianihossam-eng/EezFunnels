import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { access, rm } from "node:fs/promises";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const root = resolve(import.meta.dirname, "..");
const helperUrl = new URL("./pr019-local-worker.mjs", import.meta.url).href;
const eventPrefix = "PR019_SIGNAL_EVENT ";

function launchHelperChild(phase) {
  const source = `
    import { startPr019Worker } from ${JSON.stringify(helperUrl)};
    const phase = ${JSON.stringify(phase)};
    const emit = (event) => new Promise((resolve) => process.stdout.write(${JSON.stringify(eventPrefix)} + JSON.stringify(event) + "\\n", resolve));
    let spawnDetails;
    const onWorkerSpawn = async (details) => {
      spawnDetails = details;
      await emit({ phase, event: "spawned", ...details });
      if (phase === "readiness") await new Promise((resolve) => process.once("message", resolve));
    };
    const onSignalCleanup = async (signal) => {
      const response = await fetch(spawnDetails.origin + "/health", { signal: AbortSignal.timeout(1000) });
      await emit({ phase, event: "signal-cleanup", signal, status: response.status });
    };
    try {
      const worker = await startPr019Worker("signal-regression-" + phase, { onWorkerSpawn, onSignalCleanup });
      await emit({ phase, event: "ready", origin: worker.origin });
      await new Promise(() => {});
    } catch (error) {
      console.error(error.stack ?? error);
      process.exitCode = 1;
    }
  `;
  const child = spawn(process.execPath, ["--input-type=module", "-e", source], {
    cwd: root,
    stdio: ["ignore", "pipe", "pipe", "ipc"],
  });
  const state = {
    child,
    events: [],
    output: "",
    stderr: "",
    closed: null,
  };
  let pending = "";
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    state.output += chunk;
    pending += chunk;
    const lines = pending.split("\n");
    pending = lines.pop() ?? "";
    for (const line of lines) {
      if (!line.startsWith(eventPrefix)) continue;
      state.events.push(JSON.parse(line.slice(eventPrefix.length)));
    }
  });
  child.stderr.on("data", (chunk) => (state.stderr += chunk));
  state.closePromise = new Promise((resolveClose) => {
    child.once("close", (code, signal) => {
      state.closed = { code, signal };
      resolveClose(state.closed);
    });
  });
  return state;
}

async function waitForEvent(state, eventName, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const event = state.events.find((candidate) => candidate.event === eventName);
    if (event) return event;
    if (state.closed) {
      throw new Error(
        `Helper child exited before ${eventName}: ${JSON.stringify(state.closed)}\n${state.stderr}\n${state.output}`,
      );
    }
    await delay(25);
  }
  throw new Error(`Timed out waiting for ${eventName}\n${state.stderr}\n${state.output}`);
}

async function waitForHealth(origin) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${origin}/health`, { signal: AbortSignal.timeout(500) });
      if (response.ok) {
        const body = await response.json();
        if (body.status === "ok") return;
      }
    } catch {}
    await delay(50);
  }
  throw new Error(`Worker never became healthy at ${origin}`);
}

async function waitForPortClosed(origin) {
  const deadline = Date.now() + 10000;
  let lastResponse;
  while (Date.now() < deadline) {
    try {
      lastResponse = await fetch(`${origin}/health`, { signal: AbortSignal.timeout(500) });
    } catch {
      return;
    }
    await delay(50);
  }
  throw new Error(`Worker port remained open after shutdown (last status ${lastResponse?.status})`);
}

async function pathExists(path) {
  try {
    await access(path);
    return true;
  } catch (error) {
    if (error.code === "ENOENT") return false;
    throw error;
  }
}

async function assertCleaned(details) {
  assert.equal(await pathExists(details.stateDir), false, `D1 state remained at ${details.stateDir}`);
  assert.equal(await pathExists(details.configPath), false, `Temporary config remained at ${details.configPath}`);
}

async function forceCleanup(state) {
  const details = state.events.find((event) => event.event === "spawned");
  if (!state.closed) {
    if (details?.workerPid) {
      try {
        process.kill(-details.workerPid, "SIGKILL");
      } catch (error) {
        if (error.code !== "ESRCH") throw error;
      }
    }
    state.child.kill("SIGKILL");
    await waitForClose(state, 10000).catch(() => {});
  }
  if (details) {
    await rm(details.stateDir, { recursive: true, force: true });
    await rm(details.configPath, { force: true });
    await waitForPortClosed(details.origin);
  }
}

function waitForClose(state, timeoutMs) {
  if (state.closed) return Promise.resolve(state.closed);
  return new Promise((resolveClose, reject) => {
    const timer = setTimeout(() => reject(new Error("Helper child did not close")), timeoutMs);
    timer.unref();
    state.closePromise.then((closed) => {
      clearTimeout(timer);
      resolveClose(closed);
    });
  });
}

async function verifySignalPhase(phase) {
  const state = launchHelperChild(phase);
  let details;
  try {
    details = await waitForEvent(state, "spawned");
    if (phase === "readiness") {
      await waitForHealth(details.origin);
      assert.equal(
        state.events.some((event) => event.event === "ready"),
        false,
        "startup signal case must interrupt the helper before it returns",
      );
    } else {
      await waitForEvent(state, "ready");
      await waitForHealth(details.origin);
    }

    state.child.kill("SIGTERM");
    const closed = await waitForClose(state, 20000).catch((error) => {
      throw new Error(`Helper child did not exit after SIGTERM during ${phase}`, { cause: error });
    });
    assert.deepEqual(closed, { code: 143, signal: null }, `SIGTERM exit status during ${phase}`);
    const cleanupEvent = state.events.find((event) => event.event === "signal-cleanup");
    assert.deepEqual(
      cleanupEvent && { signal: cleanupEvent.signal, status: cleanupEvent.status },
      { signal: "SIGTERM", status: 200 },
      `pre-shutdown cleanup callback during ${phase}`,
    );
    await waitForPortClosed(details.origin);
    await assertCleaned(details);
    console.log(`PR-019 signal cleanup ${phase}: child exited, Worker port closed, D1/config removed`);
  } finally {
    await forceCleanup(state);
  }
}

await verifySignalPhase("readiness");
await verifySignalPhase("running");
