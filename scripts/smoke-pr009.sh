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

state_dir="$(mktemp -d /tmp/eezfunnel-pr009-smoke.XXXXXX)"
log_file="/tmp/eezfunnel-pr009-wrangler.log"
funnel_ids_file="$state_dir/funnel-ids.txt"
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

cleanup() {
  stop_worker
  rm -rf "$state_dir"
}
trap cleanup EXIT

if curl --silent --output /dev/null --connect-timeout 1 "http://127.0.0.1:$port/health"; then
  printf 'Port %s is already occupied\n' "$port" >&2
  exit 1
fi

npm ci --no-audit --no-fund
bash scripts/install-worker-build.sh
./node_modules/.bin/wrangler d1 migrations apply DB --local --env dev --persist-to "$state_dir"
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

request() {
  local method="$1"
  local path="$2"
  local output="$3"
  local headers="$4"
  shift 4
  curl --silent --show-error --request "$method" --output "$output" --dump-header "$headers" --write-out '%{http_code}' \
    -H "Host: localhost:$port" "$@" "$base_url$path"
}

list_status="$(request GET /api/funnels "$state_dir/initial-list.json" "$state_dir/initial-list.headers")"
test "$list_status" = 200
python3 - "$state_dir/initial-list.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert response["items"] == [], response
PY

missing_key_status="$(request POST /api/funnels "$state_dir/missing-key.json" "$state_dir/missing-key.headers" \
  -H 'Content-Type: application/json' --data '{"name":"First offer","blueprint":"tripwire"}')"
python3 - "$state_dir/missing-key.json" "$state_dir/missing-key.headers" "$missing_key_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
with open(sys.argv[2], encoding="utf-8") as headers_file:
    headers = headers_file.read().lower()
assert sys.argv[3] == "400", (sys.argv[3], response)
assert response["code"] == "validation/missing_field", response
assert f"x-correlation-id: {response['correlation_id']}" in headers, headers
PY
create_payload='{"name":"First offer","blueprint":"tripwire"}'
create_status="$(request POST /api/funnels "$state_dir/create.json" "$state_dir/create.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr009-create-1' --data "$create_payload")"
test "$create_status" = 201
python3 - "$state_dir/create.json" "$state_dir/create.headers" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
with open(sys.argv[2], encoding="utf-8") as headers_file:
    headers = headers_file.read().lower()
assert response["command_id"] == "funnel.create", response
assert response["funnel"]["name"] == "First offer", response
assert response["funnel"]["published_revision_id"] is None, response
assert len(response["initial_revision"]["section_types"]) == 3, response
assert len(response["initial_revision"]["section_ids"]) == 3, response
assert f"location: /funnels/{response['funnel']['id']}" in headers, headers
assert f"x-correlation-id: {response['correlation_id']}" in headers, headers
PY
python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["id"])' \
  "$state_dir/create.json" > "$funnel_ids_file"

replay_status="$(request POST /api/funnels "$state_dir/replay.json" "$state_dir/replay.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr009-create-1' --data "$create_payload")"
test "$replay_status" = 201
python3 - "$state_dir/create.json" "$state_dir/replay.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as first_file:
    first = json.load(first_file)
with open(sys.argv[2], encoding="utf-8") as replay_file:
    replay = json.load(replay_file)
assert first["funnel"] == replay["funnel"], (first, replay)
assert first["initial_revision"] == replay["initial_revision"], (first, replay)
PY

conflict_status="$(request POST /api/funnels "$state_dir/conflict.json" "$state_dir/conflict.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr009-create-1' \
  --data '{"name":"Changed offer","blueprint":"tripwire"}')"
python3 - "$state_dir/conflict.json" "$conflict_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "409", (sys.argv[2], response)
assert response["code"] == "conflict/idempotency_payload_mismatch", response
PY

for blueprint in vsl webinar launch lead_gen; do
  blueprint_payload="{\"name\":\"Blueprint $blueprint\",\"blueprint\":\"$blueprint\"}"
  blueprint_status="$(request POST /api/funnels "$state_dir/$blueprint.json" "$state_dir/$blueprint.headers" \
    -H 'Content-Type: application/json' -H "Idempotency-Key: pr009-create-$blueprint" --data "$blueprint_payload")"
  python3 - "$state_dir/$blueprint.json" "$blueprint_status" "$blueprint" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
expected = {
    "vsl": ["hero", "testimonials", "cta"],
    "webinar": ["hero", "features", "testimonials", "cta"],
    "launch": ["hero", "features", "testimonials", "cta", "footer"],
    "lead_gen": ["hero", "features", "cta", "footer"],
}[sys.argv[3]]
assert sys.argv[2] == "201", (sys.argv[2], response)
assert response["initial_revision"]["section_types"] == expected, response
assert len(response["initial_revision"]["section_ids"]) == len(expected), response
PY
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["id"])' \
    "$state_dir/$blueprint.json" >> "$funnel_ids_file"
done

malformed_status="$(request POST /api/funnels "$state_dir/malformed.json" "$state_dir/malformed.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr009-malformed' --data '{')"
python3 - "$state_dir/malformed.json" "$malformed_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "400", (sys.argv[2], response)
assert response["code"] == "validation/invalid_json", response
PY

list_status="$(request GET /api/funnels "$state_dir/list.json" "$state_dir/list.headers")"
test "$list_status" = 200
python3 - "$state_dir/list.json" "$state_dir/create.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as list_file:
    listed = json.load(list_file)["items"]
with open(sys.argv[2], encoding="utf-8") as create_file:
    created_id = json.load(create_file)["funnel"]["id"]
assert len(listed) == 5, listed
assert created_id in [item["id"] for item in listed], listed
PY

funnel_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["id"])' "$state_dir/create.json")"
delete_status="$(request DELETE "/api/funnels/$funnel_id" "$state_dir/delete.json" "$state_dir/delete.headers" \
  -H 'Idempotency-Key: pr009-delete-1')"
test "$delete_status" = 200
replay_delete_status="$(request DELETE "/api/funnels/$funnel_id" "$state_dir/delete-replay.json" "$state_dir/delete-replay.headers" \
  -H 'Idempotency-Key: pr009-delete-1')"
test "$replay_delete_status" = 200
python3 - "$state_dir/delete.json" "$state_dir/delete-replay.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as first_file:
    first = json.load(first_file)
with open(sys.argv[2], encoding="utf-8") as replay_file:
    replay = json.load(replay_file)
assert first["deleted"] is True, first
assert replay["deleted"] is True, replay
assert first["funnel_id"] == replay["funnel_id"], (first, replay)
PY

delete_index=0
while IFS= read -r funnel_id_to_delete; do
  if [ "$funnel_id_to_delete" = "$funnel_id" ]; then
    continue
  fi
  delete_index=$((delete_index + 1))
  blueprint_delete_status="$(request DELETE "/api/funnels/$funnel_id_to_delete" \
    "$state_dir/delete-$delete_index.json" "$state_dir/delete-$delete_index.headers" \
    -H "Idempotency-Key: pr009-delete-blueprint-$delete_index")"
  test "$blueprint_delete_status" = 200
done < "$funnel_ids_file"

empty_status="$(request GET /api/funnels "$state_dir/empty-list.json" "$state_dir/empty-list.headers")"
test "$empty_status" = 200
python3 - "$state_dir/empty-list.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert response["items"] == [], response
PY

unknown_status="$(request DELETE /api/funnels/unknown-funnel "$state_dir/not-found.json" "$state_dir/not-found.headers" \
  -H 'Idempotency-Key: pr009-delete-missing')"
python3 - "$state_dir/not-found.json" "$unknown_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "404", (sys.argv[2], response)
assert response["code"] == "not_found/funnel", response
PY

method_status="$(request PUT /api/funnels "$state_dir/method.json" "$state_dir/method.headers")"
python3 - "$state_dir/method.json" "$state_dir/method.headers" "$method_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
with open(sys.argv[2], encoding="utf-8") as headers_file:
    headers = headers_file.read().lower()
assert sys.argv[3] == "405", (sys.argv[3], response)
assert response["code"] == "validation/operation_not_supported", response
assert "allow: get,post" in headers, headers
PY

denial_status="$(curl --silent --show-error --output "$state_dir/denial.json" --write-out '%{http_code}' \
  -H 'Host: not-allowed.example' "$base_url/api/funnels")"
python3 - "$state_dir/denial.json" "$denial_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "403", (sys.argv[2], response)
assert response["code"] == "authorization/host_not_allowed", response
PY

printf 'list=200 five-blueprints=created replay=201 payload-mismatch=409 delete=200 replay-delete=200 host-denial=403\n'
