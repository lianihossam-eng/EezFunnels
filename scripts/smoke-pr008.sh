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

state_dir="$(mktemp -d /tmp/eezfunnel-pr008-smoke.XXXXXX)"
log_file="/tmp/eezfunnel-pr008-wrangler.log"
port=8787
worker_pid=""
worker_pgid=""

stop_worker() {
  if [ -z "$worker_pid" ]; then
    return
  fi

  kill -INT "$worker_pid" 2>/dev/null || true
  for attempt in $(seq 1 20); do
    worker_state="$(ps -o stat= -p "$worker_pid" 2>/dev/null | tr -d '[:space:]' || true)"
    if [ -z "$worker_state" ] || [[ "$worker_state" == Z* ]]; then
      break
    fi
    sleep 0.1
  done
  worker_state="$(ps -o stat= -p "$worker_pid" 2>/dev/null | tr -d '[:space:]' || true)"
  if [ -n "$worker_state" ] && [[ "$worker_state" != Z* ]]; then
    workerd_pids="$(ps -eo pid=,pgid=,comm=,stat= | awk -v pgid="$worker_pgid" '$2 == pgid && $3 == "workerd" && $4 !~ /^Z/ { print $1 }')"
    if [ -n "$workerd_pids" ]; then
      kill -TERM $workerd_pids 2>/dev/null || true
    fi
    if [ "$worker_pgid" = "$worker_pid" ] && [ "$worker_pgid" != "$$" ]; then
      kill -TERM -- "-$worker_pgid" 2>/dev/null || true
      sleep 1
      kill -KILL -- "-$worker_pgid" 2>/dev/null || true
    else
      kill -TERM "$worker_pid" 2>/dev/null || true
    fi
  fi
  wait "$worker_pid" 2>/dev/null || true
  worker_pid=""
  worker_pgid=""
}

assert_port_closed() {
  for attempt in $(seq 1 20); do
    if ! curl --silent --output /dev/null --connect-timeout 1 "http://127.0.0.1:$port/health"; then
      return 0
    fi
    sleep 0.1
  done
  printf 'Wrangler left port %s open after shutdown\n' "$port" >&2
  return 1
}

cleanup() {
  local had_worker="$worker_pid"
  local cleanup_status=0
  stop_worker
  if [ -n "$had_worker" ]; then
    assert_port_closed || cleanup_status=$?
  fi
  rm -rf "$state_dir"
  return "$cleanup_status"
}
trap cleanup EXIT

if curl --silent --output /dev/null --connect-timeout 1 "http://127.0.0.1:$port/health"; then
  printf 'Port %s is already occupied\n' "$port" >&2
  exit 1
fi

bash scripts/install-wasm-bindgen.sh
bash scripts/install-worker-build.sh
npm ci --no-audit --no-fund

setsid ./node_modules/.bin/wrangler dev --local --env dev --port "$port" --ip 127.0.0.1 --persist-to "$state_dir" > "$log_file" 2>&1 &
worker_pid=$!
worker_pgid="$(ps -o pgid= -p "$worker_pid" | tr -d '[:space:]')"
if [ "$worker_pgid" != "$worker_pid" ] || [ "$worker_pgid" = "$$" ]; then
  printf 'Wrangler did not start in its own process group (pid=%s pgid=%s)\n' "$worker_pid" "$worker_pgid" >&2
  exit 1
fi

base_url="http://127.0.0.1:$port"
ready=false
for attempt in $(seq 1 90); do
  if curl --silent --output /dev/null --connect-timeout 1 -H "Host: localhost:$port" "$base_url/health"; then
    ready=true
    break
  fi
  worker_state="$(ps -o stat= -p "$worker_pid" 2>/dev/null | tr -d '[:space:]' || true)"
  if [ -z "$worker_state" ] || [[ "$worker_state" == Z* ]]; then
    break
  fi
  sleep 1
done
if [ "$ready" != true ]; then
  cat "$log_file" >&2
  printf 'Wrangler did not become ready on port %s\n' "$port" >&2
  exit 1
fi

request_status() {
  local path="$1"
  local output="$2"
  local headers="$3"
  shift 3
  curl --silent --show-error --output "$output" --dump-header "$headers" --write-out '%{http_code}' "$@" \
    -H "Host: localhost:$port" "$base_url$path"
}

for entry in \
  'root|/|Dashboard' \
  'funnels|/funnels|Your funnels' \
  'detail|/funnels/demo-1|demo-1'; do
  IFS='|' read -r label path expected <<< "$entry"
  status="$(request_status "$path" "$state_dir/$label.html" "$state_dir/$label.headers")"
  test "$status" = 200
  tr -d '\r' < "$state_dir/$label.headers" | grep -Eiq '^content-type: text/html'
  python3 - "$state_dir/$label.html" "$expected" <<'PY'
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    html = response_file.read()
assert sys.argv[2] in html, (sys.argv[2], html)
assert 'id="main"' in html
assert 'id="dioxus-hydration-data" type="application/json"' in html
assert '/assets/bootstrap.js' in html
PY
  printf '%s=%s\n' "$label" "$status"
done

python3 - "$state_dir/root.html" <<'PY'
import base64
import json
import re
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    html = response_file.read()
match = re.search(
    r'<script id="dioxus-hydration-data" type="application/json">(.*?)</script>',
    html,
    re.DOTALL,
)
assert match, "hydration payload script is missing"
payload = json.loads(match.group(1))
data = payload["data"]
assert isinstance(data, str) and data
base64.b64decode(data, validate=True)
for key in ("debug_types", "debug_locations"):
    if key in payload:
        assert isinstance(payload[key], list)
PY

for asset in dashboard.css bootstrap.js eezfunnel.js eezfunnel_bg.wasm; do
  status="$(request_status "/assets/$asset" "$state_dir/$asset" "$state_dir/$asset.headers")"
  test "$status" = 200
  test -s "$state_dir/$asset"
  printf 'asset-%s=%s\n' "$asset" "$status"
done

while IFS= read -r module; do
  status="$(request_status "/assets/$module" "$state_dir/imported-module.js" "$state_dir/imported-module.headers")"
  test "$status" = 200
  test -s "$state_dir/imported-module.js"
  printf 'asset-module-%s=%s\n' "$module" "$status"
done < <(python3 - "$state_dir/eezfunnel.js" <<'PY'
import re
import sys

with open(sys.argv[1], encoding="utf-8") as module_file:
    source = module_file.read()
for module in re.findall(r"from ['\"]\./([^'\"]+)['\"]", source):
    print(module)
PY
)

head_status="$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' --head \
  -H "Host: localhost:$port" "$base_url/assets/dashboard.css")"
test "$head_status" = 200

for path in /api/funnels /__local/probe /funnels/demo-1/extra; do
  status="$(request_status "$path" "$state_dir/not-found.json" "$state_dir/not-found.headers")"
  python3 - "$state_dir/not-found.json" "$status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    body = json.load(response_file)
assert sys.argv[2] == "404", (sys.argv[2], body)
assert body["code"] == "not_found/route", body
assert "data" not in body
PY
done

probe_status="$(curl --silent --show-error --request POST --output "$state_dir/probe.json" --write-out '%{http_code}' \
  -H "Host: localhost:$port" "$base_url/__local/probe")"
python3 - "$state_dir/probe.json" "$probe_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    body = json.load(response_file)
assert sys.argv[2] == "404", (sys.argv[2], body)
assert body["code"] == "not_found/route", body
PY

denial_status="$(curl --silent --show-error --output "$state_dir/denial.json" --write-out '%{http_code}' \
  -H 'Host: not-allowed.example' "$base_url/")"
python3 - "$state_dir/denial.json" "$denial_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    body = json.load(response_file)
assert sys.argv[2] == "403", (sys.argv[2], body)
assert body["code"] == "authorization/host_not_allowed", body
PY

printf 'unknown-api-probe-routes=404\nunknown-host=403\n'
