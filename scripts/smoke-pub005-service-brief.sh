#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export WRANGLER_SEND_METRICS=false
state_dir="$(mktemp -d "${TMPDIR:-/tmp}/eezfunnel-pub005-d1.XXXXXX")"
port=18793
base_url="http://localhost:${port}"
funnel_id="11111111-1111-4111-8111-111111111111"
worker_pid=""

cleanup() {
  if [[ -n "$worker_pid" ]] && kill -0 "$worker_pid" 2>/dev/null; then
    kill -TERM -- "-$worker_pid" 2>/dev/null || kill "$worker_pid" 2>/dev/null || true
    for _ in $(seq 1 50); do
      if ! kill -0 "$worker_pid" 2>/dev/null; then break; fi
      sleep 0.1
    done
    kill -KILL -- "-$worker_pid" 2>/dev/null || true
  fi
  rm -rf "$state_dir"
}
trap cleanup EXIT

./node_modules/.bin/wrangler d1 migrations apply DB --local --env dev --persist-to "$state_dir"
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" \
  --command "INSERT INTO funnels (id, name, slug) VALUES ('${funnel_id}', 'PUB-005 fixture', 'pub005-fixture')"

log_file="${TMPDIR:-/tmp}/eezfunnel-pub005-wrangler.log"
setsid ./node_modules/.bin/wrangler dev --local --env dev --port "$port" --ip 127.0.0.1 \
  --var "ALLOWED_HOSTS:localhost:${port}" \
  --var "PUBLIC_FUNNEL_HOSTS:custom.example=pub005-fixture" \
  --persist-to "$state_dir" >"$log_file" 2>&1 &
worker_pid=$!

python3 - "$base_url" "$log_file" <<'PY'
import sys
import time
import urllib.error
import urllib.request

base_url, log_file = sys.argv[1:]
deadline = time.monotonic() + 180
while time.monotonic() < deadline:
    try:
        with urllib.request.urlopen(base_url + "/health", timeout=1) as response:
            if response.status == 200:
                break
    except (OSError, urllib.error.URLError):
        time.sleep(0.5)
else:
    raise SystemExit(f"Local Worker failed to start; see {log_file}")
PY

PUB005_BASE_URL="$base_url" PUB005_FUNNEL_ID="$funnel_id" \
  node scripts/test-pub005-service-brief-browser.mjs
