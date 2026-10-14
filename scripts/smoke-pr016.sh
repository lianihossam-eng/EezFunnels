#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
if [ -f .project-tools/env.sh ]; then
  source .project-tools/env.sh
fi
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-/tmp/eezfunnel-xdg}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-/tmp/eezfunnel-cache}"
export NPM_CONFIG_CACHE="${NPM_CONFIG_CACHE:-/tmp/eezfunnel-npm-cache}"
export WRANGLER_SEND_METRICS=false
cargo build --locked -p eez-cli

python3 - "$repo_root" <<'PY'
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

root = Path(sys.argv[1])
target = Path(os.environ.get("CARGO_TARGET_DIR", root / "target"))
if not target.is_absolute():
    target = root / target
binary = target / "debug" / "eez"
wrangler = root / "node_modules" / ".bin" / "wrangler"
port = 18787
endpoint = f"http://localhost:{port}"
with socket.socket() as probe:
    if probe.connect_ex(("127.0.0.1", port)) == 0:
        raise SystemExit(f"Port {port} is occupied")
with tempfile.TemporaryDirectory(prefix="eezfunnel-pr016-") as directory:
    isolated = Path(directory)
    storage = isolated / "d1"
    config = isolated / "config"
    cwd = isolated / "caller"
    cwd.mkdir()
    env = os.environ | {"EEZ_CONFIG_DIR": str(config)}
    subprocess.run([str(wrangler), "d1", "migrations", "apply", "DB", "--local", "--env", "dev", "--persist-to", str(storage)], cwd=root, check=True, timeout=120)
    log_path = Path("/tmp/eezfunnel-pr016-wrangler.log")
    with log_path.open("w") as log:
        worker = subprocess.Popen([str(wrangler), "dev", "--local", "--env", "dev", "--port", str(port), "--ip", "127.0.0.1", "--var", f"ALLOWED_HOSTS:localhost:{port}", "--persist-to", str(storage)], cwd=root, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            deadline = time.monotonic() + 180
            while True:
                try:
                    with urllib.request.urlopen(endpoint + "/health", timeout=1) as response:
                        if response.status == 200:
                            break
                except (OSError, urllib.error.URLError):
                    pass
                if worker.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError(f"Local Worker failed to start; see {log_path}")
                time.sleep(0.5)
            def cli(*args):
                result = subprocess.run([str(binary), *args], cwd=cwd, env=env, text=True, capture_output=True, timeout=75)
                if result.returncode:
                    raise RuntimeError(f"CLI failed: {args}: {result.stderr}")
                return result
            cli("config", "set", "endpoint", endpoint)
            assert json.loads(cli("funnels", "list").stdout)["items"] == []
            first = cli("funnels", "create", "Outside checkout offer")
            created = json.loads(first.stdout)
            id = created["funnel"]["id"]
            operation = first.stderr.split("eez funnels retry ", 1)[1].split()[0]
            assert json.loads(cli("funnels", "retry", operation).stdout) == created
            second = json.loads(cli("funnels", "create", "Outside checkout offer", "--blueprint", "webinar").stdout)
            assert second["funnel"]["id"] != id
            assert len(json.loads(cli("funnels", "list").stdout)["items"]) == 2
            publication = cli("funnels", "publish", id)
            published = json.loads(publication.stdout)
            assert published["published_revision_id"] == created["initial_revision"]["id"]
            operation = publication.stderr.split("eez funnels retry ", 1)[1].split()[0]
            assert json.loads(cli("funnels", "retry", operation).stdout) == published
            assert json.loads(cli("funnels", "publish", id).stdout)["already_published"]
            with urllib.request.urlopen(endpoint + "/f/" + created["funnel"]["slug"], timeout=5) as response:
                assert response.status == 200 and b"Outside checkout offer" in response.read()
            print("PR-016=outside-checkout,isolated-config,local-D1,list,create-default,create-webinar,create-replay,fresh-create,publish-CAS,publish-replay,already-published,public-render")
        finally:
            def live_group():
                rows = subprocess.run(["ps", "-eo", "pgid=,stat="], capture_output=True, text=True, check=True, timeout=5).stdout.splitlines()
                return any(len(fields := row.split()) == 2 and fields[0] == str(worker.pid) and not fields[1].startswith("Z") for row in rows)
            for sig, limit in [(signal.SIGINT, 10), (signal.SIGTERM, 5), (signal.SIGKILL, 5)]:
                if not live_group():
                    break
                try:
                    os.killpg(worker.pid, sig)
                except ProcessLookupError:
                    break
                until = time.monotonic() + limit
                while live_group() and time.monotonic() < until:
                    time.sleep(0.1)
            worker.wait(timeout=5)
            assert not live_group(), "Worker process group has live members after cleanup"
    with socket.socket() as probe:
        assert probe.connect_ex(("127.0.0.1", port)) != 0, "Worker port remains open"
PY
