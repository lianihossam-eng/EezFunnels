#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
source .project-tools/env.sh
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-/tmp/eezfunnel-xdg}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-/tmp/eezfunnel-cache}"
export NPM_CONFIG_CACHE="${NPM_CONFIG_CACHE:-/tmp/eezfunnel-npm-cache}"
state_dir="$(mktemp -d /tmp/eezfunnel-pr003-smoke.XXXXXX)"
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
cleanup() {
  stop_worker
  rm -rf "$state_dir"
}
trap cleanup EXIT
npm ci --no-audit --no-fund
./node_modules/.bin/wrangler d1 migrations apply DB --local --env dev --persist-to "$state_dir"
setsid ./node_modules/.bin/wrangler dev --local --env dev --var "LOCAL_PROBE_TOKEN:$probe_token" --port 8787 --ip 127.0.0.1 --persist-to "$state_dir" > /tmp/eezfunnel-pr003-wrangler.log 2>&1 &
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
assert_error_case() {
  local label="$1"
  local path="$2"
  local method="$3"
  local expected_status="$4"
  local expected_code="$5"
  local expected_category="$6"
  local expected_message="$7"
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
      requested_id="pr003-preserved-$label"
      curl_args+=(-H "x-correlation-id: $requested_id")
      ;;
    *) return 2 ;;
  esac

  if [ "$method" != GET ]; then
    curl_args+=(--request "$method")
  fi
  local actual_status
  actual_status="$(curl "${curl_args[@]}" "http://127.0.0.1:8787$path")"
  python3 - "$response_file" "$actual_status" "$expected_status" "$expected_code" "$expected_category" "$expected_message" "$id_mode" "$requested_id" "$method" "$path" /tmp/eezfunnel-pr003-wrangler.log <<'PY'
import json
import re
import sys
import time
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

ansi = re.compile(r"\x1b\[[0-9;]*m")
def matching_records():
    records = []
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
    api_error = any(
        record.get("event") == "api_error"
        and record.get("code") == code
        and record.get("correlation_id") == correlation_id
        for record in records
    )
    request = any(
        record.get("event") == "request"
        and record.get("method") == method
        and record.get("path") == path
        and record.get("status") == int(expected_status)
        and record.get("correlation_id") == correlation_id
        for record in records
    )
    return api_error, request, records

deadline = time.monotonic() + 5
while True:
    api_error_found, request_found, records = matching_records()
    if api_error_found and request_found:
        break
    if time.monotonic() >= deadline:
        assert api_error_found, ("api_error correlation ID mismatch", response, records)
        assert request_found, ("request log correlation ID mismatch", response, records)
    time.sleep(0.1)
PY
  if [ "$expected_status" = 405 ]; then
    tr -d '\r' < "$headers_file" | grep -Eiq '^Allow: GET,HEAD$'
  fi
  printf '%s=%s\n' "$label" "$(cat "$response_file")"
}

for id_mode in absent empty overlength provided; do
  assert_error_case "route-$id_mode" "/__pr003_route_missing/$id_mode" GET 404 not_found/route not_found "The requested route was not found." "$id_mode"
  assert_error_case "method-$id_mode" /health POST 405 validation/operation_not_supported validation "The requested operation is not supported." "$id_mode"
done
unauthorized_status="$(curl --silent --output "$state_dir/unauthorized.json" --write-out '%{http_code}' --request POST -H 'Host: localhost:8787' http://127.0.0.1:8787/__local/product-probe)"
python3 - "$state_dir/unauthorized.json" "$unauthorized_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "404"
assert response["code"] == "not_found/route"
assert response["category"] == "not_found"
PY
product_probe="$(curl --fail --silent --show-error --request POST -H 'Host: localhost:8787' -H "x-local-probe-token: $probe_token" -H 'x-correlation-id: pr003-smoke-products' http://127.0.0.1:8787/__local/product-probe)"
printf '%s\n' "$product_probe" > "$state_dir/product-probe.json"
python3 - "$state_dir/product-probe.json" "$state_dir/publication-race-ids.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert response["status"] == "ok"
assert response["funnels"] == "ok"
assert response["revisions"] == "ok"
assert response["themes"] == "ok"
assert response["publication"] == "ok"
assert response["unsupported_schema_version"] == "rejected"
observed = {
    "cross_funnel_published_revision_insert_accepted": response[
        "cross_funnel_published_revision_insert_accepted"
    ],
    "null_published_revision_insert_accepted": response[
        "null_published_revision_insert_accepted"
    ],
    "unsupported_schema_publish_accepted": response["unsupported_schema_publish_accepted"],
    "unsupported_schema_published_revision_id": response[
        "unsupported_schema_published_revision_id"
    ],
    "current_revision_noop_accepted": response["current_revision_noop_accepted"],
    "publication_baseline_cas_successes": response["publication_baseline_cas_successes"],
    "distinct_candidate_cas_successes": response["distinct_candidate_cas_successes"],
    "publication_replay_interleavings_safe": response[
        "publication_replay_interleavings_safe"
    ],
}
print(f"publication_regression_observations={json.dumps(observed, sort_keys=True)}")
failures = []
if observed["cross_funnel_published_revision_insert_accepted"]:
    failures.append("cross-funnel publication pointer was accepted on insert")
if not observed["null_published_revision_insert_accepted"]:
    failures.append("NULL published revision pointer was rejected on insert")
if observed["unsupported_schema_publish_accepted"]:
    failures.append("unsupported schema version was publishable")
if observed["unsupported_schema_published_revision_id"] is not None:
    failures.append("unsupported publication changed the funnel pointer")
if observed["current_revision_noop_accepted"]:
    failures.append("re-publishing the current revision was counted as a CAS success")
if observed["publication_baseline_cas_successes"] != 1:
    failures.append("a published baseline produced more than one successful CAS")
if observed["distinct_candidate_cas_successes"] != 1:
    failures.append("distinct candidates did not produce exactly one winner")
if not observed["publication_replay_interleavings_safe"]:
    failures.append("late publication/unpublication replay reapplied a pointer mutation")
assert not failures, failures
assert response["correlation_id"] == "pr003-smoke-products"
assert response["publication_race_funnel_id"] == "pr003-race-funnel"
assert len(response["publication_race_revision_ids"]) == 100
with open(sys.argv[2], "w", encoding="utf-8") as output_file:
    json.dump(response["publication_race_revision_ids"], output_file)
PY
python3 - "$state_dir/publication-race-ids.json" "$probe_token" <<'PY'
import concurrent.futures
import json
import sys
import urllib.request

with open(sys.argv[1], encoding="utf-8") as ids_file:
    revision_ids = json.load(ids_file)
token = sys.argv[2]

def publish(index, revision_id):
    request = urllib.request.Request(
        f"http://127.0.0.1:8787/__local/publication-race?revision_id={revision_id}",
        method="POST",
        headers={
            "Host": "localhost:8787",
            "x-local-probe-token": token,
            "x-correlation-id": f"pr003-race-{index}",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        body = json.load(response)
    assert body["correlation_id"] == f"pr003-race-{index}", body
    return revision_id, body["published"]

with concurrent.futures.ThreadPoolExecutor(max_workers=100) as executor:
    results = list(executor.map(lambda item: publish(*item), enumerate(revision_ids)))

winners = [revision_id for revision_id, published in results if published]
assert len(winners) == 1, winners
request = urllib.request.Request(
    "http://127.0.0.1:8787/__local/publication-race",
    headers={
        "Host": "localhost:8787",
        "x-local-probe-token": token,
        "x-correlation-id": "pr003-race-status",
    },
)
with urllib.request.urlopen(request, timeout=30) as response:
    status = json.load(response)
assert status["published_revision_id"] == winners[0], (status, winners)
assert status["correlation_id"] == "pr003-race-status", status
baseline_revision_id = winners[0]
baseline_candidates = [revision_id for revision_id in revision_ids if revision_id != baseline_revision_id][:2]
assert len(baseline_candidates) == 2, baseline_candidates

def publish_from_baseline(index, revision_id):
    request = urllib.request.Request(
        f"http://127.0.0.1:8787/__local/publication-race?revision_id={revision_id}&expected_published_revision_id={baseline_revision_id}",
        method="POST",
        headers={
            "Host": "localhost:8787",
            "x-local-probe-token": token,
            "x-correlation-id": f"pr003-baseline-race-{index}",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        body = json.load(response)
    assert body["correlation_id"] == f"pr003-baseline-race-{index}", body
    return revision_id, body["published"]

with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
    baseline_results = list(
        executor.map(
            lambda item: publish_from_baseline(*item), enumerate(baseline_candidates)
        )
    )

baseline_winners = [revision_id for revision_id, published in baseline_results if published]
assert len(baseline_winners) == 1, (baseline_results, baseline_revision_id)
request = urllib.request.Request(
    "http://127.0.0.1:8787/__local/publication-race",
    headers={
        "Host": "localhost:8787",
        "x-local-probe-token": token,
        "x-correlation-id": "pr003-baseline-race-status",
    },
)
with urllib.request.urlopen(request, timeout=30) as response:
    baseline_status = json.load(response)
assert baseline_status["published_revision_id"] == baseline_winners[0], (
    baseline_status,
    baseline_winners,
)
cleanup = urllib.request.Request(
    "http://127.0.0.1:8787/__local/publication-race",
    method="DELETE",
    headers={
        "Host": "localhost:8787",
        "x-local-probe-token": token,
        "x-correlation-id": "pr003-race-cleanup",
    },
)
with urllib.request.urlopen(cleanup, timeout=30) as response:
    cleanup_result = json.load(response)
assert cleanup_result["status"] == "ok", cleanup_result
print(f"publication_race_candidates={len(revision_ids)} winners={len(winners)} winner={winners[0]}")
print(
    f"baseline_publication_candidates={len(baseline_candidates)} "
    f"winners={len(baseline_winners)} winner={baseline_winners[0]} "
    f"baseline={baseline_revision_id}"
)
PY
stop_worker
assert_port_closed
printf 'product_repository_probe=passed\nlocal_mode=only\nport_after_cleanup=closed\n'
