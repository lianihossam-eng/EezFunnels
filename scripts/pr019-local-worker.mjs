import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { access, mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { setTimeout as delay } from "node:timers/promises";
import { randomUUID } from "node:crypto";

const root = resolve(import.meta.dirname, "..");
const wrangler = join(root, "node_modules/.bin/wrangler");
const requiredBuildFiles = [
  "apps/dashboard/dist/assets/eezfunnel.js",
  "apps/dashboard/dist/assets/eezfunnel_bg.wasm",
  "apps/api/build/index.js",
  "apps/api/build/worker/shim.mjs",
];

async function unusedPort() {
  const server = createServer();
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  assert(address && typeof address === "object");
  await new Promise((resolveClose, reject) => {
    server.close((error) => (error ? reject(error) : resolveClose()));
  });
  return address.port;
}

function groupHasLiveMembers(groupId) {
  const result = spawnSync("ps", ["-eo", "pgid=,stat="], {
    encoding: "utf8",
    timeout: 5000,
  });
  assert.equal(result.status, 0, `ps failed while checking process group ${groupId}`);
  return result.stdout.split("\n").some((line) => {
    const fields = line.trim().split(/\s+/);
    return fields.length === 2 && fields[0] === String(groupId) && !fields[1].startsWith("Z");
  });
}

async function waitForGroup(groupId, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (groupHasLiveMembers(groupId) && Date.now() < deadline) await delay(100);
}

export async function startPr019Worker(label) {
  const stateDir = await mkdtemp(join(tmpdir(), `eezfunnel-pr019-${label}-`));
  const localConfigPath = join(root, `.wrangler-pr019-${process.pid}-${randomUUID()}.toml`);
  const port = await unusedPort();
  const origin = `http://127.0.0.1:${port}`;
  const env = {
    ...process.env,
    XDG_CONFIG_HOME: join(stateDir, "xdg-config"),
    XDG_CACHE_HOME: join(stateDir, "xdg-cache"),
    NPM_CONFIG_CACHE: join(stateDir, "npm-cache"),
    WRANGLER_SEND_METRICS: "false",
    NO_COLOR: "1",
  };
  for (const name of Object.keys(env)) {
    if (name.startsWith("CLOUDFLARE_")) delete env[name];
  }
  let worker;
  let workerExited = false;
  let workerSpawnError;
  let workerLog = "";
  let workerExit = Promise.resolve({ code: null, signal: null });
  let stopped = false;

  async function stop() {
    if (stopped) return;
    stopped = true;
    let cleanupFailure;
    if (worker?.pid) {
      for (const [signal, timeoutMs] of [
        ["SIGINT", 5000],
        ["SIGTERM", 5000],
        ["SIGKILL", 1000],
      ]) {
        if (!groupHasLiveMembers(worker.pid)) break;
        try {
          process.kill(-worker.pid, signal);
        } catch (error) {
          if (error.code !== "ESRCH") cleanupFailure = error;
        }
        await waitForGroup(worker.pid, timeoutMs);
      }
      if (groupHasLiveMembers(worker.pid)) {
        cleanupFailure ??= new Error(`Wrangler process group ${worker.pid} stayed alive`);
      }
      if (!workerExited) await Promise.race([workerExit, delay(1000)]);
    }
    await rm(stateDir, { recursive: true, force: true });
    await rm(localConfigPath, { force: true });
    if (cleanupFailure) throw cleanupFailure;
  }

  try {
    const missingBuildFiles = [];
    for (const file of requiredBuildFiles) {
      try {
        await access(join(root, file));
      } catch {
        missingBuildFiles.push(file);
      }
    }
    assert.deepEqual(
      missingBuildFiles,
      [],
      `Build the local Worker and dashboard first; missing: ${missingBuildFiles.join(", ")}`,
    );

    const rootConfig = await readFile(join(root, "wrangler.toml"), "utf8");
    const localConfig = rootConfig
      .replace(/^\[build\]\r?\ncommand = .+\r?\n/m, "")
      .replace('main = "apps/api/build/worker/shim.mjs"', 'main = "apps/api/build/index.js"');
    assert.notEqual(localConfig, rootConfig, "Expected a local Worker config without a build command");
    await writeFile(localConfigPath, localConfig, "utf8");
    await mkdir(env.XDG_CONFIG_HOME, { recursive: true });
    await mkdir(env.XDG_CACHE_HOME, { recursive: true });
    await mkdir(env.NPM_CONFIG_CACHE, { recursive: true });

    const migration = spawnSync(
      wrangler,
      [
        "d1",
        "migrations",
        "apply",
        "DB",
        "--local",
        "--env",
        "dev",
        "--persist-to",
        stateDir,
        "--config",
        localConfigPath,
      ],
      { cwd: root, encoding: "utf8", env, timeout: 120000, maxBuffer: 8 * 1024 * 1024 },
    );
    assert.equal(
      migration.status,
      0,
      `Local D1 migrations failed:\n${migration.stdout}\n${migration.stderr}`,
    );

    worker = spawn(
      wrangler,
      [
        "dev",
        "--local",
        "--no-bundle",
        "--env",
        "dev",
        "--config",
        localConfigPath,
        "--port",
        String(port),
        "--ip",
        "127.0.0.1",
        "--var",
        `ALLOWED_HOSTS:127.0.0.1:${port}`,
        "--persist-to",
        stateDir,
        "--show-interactive-dev-session=false",
      ],
      { cwd: root, detached: true, env, stdio: ["ignore", "pipe", "pipe"] },
    );
    worker.stdout.on("data", (chunk) => (workerLog += chunk.toString()));
    worker.stderr.on("data", (chunk) => (workerLog += chunk.toString()));
    worker.on("error", (error) => (workerSpawnError = error));
    workerExit = new Promise((resolveExit) =>
      worker.once("exit", (code, signal) => {
        workerExited = true;
        resolveExit({ code, signal });
      }),
    );

    const deadline = Date.now() + 120000;
    while (Date.now() < deadline) {
      if (workerSpawnError) throw workerSpawnError;
      if (workerExited) {
        const exit = await workerExit;
        throw new Error(`Wrangler exited early (${JSON.stringify(exit)}):\n${workerLog}`);
      }
      try {
        const response = await fetch(`${origin}/health`, { signal: AbortSignal.timeout(1000) });
        if (response.ok) {
          const body = await response.json();
          assert.equal(body.status, "ok");
          return { origin, stateDir, port, stop, workerLog: () => workerLog };
        }
      } catch {}
      await delay(250);
    }
    throw new Error(`Wrangler did not become ready:\n${workerLog}`);
  } catch (error) {
    await stop().catch(() => {});
    throw error;
  }
}
