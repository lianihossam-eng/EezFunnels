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
worker_sid=""
worker_launcher_pid=""

worker_tree_pids() {
  local pending=("$1")
  local seen=()
  local current children child
  while [ "${#pending[@]}" -gt 0 ]; do
    current="${pending[0]}"
    pending=("${pending[@]:1}")
    case " ${seen[*]} " in
      *" $current "*) continue ;;
    esac
    seen+=("$current")
    children="$(ps -o pid= --ppid "$current" 2>/dev/null | awk '{$1=$1; print}' || true)"
    if [ -n "$children" ]; then
      while IFS= read -r child; do
        [ -n "$child" ] && pending+=("$child")
      done <<< "$children"
    fi
  done
  printf '%s\n' "${seen[@]}"
}

stop_worker() {
  if [ -z "$worker_launcher_pid" ] && [ -z "$worker_pid" ]; then
    return
  fi

  if [ -n "$worker_pid" ] && [ "$worker_pgid" = "$worker_pid" ] && [ "$worker_pgid" != "$$" ]; then
    kill -INT -- "-$worker_pgid" 2>/dev/null || true
    for attempt in $(seq 1 20); do
      launcher_state="$(ps -o stat= -p "$worker_launcher_pid" 2>/dev/null | tr -d '[:space:]' || true)"
      if [ -z "$launcher_state" ] || [[ "$launcher_state" == Z* ]]; then
        break
      fi
      sleep 0.1
    done
    launcher_state="$(ps -o stat= -p "$worker_launcher_pid" 2>/dev/null | tr -d '[:space:]' || true)"
    if [ -n "$launcher_state" ] && [[ "$launcher_state" != Z* ]]; then
      kill -TERM -- "-$worker_pgid" 2>/dev/null || true
      sleep 1
      kill -KILL -- "-$worker_pgid" 2>/dev/null || true
    fi
  else
    if [ -n "$worker_launcher_pid" ]; then
      worker_pids="$(worker_tree_pids "$worker_launcher_pid")"
      if [ -n "$worker_pids" ]; then
        while IFS= read -r pid; do
          kill -INT "$pid" 2>/dev/null || true
        done <<< "$worker_pids"
      fi
    elif [ -n "$worker_pid" ]; then
      kill -INT "$worker_pid" 2>/dev/null || true
    fi
    sleep 2
    if [ -n "$worker_launcher_pid" ]; then
      worker_pids="$(worker_tree_pids "$worker_launcher_pid")"
      if [ -n "$worker_pids" ]; then
        while IFS= read -r pid; do
          kill -TERM "$pid" 2>/dev/null || true
        done <<< "$worker_pids"
        sleep 1
        worker_pids="$(worker_tree_pids "$worker_launcher_pid")"
        if [ -n "$worker_pids" ]; then
          while IFS= read -r pid; do
            kill -KILL "$pid" 2>/dev/null || true
          done <<< "$worker_pids"
        fi
      fi
    fi
  fi

  if [ -n "$worker_launcher_pid" ]; then
    wait "$worker_launcher_pid" 2>/dev/null || true
  elif [ -n "$worker_pid" ]; then
    wait "$worker_pid" 2>/dev/null || true
  fi
  worker_pid=""
  worker_pgid=""
  worker_launcher_pid=""
}

cleanup() {
  local had_worker="$worker_launcher_pid"
  local cleanup_status=0
  stop_worker
  if [ -n "$had_worker" ]; then
    for attempt in $(seq 1 20); do
      if ! curl --silent --output /dev/null --connect-timeout 1 "http://127.0.0.1:$port/health"; then
        break
      fi
      sleep 0.1
    done
    if curl --silent --output /dev/null --connect-timeout 1 "http://127.0.0.1:$port/health"; then
      printf 'Wrangler left port %s open after shutdown\n' "$port" >&2
      cleanup_status=1
    fi
  fi
  rm -rf "$state_dir"
  return "$cleanup_status"
}
trap cleanup EXIT

if curl --silent --output /dev/null --connect-timeout 1 "http://127.0.0.1:$port/health"; then
  printf 'Port %s is already occupied\n' "$port" >&2
  exit 1
fi

npm ci --no-audit --no-fund
bash scripts/install-worker-build.sh
./node_modules/.bin/wrangler d1 migrations apply DB --local --env dev --persist-to "$state_dir"
setsid --fork --wait ./node_modules/.bin/wrangler dev --local --env dev --port "$port" --ip 127.0.0.1 --persist-to "$state_dir" > "$log_file" 2>&1 &
worker_launcher_pid=$!
for attempt in $(seq 1 100); do
  candidate_pids="$(ps -o pid= --ppid "$worker_launcher_pid" 2>/dev/null | awk '{$1=$1; print}' || true)"
  while IFS= read -r candidate_pid; do
    [ -n "$candidate_pid" ] || continue
    candidate_pgid="$(ps -o pgid= -p "$candidate_pid" 2>/dev/null | tr -d '[:space:]' || true)"
    candidate_sid="$(ps -o sid= -p "$candidate_pid" 2>/dev/null | tr -d '[:space:]' || true)"
    candidate_state="$(ps -o stat= -p "$candidate_pid" 2>/dev/null | tr -d '[:space:]' || true)"
    if [ "$candidate_pgid" = "$candidate_pid" ] && [ "$candidate_sid" = "$candidate_pid" ] && \
      [ -n "$candidate_state" ] && [[ "$candidate_state" != Z* ]]; then
      worker_pid="$candidate_pid"
      worker_pgid="$candidate_pgid"
      worker_sid="$candidate_sid"
      break 2
    fi
  done <<< "$candidate_pids"
  launcher_state="$(ps -o stat= -p "$worker_launcher_pid" 2>/dev/null | tr -d '[:space:]' || true)"
  if [ -z "$launcher_state" ] || [[ "$launcher_state" == Z* ]]; then
    break
  fi
  sleep 0.02
done
if [ -z "$worker_pid" ] || [ "$worker_pgid" = "$$" ]; then
  printf 'Wrangler did not start in its own session and process group (launcher=%s pid=%s pgid=%s sid=%s)\n' \
    "$worker_launcher_pid" "$worker_pid" "$worker_pgid" "$worker_sid" >&2
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

seed_expired_receipt() {
  local key="$1"
  ./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" \
    --command "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) VALUES ('$key', 'funnel.create', '{}', '{}', 0);" >/dev/null
}

assert_no_expired_receipts() {
  local output_file="$state_dir/expired-receipts.json"
  ./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" --json \
    --command "SELECT COUNT(*) AS expired_count FROM command_receipts WHERE expires_at <= CAST(strftime('%s', 'now') AS INTEGER);" > "$output_file"
  python3 - "$output_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as result_file:
    result = json.load(result_file)
expired_count = result[-1]["results"][0]["expired_count"]
assert expired_count == 0, result
PY
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

seed_expired_receipt pr009-expired-before-create
cleanup_create_status="$(request POST /api/funnels "$state_dir/receipt-cleanup-create.json" \
  "$state_dir/receipt-cleanup-create.headers" -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: pr009-create-after-expiry' \
  --data '{"name":"Receipt cleanup create","blueprint":"tripwire"}')"
test "$cleanup_create_status" = 201
assert_no_expired_receipts
python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["id"])' \
  "$state_dir/receipt-cleanup-create.json" >> "$funnel_ids_file"

funnel_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["id"])' "$state_dir/create.json")"
seed_expired_receipt pr009-expired-before-delete
delete_status="$(request DELETE "/api/funnels/$funnel_id" "$state_dir/delete.json" "$state_dir/delete.headers" \
  -H 'Idempotency-Key: pr009-delete-1')"
test "$delete_status" = 200
assert_no_expired_receipts
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

printf 'list=200 five-blueprints=created replay=201 payload-mismatch=409 receipt-cleanup=create,delete delete=200 replay-delete=200 host-denial=403\n'
