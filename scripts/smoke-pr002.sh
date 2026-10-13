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
assert_error_case() {
  local label="$1"
  local path="$2"
  local method="$3"
  local status="$4"
  local code="$5"
  local category="$6"
  local message="$7"
  local id_mode="$8"
  local requested_id=""
  local response_file="$state_dir/$label.json"
  local headers_file="$state_dir/$label.headers"
  local correlation_header=""
  local -a curl_args=(--silent --output "$response_file" --dump-header "$headers_file" --write-out '%{http_code}' -H 'Host: localhost:8787')

  case "$id_mode" in
    absent) ;;
    empty) curl_args+=(-H 'x-correlation-id;') ;;
    overlength)
      correlation_header="$(printf '%129s' '' | tr ' ' x)"
      curl_args+=(-H "x-correlation-id: $correlation_header")
      ;;
    provided)
      requested_id="pr002-preserved-$label"
      curl_args+=(-H "x-correlation-id: $requested_id")
      ;;
    *) return 2 ;;
  esac

  if [ "$method" != GET ]; then
    curl_args+=(--request "$method")
  fi
  local response_status
  response_status="$(curl "${curl_args[@]}" "http://127.0.0.1:8787$path")"
  python3 - "$response_file" "$response_status" "$status" "$code" "$category" "$message" "$id_mode" "$requested_id" "$method" "$path" /tmp/eezfunnel-pr002-wrangler.log <<'PY'
import json
import re
import sys
import uuid

response_path, actual_status, expected_status, code, category, message, id_mode, requested_id, method, path, log_path = sys.argv[1:]
with open(response_path, encoding="utf-8") as response_file:
    response = json.load(response_file)
assert actual_status == expected_status, (actual_status, expected_status)
assert response["code"] == code, response
assert response["category"] == category, response
assert response["message"] == message, response
correlation_id = response["correlation_id"]
if id_mode == "provided":
    assert correlation_id == requested_id, response
else:
    uuid.UUID(correlation_id)

records = []
ansi = re.compile(r"\x1b\[[0-9;]*m")
with open(log_path, encoding="utf-8", errors="replace") as log_file:
    for line in log_file:
        clean_line = ansi.sub("", line)
        start = clean_line.find("{")
        if start < 0:
            continue
        try:
            records.append(json.loads(clean_line[start:]))
        except json.JSONDecodeError:
            continue
assert any(record.get("event") == "api_error" and record.get("code") == code and record.get("correlation_id") == correlation_id for record in records), ("api_error correlation ID mismatch", response, records)
assert any(record.get("event") == "request" and record.get("method") == method and record.get("path") == path and record.get("status") == int(expected_status) and record.get("correlation_id") == correlation_id for record in records), ("request log correlation ID mismatch", response, records)
PY
  if [ "$status" = 405 ]; then
    tr -d '\r' < "$headers_file" | grep -Eiq '^Allow: GET,HEAD$'
  fi
  printf '%s=%s\n' "$label" "$(cat "$response_file")"
}

for id_mode in absent empty overlength provided; do
  assert_error_case "route-$id_mode" "/__route_does_not_exist/$id_mode" GET 404 not_found/route not_found "The requested route was not found." "$id_mode"
  assert_error_case "method-$id_mode" /health POST 405 validation/operation_not_supported validation "The requested operation is not supported." "$id_mode"
done
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
printf 'health=%s\nproduction_denial=%s\nprobe=%s\n' "$health" "$(cat "$state_dir/production-denial.json")" "$probe"
