#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
source .project-tools/env.sh
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-/tmp/eezfunnel-xdg}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-/tmp/eezfunnel-cache}"
export NPM_CONFIG_CACHE="${NPM_CONFIG_CACHE:-/tmp/eezfunnel-npm-cache}"
state_dir="$(mktemp -d /tmp/eezfunnel-pr002-smoke.XXXXXX)"
probe_token="$(python3 -c 'import secrets; print(secrets.token_urlsafe(32))')"
worker_pid=""
worker_pgid=""
stop_worker() {
  if [ -n "$worker_pid" ]; then
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
  fi
}
assert_port_closed() {
  for attempt in $(seq 1 20); do
    if ! curl --silent --output /dev/null --connect-timeout 1 http://127.0.0.1:8787/health; then
      return 0
    fi
    sleep 0.1
  done
  printf 'Wrangler left port 8787 open after shutdown\n' >&2
  return 1
}
start_worker() {
  setsid ./node_modules/.bin/wrangler dev --local "$@" --port 8787 --ip 127.0.0.1 --persist-to "$state_dir" > /tmp/eezfunnel-pr002-wrangler.log 2>&1 &
  worker_pid=$!
  worker_pgid="$(ps -o pgid= -p "$worker_pid" | tr -d '[:space:]')"
  if [ "$worker_pgid" != "$worker_pid" ] || [ "$worker_pgid" = "$$" ]; then
    printf 'Wrangler did not start in its own process group (pid=%s pgid=%s)\n' "$worker_pid" "$worker_pgid" >&2
    exit 1
  fi
  ready=false
  for attempt in $(seq 1 30); do
    if curl --silent --output /dev/null --connect-timeout 1 http://127.0.0.1:8787/health; then
      ready=true
      break
    fi
    sleep 1
  done
  test "$ready" = true
}
cleanup() {
  stop_worker
  rm -rf "$state_dir"
}
trap cleanup EXIT
npm ci --no-audit --no-fund
start_worker
production_health_status="$(curl --silent --output "$state_dir/production-health.json" --write-out '%{http_code}' -H 'Host: eezfunnel.workers.dev' -H 'x-correlation-id: pr002-production-health' http://127.0.0.1:8787/health)"
test "$production_health_status" = 200
python3 -c 'import json,sys; assert json.load(open(sys.argv[1])) == {"status":"ok"}' "$state_dir/production-health.json"
production_denial_status="$(curl --silent --output "$state_dir/production-denial.json" --write-out '%{http_code}' -H 'Host: localhost:8787' -H 'x-correlation-id: pr002-production-host-denial' http://127.0.0.1:8787/health)"
python3 - "$state_dir/production-denial.json" "$production_denial_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)

assert sys.argv[2] == "403"
assert response == {
    "code": "authorization/host_not_allowed",
    "category": "authorization",
    "message": "The request host is not allowed.",
    "correlation_id": "pr002-production-host-denial",
}
PY
stop_worker
assert_port_closed
./node_modules/.bin/wrangler d1 migrations apply DB --local --env dev --persist-to "$state_dir"
start_worker --env dev --var "LOCAL_PROBE_TOKEN:$probe_token"
health="$(curl --fail --silent --show-error -H 'Host: localhost:8787' -H 'x-correlation-id: pr002-smoke-health' http://127.0.0.1:8787/health)"
python3 -c 'import json,sys; assert json.load(sys.stdin) == {"status":"ok"}' <<<"$health"
unknown_route_status="$(curl --silent --output "$state_dir/unknown-route.json" --write-out '%{http_code}' -H 'Host: localhost:8787' -H 'x-correlation-id: pr002-unknown-route' http://127.0.0.1:8787/__route_does_not_exist)"
python3 - "$state_dir/unknown-route.json" "$unknown_route_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)

assert sys.argv[2] == "404"
assert response == {
    "code": "not_found/route",
    "category": "not_found",
    "message": "The requested route was not found.",
    "correlation_id": "pr002-unknown-route",
}
PY
probe_without_token_status="$(curl --silent --output "$state_dir/probe-without-token.json" --write-out '%{http_code}' -H 'Host: localhost:8787' -H 'x-correlation-id: pr002-probe-no-token' --request POST http://127.0.0.1:8787/__local/probe)"
python3 - "$state_dir/probe-without-token.json" "$probe_without_token_status" pr002-probe-no-token <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)

assert sys.argv[2] == "404"
assert response == {
    "code": "not_found/route",
    "category": "not_found",
    "message": "The requested route was not found.",
    "correlation_id": sys.argv[3],
}
PY
probe_get_status="$(curl --silent --output "$state_dir/probe-get.json" --write-out '%{http_code}' -H 'Host: localhost:8787' -H 'x-correlation-id: pr002-probe-get' http://127.0.0.1:8787/__local/probe)"
python3 - "$state_dir/probe-get.json" "$probe_get_status" pr002-probe-get <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)

assert sys.argv[2] == "404"
assert response == {
    "code": "not_found/route",
    "category": "not_found",
    "message": "The requested route was not found.",
    "correlation_id": sys.argv[3],
}
PY
probe="$(curl --fail --silent --show-error --request POST -H 'Host: localhost:8787' -H "x-local-probe-token: $probe_token" -H 'x-correlation-id: pr002-smoke-storage' http://127.0.0.1:8787/__local/probe)"
python3 -c 'import json,sys; body=json.load(sys.stdin); assert body == {"status":"ok","d1":"ok","r2":"ok","kv":"ok","correlation_id":"pr002-smoke-storage"}' <<<"$probe"
printf 'health=%s\nproduction_denial=%s\nunknown_route=%s\nprobe=%s\n' "$health" "$(cat "$state_dir/production-denial.json")" "$(cat "$state_dir/unknown-route.json")" "$probe"
