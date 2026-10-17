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
worker_owned_processes=()

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

process_group_has_live_members() {
  local group_id="$1"
  [ -n "$group_id" ] || return 1
  ps -eo pgid=,stat= | awk -v group_id="$group_id" \
    '$1 == group_id && $2 !~ /^Z/ { found = 1 } END { exit !found }'
}

process_start_time() {
  local pid="$1"
  [ -r "/proc/$pid/stat" ] || return 1
  awk '{
    delimiter = 0
    for (i = 1; i <= length($0); i++) {
      if (substr($0, i, 1) == ")") delimiter = i
    }
    if (delimiter == 0) exit 1
    count = split(substr($0, delimiter + 2), fields, /[[:space:]]+/)
    if (count < 20) exit 1
    print fields[20]
  }' "/proc/$pid/stat" 2>/dev/null
}

worker_record_process() {
  local pid="$1"
  local identity start_time
  for identity in "${worker_owned_processes[@]}"; do
    [ "${identity%%:*}" = "$pid" ] && return 0
  done
  start_time="$(process_start_time "$pid" || true)"
  [ -n "$start_time" ] && worker_owned_processes+=("$pid:$start_time")
}

worker_record_tree() {
  local root="$1"
  local pid process_ids
  process_ids="$(worker_tree_pids "$root")"
  while IFS= read -r pid; do
    [ -n "$pid" ] && worker_record_process "$pid"
  done <<< "$process_ids"
}

worker_capture_owned_processes() {
  local identity
  local snapshot=("${worker_owned_processes[@]}")
  if [ -n "$worker_launcher_pid" ]; then
    worker_record_tree "$worker_launcher_pid"
  fi
  for identity in "${snapshot[@]}"; do
    worker_record_tree "${identity%%:*}"
  done
}

worker_process_is_live() {
  local identity="$1"
  local pid="${identity%%:*}"
  local expected_start_time="${identity#*:}"
  local current_start_time state
  current_start_time="$(process_start_time "$pid" || true)"
  [ -n "$current_start_time" ] && [ "$current_start_time" = "$expected_start_time" ] || return 1
  state="$(ps -o stat= -p "$pid" 2>/dev/null | tr -d '[:space:]' || true)"
  [ -n "$state" ] && [[ "$state" != Z* ]]
}

worker_has_live_owned_processes() {
  local identity
  for identity in "${worker_owned_processes[@]}"; do
    if worker_process_is_live "$identity"; then
      return 0
    fi
  done
  return 1
}

worker_signal_owned_processes() {
  local signal="$1"
  local identity pid
  for identity in "${worker_owned_processes[@]}"; do
    if worker_process_is_live "$identity"; then
      pid="${identity%%:*}"
      kill -"$signal" "$pid" 2>/dev/null || true
    fi
  done
}

stop_worker() {
  if [ -z "$worker_launcher_pid" ] && [ -z "$worker_pid" ]; then
    return
  fi

  if [ -n "$worker_pid" ] && [ "$worker_pgid" = "$worker_pid" ] && \
    [ "$worker_sid" = "$worker_pid" ] && [ "$worker_pgid" != "$$" ]; then
    kill -INT -- "-$worker_pgid" 2>/dev/null || true
    for attempt in $(seq 1 20); do
      if ! process_group_has_live_members "$worker_pgid"; then
        break
      fi
      sleep 0.1
    done
    if process_group_has_live_members "$worker_pgid"; then
      kill -TERM -- "-$worker_pgid" 2>/dev/null || true
      for attempt in $(seq 1 10); do
        if ! process_group_has_live_members "$worker_pgid"; then
          break
        fi
        sleep 0.1
      done
    fi
    if process_group_has_live_members "$worker_pgid"; then
      kill -KILL -- "-$worker_pgid" 2>/dev/null || true
    fi
  else
    worker_owned_processes=()
    if [ -n "$worker_launcher_pid" ]; then
      worker_record_tree "$worker_launcher_pid"
    elif [ -n "$worker_pid" ]; then
      worker_record_tree "$worker_pid"
    fi
    worker_signal_owned_processes INT
    for attempt in $(seq 1 20); do
      worker_capture_owned_processes
      if ! worker_has_live_owned_processes; then
        break
      fi
      sleep 0.1
    done
    if worker_has_live_owned_processes; then
      worker_signal_owned_processes TERM
      for attempt in $(seq 1 10); do
        worker_capture_owned_processes
        if ! worker_has_live_owned_processes; then
          break
        fi
        sleep 0.1
      done
    fi
    if worker_has_live_owned_processes; then
      worker_signal_owned_processes KILL
      for attempt in $(seq 1 10); do
        worker_capture_owned_processes
        if ! worker_has_live_owned_processes; then
          break
        fi
        sleep 0.1
      done
    fi
  fi

  if [ -n "$worker_launcher_pid" ]; then
    wait "$worker_launcher_pid" 2>/dev/null || true
  elif [ -n "$worker_pid" ]; then
    wait "$worker_pid" 2>/dev/null || true
  fi
  if [ "${#worker_owned_processes[@]}" -gt 0 ] && worker_has_live_owned_processes; then
    printf 'Fallback shutdown left an owned process alive\n' >&2
    return 1
  fi
  worker_pid=""
  worker_pgid=""
  worker_sid=""
  worker_launcher_pid=""
  worker_owned_processes=()
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

test_worker_group_cleanup_regression() {
  local fixture_file="$state_dir/worker-group-fixture.sh"
  local ready_file="$state_dir/worker-group-ready"
  local group_leader_file="$state_dir/worker-group-leader"
  local child_ready_file="$state_dir/worker-group-child-ready"
  local test_group test_launcher

  cat > "$fixture_file" <<'FIXTURE'
import os
import signal
import subprocess
import sys
import time

ready_file, group_leader_file, child_ready_file = sys.argv[1:]
signal.signal(signal.SIGINT, signal.SIG_DFL)
child_code = "import signal,sys,time; signal.signal(signal.SIGINT, signal.SIG_IGN); signal.signal(signal.SIGHUP, signal.SIG_IGN); open(sys.argv[1], 'w', encoding='utf-8').close(); exec('while True: time.sleep(1)')"
child = subprocess.Popen([sys.executable, "-c", child_code, child_ready_file], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(100):
    if os.path.exists(child_ready_file):
        break
    if child.poll() is not None:
        sys.exit("The cleanup fixture child exited before becoming ready")
    time.sleep(0.02)
else:
    child.terminate()
    sys.exit("The cleanup fixture child did not become ready")
with open(group_leader_file, "w", encoding="utf-8") as handle:
    handle.write(str(os.getpid()))
with open(ready_file, "w", encoding="utf-8") as handle:
    handle.write(str(child.pid))
while True:
    time.sleep(1)
FIXTURE

  setsid --fork --wait python3 "$fixture_file" "$ready_file" "$group_leader_file" "$child_ready_file" > "$state_dir/worker-group-fixture.log" 2>&1 &
  worker_launcher_pid=$!
  for attempt in $(seq 1 100); do
    if [ -s "$group_leader_file" ] && [ -s "$ready_file" ]; then
      worker_pid="$(cat "$group_leader_file")"
      worker_pgid="$(ps -o pgid= -p "$worker_pid" 2>/dev/null | tr -d '[:space:]' || true)"
      worker_sid="$(ps -o sid= -p "$worker_pid" 2>/dev/null | tr -d '[:space:]' || true)"
      if [ "$worker_pgid" = "$worker_pid" ] && [ "$worker_sid" = "$worker_pid" ]; then
        break
      fi
      worker_pid=""
      worker_pgid=""
      worker_sid=""
    fi
    if ! kill -0 "$worker_launcher_pid" 2>/dev/null; then
      break
    fi
    sleep 0.02
  done

  if [ -z "$worker_pid" ] || [ ! -s "$ready_file" ]; then
    printf 'Could not start the isolated process-group cleanup fixture\n' >&2
    stop_worker
    return 1
  fi

  test_group="$worker_pgid"
  test_launcher="$worker_launcher_pid"
  kill -INT -- "-$test_group" 2>/dev/null || true
  for attempt in $(seq 1 100); do
    launcher_state="$(ps -o stat= -p "$test_launcher" 2>/dev/null | tr -d '[:space:]' || true)"
    if [ -z "$launcher_state" ] || [[ "$launcher_state" == Z* ]]; then
      break
    fi
    sleep 0.02
  done

  launcher_state="$(ps -o stat= -p "$test_launcher" 2>/dev/null | tr -d '[:space:]' || true)"
  if [ -n "$launcher_state" ] && [[ "$launcher_state" != Z* ]]; then
    printf 'The cleanup fixture launcher did not exit after SIGINT\n' >&2
    stop_worker
    return 1
  fi
  if ! process_group_has_live_members "$test_group"; then
    printf 'The cleanup fixture did not leave a live group member after its launcher exited\n' >&2
    stop_worker
    return 1
  fi
  printf 'worker-group-state=launcher-exited pid=%s group=%s member-remains\n' "$test_launcher" "$test_group"
  ps -eo pid=,pgid=,sid=,stat= | awk -v group_id="$test_group" '$2 == group_id && $4 !~ /^Z/'

  stop_worker
  if process_group_has_live_members "$test_group"; then
    kill -TERM -- "-$test_group" 2>/dev/null || true
    sleep 0.1
    kill -KILL -- "-$test_group" 2>/dev/null || true
    printf 'Worker group %s still had live members after cleanup\n' "$test_group" >&2
    return 1
  fi
  printf 'worker-group-cleanup=verified-launcher-exits-before-member\n'
}

test_worker_fallback_reparent_cleanup_regression() {
  local fixture_file="$state_dir/worker-fallback-fixture.py"
  local ready_file="$state_dir/worker-fallback-ready"
  local leader_file="$state_dir/worker-fallback-leader"
  local term_file="$state_dir/worker-fallback-term"
  local test_group test_launcher test_leader test_child test_child_start leader_state child_state
  local term_child term_parent

  cat > "$fixture_file" <<'FIXTURE'
import os
import signal
import subprocess
import sys
import time

ready_file, leader_file, term_file = sys.argv[1:]
signal.signal(signal.SIGINT, signal.SIG_DFL)
child_code = "import os,signal,sys,time\ndef on_term(signum, frame):\n with open(sys.argv[1], 'w', encoding='utf-8') as f: f.write(f'{os.getpid()}:{os.getppid()}\\n')\nsignal.signal(signal.SIGINT, signal.SIG_IGN)\nsignal.signal(signal.SIGHUP, signal.SIG_IGN)\nsignal.signal(signal.SIGTERM, on_term)\nwhile True: time.sleep(1)"
child = subprocess.Popen([sys.executable, "-c", child_code, term_file], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
with open(leader_file, "w", encoding="utf-8") as handle:
    handle.write(str(os.getpid()))
with open(ready_file, "w", encoding="utf-8") as handle:
    handle.write(str(child.pid))
while True:
    time.sleep(1)
FIXTURE

  setsid --fork --wait python3 "$fixture_file" "$ready_file" "$leader_file" "$term_file" \
    > "$state_dir/worker-fallback-fixture.log" 2>&1 &
  worker_launcher_pid=$!
  for attempt in $(seq 1 100); do
    if [ -s "$leader_file" ] && [ -s "$ready_file" ]; then
      worker_pid="$(cat "$leader_file")"
      test_child="$(cat "$ready_file")"
      test_child_start="$(process_start_time "$test_child" || true)"
      worker_pgid="$(ps -o pgid= -p "$worker_pid" 2>/dev/null | tr -d '[:space:]' || true)"
      worker_sid="$(ps -o sid= -p "$worker_pid" 2>/dev/null | tr -d '[:space:]' || true)"
      if [ -n "$test_child_start" ] && [ "$worker_pgid" = "$worker_pid" ] && \
        [ "$worker_sid" = "$worker_pid" ]; then
        break
      fi
    fi
    sleep 0.02
  done

  if [ -z "${worker_pid:-}" ] || [ -z "${test_child:-}" ] || [ -z "${test_child_start:-}" ] || \
    [ "$worker_pgid" != "$worker_pid" ] || [ "$worker_sid" != "$worker_pid" ]; then
    printf 'Could not start the isolated fallback cleanup fixture\n' >&2
    stop_worker
    return 1
  fi

  test_group="$worker_pgid"
  test_launcher="$worker_launcher_pid"
  test_leader="$worker_pid"
  worker_sid=""
  stop_worker

  leader_state="$(ps -o stat= -p "$test_leader" 2>/dev/null | tr -d '[:space:]' || true)"
  child_state="$(ps -o stat= -p "$test_child" 2>/dev/null | tr -d '[:space:]' || true)"
  term_child=""
  term_parent=""
  if [ -s "$term_file" ]; then
    IFS=: read -r term_child term_parent < "$term_file"
  fi
  if [ ! -s "$term_file" ] || [ "$term_child" != "$test_child" ] || \
    [ "$term_parent" = "$test_leader" ] || worker_process_is_live "$test_child:$test_child_start" || \
    { [ -n "$leader_state" ] && [[ "$leader_state" != Z* ]]; }; then
    printf 'Fallback cleanup missed a reparented child after launcher exit (launcher=%s leader=%s child=%s child_state=%s term=%s term_parent=%s)\n' \
      "$test_launcher" "$test_leader" "$test_child" "${child_state:-absent}" \
      "$([ -s "$term_file" ] && printf received || printf missing)" "${term_parent:-unknown}" >&2
    kill -TERM -- "-$test_group" 2>/dev/null || true
    sleep 0.1
    kill -KILL -- "-$test_group" 2>/dev/null || true
    wait "$test_launcher" 2>/dev/null || true
    return 1
  fi

  printf 'worker-fallback-cleanup=launcher-exited-child-reparented term=received kill=verified\n'
}

trap cleanup EXIT

if [ "${1:-}" = "--test-worker-fallback-cleanup" ]; then
  test_worker_fallback_reparent_cleanup_regression
  exit 0
fi

test_worker_group_cleanup_regression
test_worker_fallback_reparent_cleanup_regression
if [ "${1:-}" = "--test-worker-group-cleanup" ]; then
  exit 0
fi

if curl --silent --output /dev/null --connect-timeout 1 "http://127.0.0.1:$port/health"; then
  printf 'Port %s is already occupied\n' "$port" >&2
  exit 1
fi

npm ci --no-audit --no-fund
bash scripts/install-worker-build.sh
./node_modules/.bin/wrangler d1 migrations apply DB --local --env dev --persist-to "$state_dir"
setsid --fork --wait ./node_modules/.bin/wrangler dev --local --env dev --test-scheduled --port "$port" --ip 127.0.0.1 --persist-to "$state_dir" > "$log_file" 2>&1 &
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

assert_error_response() {
  local response_file="$1"
  local actual_status="$2"
  local expected_status="$3"
  local expected_code="$4"
  python3 - "$response_file" "$actual_status" "$expected_status" "$expected_code" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == sys.argv[3], (sys.argv[2], response)
assert response["code"] == sys.argv[4], response
PY
}

seed_expired_receipt() {
  local key="$1"
  local command_id="${2:-funnel.create}"
  ./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" \
    --command "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) VALUES ('$key', '$command_id', '{}', '{}', 0);" >/dev/null
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

funnel_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["id"])' "$state_dir/create.json")"
funnel_slug="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["slug"])' "$state_dir/create.json")"
revision_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["initial_revision"]["id"])' "$state_dir/create.json")"

expired_stage_hash="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
expired_attempt_id="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
expired_receiving_attempt_id="cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" \
  --command "INSERT INTO v2_document_stages (stage_id_hash, funnel_id, base_revision_id, purpose, document_type, document_version, schema_version, manifest_version, maximum_bytes, expected_length, expected_sha256, expires_at, state) VALUES ('$expired_stage_hash', '$funnel_id', '$revision_id', 'full_snapshot', 'eezfunnel.document', 2, 2, 2, 1048576, 1, '0000000000000000000000000000000000000000000000000000000000000000', CAST(strftime('%s', 'now') AS INTEGER) - 1, 'issued');" >/dev/null
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" \
  --command "INSERT INTO v2_document_stage_attempts (attempt_id, stage_id_hash, state, expected_length, expected_sha256, deadline_at, tombstone_expires_at) VALUES ('$expired_attempt_id', '$expired_stage_hash', 'abandoned', 1, '0000000000000000000000000000000000000000000000000000000000000000', CAST(strftime('%s', 'now') AS INTEGER) - 3600, CAST(strftime('%s', 'now') AS INTEGER) - 1);" >/dev/null
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" \
  --command "INSERT INTO v2_document_stage_attempts (attempt_id, stage_id_hash, state, expected_length, expected_sha256, deadline_at) VALUES ('$expired_receiving_attempt_id', '$expired_stage_hash', 'receiving', 1, '0000000000000000000000000000000000000000000000000000000000000000', CAST(strftime('%s', 'now') AS INTEGER) - 3600);" >/dev/null
scheduled_status="$(request GET '/__scheduled?cron=%2A%2F15+%2A+%2A+%2A+%2A' "$state_dir/scheduled.txt" "$state_dir/scheduled.headers")"
test "$scheduled_status" = 200
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" --json \
  --command "SELECT (SELECT COUNT(*) FROM v2_document_stages WHERE stage_id_hash = '$expired_stage_hash' AND slot_held = 1) AS held_stage_count, (SELECT COUNT(*) FROM v2_document_stage_attempts WHERE attempt_id IN ('$expired_attempt_id', '$expired_receiving_attempt_id') AND quota_held = 1) AS held_attempt_count;" > "$state_dir/scheduled-cleanup.json"
python3 - "$state_dir/scheduled-cleanup.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as result_file:
    result = json.load(result_file)
state = result[-1]["results"][0]
assert state["held_stage_count"] == 0, state
assert state["held_attempt_count"] == 0, state
PY
printf 'scheduled-stage-cleanup=expired-stage-and-both-attempt-quotas-released\n'

foreign_create_status="$(request POST /api/funnels "$state_dir/foreign-create.json" "$state_dir/foreign-create.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-foreign-create' \
  --data '{"name":"Foreign funnel","blueprint":"tripwire"}')"
test "$foreign_create_status" = 201
foreign_funnel_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["id"])' "$state_dir/foreign-create.json")"
cross_funnel_publish_status="$(request POST "/api/funnels/$foreign_funnel_id/publish" "$state_dir/cross-funnel-publish.json" "$state_dir/cross-funnel-publish.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-cross-funnel-publish' \
  --data "{\"revision_id\":\"$revision_id\",\"expected_published_revision_id\":null}")"
assert_error_response "$state_dir/cross-funnel-publish.json" "$cross_funnel_publish_status" 404 not_found/revision
foreign_delete_status="$(request DELETE "/api/funnels/$foreign_funnel_id" "$state_dir/foreign-delete.json" "$state_dir/foreign-delete.headers" \
  -H 'Idempotency-Key: pr014-foreign-delete')"
test "$foreign_delete_status" = 200

section_ids='["hero","features","cta"]'
revision_2_save_status="$(request PUT "/api/funnels/$funnel_id/revisions" "$state_dir/revision-2-save.json" "$state_dir/revision-2-save.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-v2-save' \
  --data "{\"base_revision_id\":\"$revision_id\",\"section_ids\":$section_ids,\"settings\":{\"schema_version\":1,\"changes\":[{\"id\":\"section.hero.title\",\"value\":{\"kind\":\"text\",\"value\":\"Published revision two\"}},{\"id\":\"theme.primary_color\",\"value\":{\"kind\":\"color\",\"value\":\"#123456\"}}]}}")"
test "$revision_2_save_status" = 200
revision_2="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["revision_id"])' "$state_dir/revision-2-save.json")"
revision_3_save_status="$(request PUT "/api/funnels/$funnel_id/revisions" "$state_dir/revision-3-save.json" "$state_dir/revision-3-save.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-v3-save' \
  --data "{\"base_revision_id\":\"$revision_2\",\"section_ids\":$section_ids,\"settings\":{\"schema_version\":1,\"changes\":[{\"id\":\"section.hero.title\",\"value\":{\"kind\":\"text\",\"value\":\"Published race candidate three\"}}]}}")"
test "$revision_3_save_status" = 200
revision_3="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["revision_id"])' "$state_dir/revision-3-save.json")"
revision_4_save_status="$(request PUT "/api/funnels/$funnel_id/revisions" "$state_dir/revision-4-save.json" "$state_dir/revision-4-save.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-v4-save' \
  --data "{\"base_revision_id\":\"$revision_3\",\"section_ids\":$section_ids,\"settings\":{\"schema_version\":1,\"changes\":[{\"id\":\"section.hero.title\",\"value\":{\"kind\":\"text\",\"value\":\"Published race candidate four\"}}]}}")"
test "$revision_4_save_status" = 200
revision_4="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["revision_id"])' "$state_dir/revision-4-save.json")"

publish_payload="{\"revision_id\":\"$revision_id\",\"expected_published_revision_id\":null}"
publish_status="$(request POST "/api/funnels/$funnel_id/publish" "$state_dir/publish.json" "$state_dir/publish.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-1' --data "$publish_payload")"
test "$publish_status" = 200
published_status="$(request GET "/f/$funnel_slug" "$state_dir/published.html" "$state_dir/published.headers")"
test "$published_status" = 200
python3 - "$state_dir/publish.json" "$state_dir/published.html" "$state_dir/published.headers" "$revision_id" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
with open(sys.argv[2], encoding="utf-8") as html_file:
    html = html_file.read()
with open(sys.argv[3], encoding="utf-8") as headers_file:
    headers = headers_file.read().lower()
assert response["published_revision_id"] == sys.argv[4], response
assert "<h1>First offer</h1>" in html, html
assert "etag: \"" in headers, headers
assert "cache-control: public" in headers, headers
PY
invalid_publish_route_replay_status="$(request POST "/api/funnels/invalid!/publish" "$state_dir/invalid-publish-route-replay.json" "$state_dir/invalid-publish-route-replay.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-1' --data "$publish_payload")"
assert_error_response "$state_dir/invalid-publish-route-replay.json" "$invalid_publish_route_replay_status" 409 conflict/idempotency_payload_mismatch
invalid_publish_route_fresh_status="$(request POST "/api/funnels/invalid!/publish" "$state_dir/invalid-publish-route-fresh.json" "$state_dir/invalid-publish-route-fresh.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-invalid-route-fresh' --data "$publish_payload")"
assert_error_response "$state_dir/invalid-publish-route-fresh.json" "$invalid_publish_route_fresh_status" 404 not_found/funnel
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" --json \
  --command "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) VALUES ('pr014-unsupported-public-revision', '$funnel_id', 3, '{}', '{}');" \
  > "$state_dir/insert-unsupported-public-revision.json"
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" --json \
  --command "UPDATE funnels SET published_revision_id = 'pr014-unsupported-public-revision' WHERE id = '$funnel_id';" \
  > "$state_dir/publish-unsupported-public-revision.json"
unsupported_public_status="$(request GET "/f/$funnel_slug" "$state_dir/unsupported-public.json" "$state_dir/unsupported-public.headers")"
assert_error_response "$state_dir/unsupported-public.json" "$unsupported_public_status" 400 validation/schema_version_unsupported
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" --json \
  --command "UPDATE funnels SET published_revision_id = '$revision_id' WHERE id = '$funnel_id';" \
  > "$state_dir/restore-published-revision.json"
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" --json \
  --command "DELETE FROM revisions WHERE id = 'pr014-unsupported-public-revision';" \
  > "$state_dir/delete-unsupported-public-revision.json"
publish_etag="$(awk 'BEGIN{IGNORECASE=1} /^ETag:/ {gsub("\r", ""); sub(/^[^:]*:[[:space:]]*/, ""); print; exit}' "$state_dir/published.headers")"
conditional_status="$(request GET "/f/$funnel_slug" "$state_dir/not-modified.body" "$state_dir/not-modified.headers" \
  -H "If-None-Match: W/$publish_etag")"
test "$conditional_status" = 304
test ! -s "$state_dir/not-modified.body"
publish_replay_status="$(request POST "/api/funnels/$funnel_id/publish" "$state_dir/publish-replay.json" "$state_dir/publish-replay.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-1' --data "$publish_payload")"
test "$publish_replay_status" = 200
duplicate_publish_used_status="$(request POST "/api/funnels/$funnel_id/publish" "$state_dir/duplicate-publish-used.json" "$state_dir/duplicate-publish-used.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-1' \
  --data "{\"revision_id\":\"$revision_id\",\"expected_published_revision_id\":null,\"expected_published_revision_id\":null}")"
assert_error_response "$state_dir/duplicate-publish-used.json" "$duplicate_publish_used_status" 409 conflict/idempotency_payload_mismatch
duplicate_publish_fresh_status="$(request POST "/api/funnels/$funnel_id/publish" "$state_dir/duplicate-publish-fresh.json" "$state_dir/duplicate-publish-fresh.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-duplicate-fresh' \
  --data "{\"revision_id\":\"$revision_id\",\"expected_published_revision_id\":null,\"expected_published_revision_id\":null}")"
assert_error_response "$state_dir/duplicate-publish-fresh.json" "$duplicate_publish_fresh_status" 400 validation/invalid_json
used_key_mismatch_status="$(request POST "/api/funnels/$funnel_id/publish" "$state_dir/used-key-mismatch.json" "$state_dir/used-key-mismatch.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-1' \
  --data "{\"revision_id\":\"$revision_2\",\"expected_published_revision_id\":\"$revision_id\"}")"
assert_error_response "$state_dir/used-key-mismatch.json" "$used_key_mismatch_status" 409 conflict/idempotency_payload_mismatch
missing_publish_revision_status="$(request POST "/api/funnels/$funnel_id/publish" "$state_dir/missing-publish-revision.json" "$state_dir/missing-publish-revision.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-missing-revision' \
  --data '{"expected_published_revision_id":null}')"
assert_error_response "$state_dir/missing-publish-revision.json" "$missing_publish_revision_status" 400 validation/missing_field
missing_publish_revision_used_key_status="$(request POST "/api/funnels/$funnel_id/publish" "$state_dir/missing-publish-revision-used-key.json" "$state_dir/missing-publish-revision-used-key.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-1' \
  --data '{"expected_published_revision_id":null}')"
assert_error_response "$state_dir/missing-publish-revision-used-key.json" "$missing_publish_revision_used_key_status" 409 conflict/idempotency_payload_mismatch
missing_unpublish_baseline_status="$(request POST "/api/funnels/$funnel_id/unpublish" "$state_dir/missing-unpublish-baseline.json" "$state_dir/missing-unpublish-baseline.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-unpublish-missing-baseline' \
  --data '{}')"
assert_error_response "$state_dir/missing-unpublish-baseline.json" "$missing_unpublish_baseline_status" 400 validation/missing_field
stale_publish_status="$(request POST "/api/funnels/$funnel_id/publish" "$state_dir/stale-publish.json" "$state_dir/stale-publish.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-stale' \
  --data "{\"revision_id\":\"$revision_2\",\"expected_published_revision_id\":null}")"
assert_error_response "$state_dir/stale-publish.json" "$stale_publish_status" 409 conflict/publish_stale_revision

publish_2_payload="{\"revision_id\":\"$revision_2\",\"expected_published_revision_id\":\"$revision_id\"}"
publish_2_status="$(request POST "/api/funnels/$funnel_id/publish" "$state_dir/publish-2.json" "$state_dir/publish-2.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-2' --data "$publish_2_payload")"
test "$publish_2_status" = 200
revision_2_status="$(request GET "/f/$funnel_slug" "$state_dir/published-v2.html" "$state_dir/published-v2.headers" \
  -H "If-None-Match: $publish_etag")"
test "$revision_2_status" = 200
revision_2_etag="$(awk 'BEGIN{IGNORECASE=1} /^ETag:/ {gsub("\r", ""); sub(/^[^:]*:[[:space:]]*/, ""); print; exit}' "$state_dir/published-v2.headers")"
python3 - "$state_dir/published-v2.html" "$revision_2_etag" "$publish_etag" <<'PY'
import sys

with open(sys.argv[1], encoding="utf-8") as html_file:
    html = html_file.read()
assert "<h1>Published revision two</h1>" in html, html
assert "--color-primary:#123456" in html, html
assert sys.argv[2] != sys.argv[3], (sys.argv[2], sys.argv[3])
PY

publish_race_a_status_file="$state_dir/publish-race-a.status"
publish_race_b_status_file="$state_dir/publish-race-b.status"
request POST "/api/funnels/$funnel_id/publish" "$state_dir/publish-race-a.json" "$state_dir/publish-race-a.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-race-a' \
  --data "{\"revision_id\":\"$revision_3\",\"expected_published_revision_id\":\"$revision_2\"}" > "$publish_race_a_status_file" &
publish_race_a_pid=$!
request POST "/api/funnels/$funnel_id/publish" "$state_dir/publish-race-b.json" "$state_dir/publish-race-b.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-race-b' \
  --data "{\"revision_id\":\"$revision_4\",\"expected_published_revision_id\":\"$revision_2\"}" > "$publish_race_b_status_file" &
publish_race_b_pid=$!
wait "$publish_race_a_pid"
wait "$publish_race_b_pid"
publish_race_a_status="$(cat "$publish_race_a_status_file")"
publish_race_b_status="$(cat "$publish_race_b_status_file")"
python3 - "$state_dir/publish-race-a.json" "$publish_race_a_status" "$state_dir/publish-race-b.json" "$publish_race_b_status" "$revision_3" "$revision_4" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as file:
    a = json.load(file)
with open(sys.argv[3], encoding="utf-8") as file:
    b = json.load(file)
assert sorted([sys.argv[2], sys.argv[4]]) == ["200", "409"], (sys.argv[2], sys.argv[4], a, b)
winner = a if sys.argv[2] == "200" else b
loser = b if winner is a else a
assert winner["published_revision_id"] in {sys.argv[5], sys.argv[6]}, winner
if loser.get("code"):
    assert loser["code"] == "conflict/publish_stale_revision", loser
PY
if [ "$publish_race_a_status" = 200 ]; then
  publish_winner_id="$revision_3"
else
  publish_winner_id="$revision_4"
fi
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" --json \
  --command "SELECT (SELECT published_revision_id FROM funnels WHERE id = '$funnel_id') AS published_revision_id, (SELECT COUNT(*) FROM command_receipts WHERE idempotency_key IN ('pr014-cross-funnel-publish','pr014-publish-stale','pr014-publish-duplicate-fresh','pr014-publish-invalid-route-fresh')) AS rejected_receipts, (SELECT COUNT(*) FROM command_receipts WHERE idempotency_key IN ('pr014-publish-race-a','pr014-publish-race-b')) AS race_receipts, (SELECT published_revision_id FROM funnels WHERE id = '$foreign_funnel_id') AS foreign_published_revision_id;" \
  > "$state_dir/publication-state-after-race.json"
python3 - "$state_dir/publication-state-after-race.json" "$publish_winner_id" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as file:
    result = json.load(file)
state = result[-1]["results"][0]
assert state == {
    "published_revision_id": sys.argv[2],
    "rejected_receipts": 0,
    "race_receipts": 1,
    "foreign_published_revision_id": None,
}, state
PY
race_published_status="$(request GET "/f/$funnel_slug" "$state_dir/published-race.html" "$state_dir/published-race.headers" \
  -H "If-None-Match: $revision_2_etag")"
test "$race_published_status" = 200
race_etag="$(awk 'BEGIN{IGNORECASE=1} /^ETag:/ {gsub("\r", ""); sub(/^[^:]*:[[:space:]]*/, ""); print; exit}' "$state_dir/published-race.headers")"
python3 - "$state_dir/published-race.html" "$revision_3" "$revision_4" "$publish_winner_id" "$race_etag" <<'PY'
import sys

with open(sys.argv[1], encoding="utf-8") as file:
    html = file.read()
winner_title = "Published race candidate three" if sys.argv[4] == sys.argv[2] else "Published race candidate four"
assert f"<h1>{winner_title}</h1>" in html, html
assert sys.argv[5] == f'"{sys.argv[4]}"', (sys.argv[5], sys.argv[4])
PY
stale_unpublish_status="$(request POST "/api/funnels/$funnel_id/unpublish" "$state_dir/stale-unpublish.json" "$state_dir/stale-unpublish.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-unpublish-stale' \
  --data "{\"expected_published_revision_id\":\"$revision_2\"}")"
assert_error_response "$state_dir/stale-unpublish.json" "$stale_unpublish_status" 409 conflict/publish_stale_revision
unpublish_payload="{\"expected_published_revision_id\":\"$publish_winner_id\"}"
unpublish_status="$(request POST "/api/funnels/$funnel_id/unpublish" "$state_dir/unpublish.json" "$state_dir/unpublish.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-unpublish-1' --data "$unpublish_payload")"
test "$unpublish_status" = 200
unpublished_status="$(request GET "/f/$funnel_slug" "$state_dir/unpublished.json" "$state_dir/unpublished.headers" \
  -H "If-None-Match: $race_etag")"
assert_error_response "$state_dir/unpublished.json" "$unpublished_status" 404 not_found/slug
unpublish_replay_status="$(request POST "/api/funnels/$funnel_id/unpublish" "$state_dir/unpublish-replay.json" "$state_dir/unpublish-replay.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-unpublish-1' --data "$unpublish_payload")"
test "$unpublish_replay_status" = 200
missing_unpublish_baseline_used_key_status="$(request POST "/api/funnels/$funnel_id/unpublish" "$state_dir/missing-unpublish-baseline-used-key.json" "$state_dir/missing-unpublish-baseline-used-key.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-unpublish-1' --data '{}')"
assert_error_response "$state_dir/missing-unpublish-baseline-used-key.json" "$missing_unpublish_baseline_used_key_status" 409 conflict/idempotency_payload_mismatch
late_publish_replay_status="$(request POST "/api/funnels/$funnel_id/publish" "$state_dir/late-publish-replay.json" "$state_dir/late-publish-replay.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-1' --data "$publish_payload")"
test "$late_publish_replay_status" = 200
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" --json \
  --command "SELECT published_revision_id FROM funnels WHERE id = '$funnel_id';" \
  > "$state_dir/publication-state-after-late-publish-replay.json"
python3 - "$state_dir/publication-state-after-late-publish-replay.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as file:
    result = json.load(file)
state = result[-1]["results"][0]
assert state == {"published_revision_id": None}, state
PY
publish_after_unpublish_payload="{\"revision_id\":\"$publish_winner_id\",\"expected_published_revision_id\":null}"
publish_after_unpublish_status="$(request POST "/api/funnels/$funnel_id/publish" "$state_dir/publish-after-unpublish.json" "$state_dir/publish-after-unpublish.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-publish-after-unpublish' --data "$publish_after_unpublish_payload")"
test "$publish_after_unpublish_status" = 200
late_unpublish_replay_status="$(request POST "/api/funnels/$funnel_id/unpublish" "$state_dir/late-unpublish-replay.json" "$state_dir/late-unpublish-replay.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr014-unpublish-1' --data "$unpublish_payload")"
test "$late_unpublish_replay_status" = 200
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" --json \
  --command "SELECT (SELECT published_revision_id FROM funnels WHERE id = '$funnel_id') AS published_revision_id, (SELECT COUNT(*) FROM command_receipts WHERE idempotency_key IN ('pr014-cross-funnel-publish','pr014-publish-stale','pr014-publish-duplicate-fresh','pr014-unpublish-stale','pr014-publish-missing-revision','pr014-unpublish-missing-baseline')) AS rejected_receipts, (SELECT COUNT(*) FROM command_receipts WHERE idempotency_key IN ('pr014-publish-1','pr014-publish-2','pr014-unpublish-1','pr014-publish-after-unpublish')) AS successful_receipts;" \
  > "$state_dir/publication-state-after-unpublish.json"
python3 - "$state_dir/publication-state-after-unpublish.json" "$publish_winner_id" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as file:
    result = json.load(file)
state = result[-1]["results"][0]
assert state == {
    "published_revision_id": sys.argv[2],
    "rejected_receipts": 0,
    "successful_receipts": 4,
}, state
PY

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

invalid_blueprint_replay_status="$(request POST /api/funnels "$state_dir/invalid-blueprint-replay.json" \
  "$state_dir/invalid-blueprint-replay.headers" -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: pr009-create-1' \
  --data '{"name":"Changed offer","blueprint":"unknown"}')"
python3 - "$state_dir/invalid-blueprint-replay.json" "$invalid_blueprint_replay_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "409", (sys.argv[2], response)
assert response["code"] == "conflict/idempotency_payload_mismatch", response
PY

invalid_blueprint_new_key_status="$(request POST /api/funnels "$state_dir/invalid-blueprint-new-key.json" \
  "$state_dir/invalid-blueprint-new-key.headers" -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: pr009-create-invalid-blueprint' \
  --data '{"name":"Changed offer","blueprint":"unknown"}')"
python3 - "$state_dir/invalid-blueprint-new-key.json" "$invalid_blueprint_new_key_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "400", (sys.argv[2], response)
assert response["code"] == "validation/invalid_json", response
PY

seed_expired_receipt pr009-create-expired-invalid-blueprint
invalid_blueprint_expired_key_status="$(request POST /api/funnels \
  "$state_dir/invalid-blueprint-expired-key.json" "$state_dir/invalid-blueprint-expired-key.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr009-create-expired-invalid-blueprint' \
  --data '{"name":"Changed offer","blueprint":"unknown"}')"
python3 - "$state_dir/invalid-blueprint-expired-key.json" "$invalid_blueprint_expired_key_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "400", (sys.argv[2], response)
assert response["code"] == "validation/invalid_json", response
PY

python3 - "$state_dir/oversized-create.bin" "$state_dir/invalid-utf8-create.bin" <<'PY'
import sys

with open(sys.argv[1], "wb") as oversized:
    oversized.write(b'{"name":"' + b'x' * 130 + b'","blueprint":"tripwire"}' + b' ' * 16384)
with open(sys.argv[2], "wb") as invalid_utf8:
    invalid_utf8.write(b'{"name":"bad\xffname","blueprint":"tripwire"}')
PY

oversized_active_status="$(request POST /api/funnels "$state_dir/oversized-active.json" \
  "$state_dir/oversized-active.headers" -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: pr009-create-1' --data-binary "@$state_dir/oversized-create.bin")"
assert_error_response "$state_dir/oversized-active.json" "$oversized_active_status" 409 \
  conflict/idempotency_payload_mismatch
oversized_fresh_status="$(request POST /api/funnels "$state_dir/oversized-fresh.json" \
  "$state_dir/oversized-fresh.headers" -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: pr009-create-oversized-fresh' --data-binary "@$state_dir/oversized-create.bin")"
assert_error_response "$state_dir/oversized-fresh.json" "$oversized_fresh_status" 400 validation/invalid_json
seed_expired_receipt pr009-create-oversized-expired
oversized_expired_status="$(request POST /api/funnels "$state_dir/oversized-expired.json" \
  "$state_dir/oversized-expired.headers" -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: pr009-create-oversized-expired' --data-binary "@$state_dir/oversized-create.bin")"
assert_error_response "$state_dir/oversized-expired.json" "$oversized_expired_status" 400 \
  validation/invalid_json

invalid_utf8_active_status="$(request POST /api/funnels "$state_dir/invalid-utf8-active.json" \
  "$state_dir/invalid-utf8-active.headers" -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: pr009-create-1' --data-binary "@$state_dir/invalid-utf8-create.bin")"
assert_error_response "$state_dir/invalid-utf8-active.json" "$invalid_utf8_active_status" 409 \
  conflict/idempotency_payload_mismatch
invalid_utf8_fresh_status="$(request POST /api/funnels "$state_dir/invalid-utf8-fresh.json" \
  "$state_dir/invalid-utf8-fresh.headers" -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: pr009-create-invalid-utf8-fresh' --data-binary "@$state_dir/invalid-utf8-create.bin")"
assert_error_response "$state_dir/invalid-utf8-fresh.json" "$invalid_utf8_fresh_status" 400 \
  validation/invalid_json
seed_expired_receipt pr009-create-invalid-utf8-expired
invalid_utf8_expired_status="$(request POST /api/funnels "$state_dir/invalid-utf8-expired.json" \
  "$state_dir/invalid-utf8-expired.headers" -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: pr009-create-invalid-utf8-expired' \
  --data-binary "@$state_dir/invalid-utf8-create.bin")"
assert_error_response "$state_dir/invalid-utf8-expired.json" "$invalid_utf8_expired_status" 400 \
  validation/invalid_json

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

v2_create_status="$(request POST /api/funnels/v2 "$state_dir/v2-create.json" "$state_dir/v2-create.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr029-v2-create' \
  --data '{"name":"Typed offer","blueprint":"lead_gen"}')"
test "$v2_create_status" = 201
v2_funnel_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["id"])' "$state_dir/v2-create.json")"
printf '%s\n' "$v2_funnel_id" >> "$funnel_ids_file"
v2_funnel_slug="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["slug"])' "$state_dir/v2-create.json")"
v2_revision_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["initial_revision"]["id"])' "$state_dir/v2-create.json")"
v2_publish_status="$(request POST "/api/funnels/$v2_funnel_id/publish" "$state_dir/v2-publish.json" "$state_dir/v2-publish.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr029-v2-publish' \
  --data "{\"revision_id\":\"$v2_revision_id\",\"expected_published_revision_id\":null}")"
test "$v2_publish_status" = 200
v2_public_status="$(request GET "/f/$v2_funnel_slug" "$state_dir/v2-published.html" "$state_dir/v2-published.headers")"
test "$v2_public_status" = 200
python3 - "$state_dir/v2-publish.json" "$state_dir/v2-published.html" "$state_dir/v2-published.headers" "$v2_revision_id" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
with open(sys.argv[2], encoding="utf-8") as html_file:
    html = html_file.read()
with open(sys.argv[3], encoding="utf-8") as headers_file:
    headers = headers_file.read().lower()
assert response["published_revision_id"] == sys.argv[4], response
assert "<h1>" in html, html
assert "etag: \"" in headers, headers
assert "cache-control: public" in headers, headers
PY
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" --json \
  --command "SELECT published_revision_id FROM funnels WHERE id = '$v2_funnel_id';" > "$state_dir/v2-publish-pointer.json"
python3 - "$state_dir/v2-publish-pointer.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as result_file:
    result = json.load(result_file)
assert result[-1]["results"][0]["published_revision_id"], result
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

invalid_delete_replay_status="$(request DELETE /api/funnels/invalid! \
  "$state_dir/invalid-delete-replay.json" "$state_dir/invalid-delete-replay.headers" \
  -H 'Idempotency-Key: pr009-delete-1')"
python3 - "$state_dir/invalid-delete-replay.json" "$invalid_delete_replay_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "409", (sys.argv[2], response)
assert response["code"] == "conflict/idempotency_payload_mismatch", response
PY

invalid_delete_new_key_status="$(request DELETE /api/funnels/invalid! \
  "$state_dir/invalid-delete-new-key.json" "$state_dir/invalid-delete-new-key.headers" \
  -H 'Idempotency-Key: pr009-delete-invalid-route')"
python3 - "$state_dir/invalid-delete-new-key.json" "$invalid_delete_new_key_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "404", (sys.argv[2], response)
assert response["code"] == "not_found/funnel", response
PY

seed_expired_receipt pr009-delete-expired-invalid-route funnel.delete
invalid_delete_expired_key_status="$(request DELETE /api/funnels/invalid! \
  "$state_dir/invalid-delete-expired-key.json" "$state_dir/invalid-delete-expired-key.headers" \
  -H 'Idempotency-Key: pr009-delete-expired-invalid-route')"
python3 - "$state_dir/invalid-delete-expired-key.json" "$invalid_delete_expired_key_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "404", (sys.argv[2], response)
assert response["code"] == "not_found/funnel", response
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

python3 - "$state_dir/fill-to-cap.sql" <<'PY'
import sys

with open(sys.argv[1], "w", encoding="utf-8") as sql_file:
    for index in range(1, 100):
        funnel_id = f"pr009-cap-seed-{index:03d}"
        sql_file.write(
            "INSERT INTO funnels (id, name, slug) VALUES "
            f"('{funnel_id}', 'Capacity seed {index}', '{funnel_id}');\n"
        )
PY
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" \
  --file "$state_dir/fill-to-cap.sql" > "$state_dir/fill-to-cap.log"
last_slot_status="$(request POST /api/funnels "$state_dir/last-slot.json" "$state_dir/last-slot.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr009-create-last-slot' \
  --data '{"name":"Last capacity slot","blueprint":"tripwire"}')"
test "$last_slot_status" = 201
last_slot_replay_status="$(request POST /api/funnels "$state_dir/last-slot-replay.json" "$state_dir/last-slot-replay.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr009-create-last-slot' \
  --data '{"name":"Last capacity slot","blueprint":"tripwire"}')"
test "$last_slot_replay_status" = 201
python3 - "$state_dir/last-slot.json" "$state_dir/last-slot-replay.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as first_file:
    first = json.load(first_file)
with open(sys.argv[2], encoding="utf-8") as replay_file:
    replay = json.load(replay_file)
assert first["funnel"] == replay["funnel"], (first, replay)
assert first["initial_revision"] == replay["initial_revision"], (first, replay)
PY
bounded_list_status="$(request GET /api/funnels "$state_dir/cap-list.json" "$state_dir/cap-list.headers")"
test "$bounded_list_status" = 200
python3 - "$state_dir/cap-list.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert len(response["items"]) == 100, len(response["items"])
PY

capacity_status="$(request POST /api/funnels "$state_dir/capacity.json" "$state_dir/capacity.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr009-create-at-cap' \
  --data '{"name":"Beyond capacity","blueprint":"tripwire"}')"
python3 - "$state_dir/capacity.json" "$capacity_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert sys.argv[2] == "409", (sys.argv[2], response)
assert response["code"] == "conflict/funnel_limit_reached", response
assert response["category"] == "conflict", response
assert response["message"] == "The workspace has reached its funnel limit.", response
PY

./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" --json \
  --command "SELECT (SELECT COUNT(*) FROM funnels) AS funnel_count, (SELECT COUNT(*) FROM command_receipts WHERE idempotency_key = 'pr009-create-at-cap') AS receipt_count;" \
  > "$state_dir/capacity-persistence.json"
python3 - "$state_dir/capacity-persistence.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as result_file:
    result = json.load(result_file)
counts = result[-1]["results"][0]
assert counts == {"funnel_count": 100, "receipt_count": 0}, counts
PY

python3 - "$state_dir/over-cap-insert.sql" <<'PY'
import sys

with open(sys.argv[1], "w", encoding="utf-8") as sql_file:
    sql_file.write(
        "INSERT INTO funnels (id, name, slug) "
        "VALUES ('pr009-cap-overflow', 'Over capacity', 'pr009-cap-overflow');\n"
    )
PY
if ./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" \
  --file "$state_dir/over-cap-insert.sql" > "$state_dir/over-cap-insert.log" 2>&1; then
  printf 'The database accepted a direct insert beyond the workspace funnel limit\n' >&2
  exit 1
fi
grep -q 'workspace funnel limit reached' "$state_dir/over-cap-insert.log"

python3 - "$state_dir/seed-list-overflow.sql" <<'PY'
import sys

with open(sys.argv[1], "w", encoding="utf-8") as sql_file:
    sql_file.write(
        "DROP TRIGGER funnels_workspace_funnel_limit;\n"
        "INSERT INTO funnels (id, name, slug) "
        "VALUES ('pr009-cap-overflow', 'Over capacity', 'pr009-cap-overflow');\n"
        "CREATE TRIGGER funnels_workspace_funnel_limit\n"
        "BEFORE INSERT ON funnels\n"
        "WHEN (SELECT COUNT(*) FROM funnels) >= 100\n"
        "BEGIN\n"
        "    SELECT RAISE(ABORT, 'workspace funnel limit reached');\n"
        "END;\n"
    )
PY
./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" \
  --file "$state_dir/seed-list-overflow.sql" > "$state_dir/seed-list-overflow.log"
overflow_list_status="$(request GET /api/funnels "$state_dir/overflow-list.json" "$state_dir/overflow-list.headers")"
test "$overflow_list_status" = 200
python3 - "$state_dir/overflow-list.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as response_file:
    response = json.load(response_file)
assert len(response["items"]) == 100, len(response["items"])
PY

./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" \
  --command "DELETE FROM funnels WHERE id = 'pr009-cap-overflow';" > /dev/null
last_slot_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["id"])' \
  "$state_dir/last-slot.json")"
free_slot_delete_status="$(request DELETE "/api/funnels/$last_slot_id" "$state_dir/free-slot-delete.json" \
  "$state_dir/free-slot-delete.headers" -H 'Idempotency-Key: pr009-delete-last-slot')"
test "$free_slot_delete_status" = 200
freed_slot_create_status="$(request POST /api/funnels "$state_dir/freed-slot.json" "$state_dir/freed-slot.headers" \
  -H 'Content-Type: application/json' -H 'Idempotency-Key: pr009-create-after-limit' \
  --data '{"name":"Freed capacity slot","blueprint":"tripwire"}')"
test "$freed_slot_create_status" = 201
freed_slot_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["funnel"]["id"])' \
  "$state_dir/freed-slot.json")"
freed_slot_delete_status="$(request DELETE "/api/funnels/$freed_slot_id" "$state_dir/freed-slot-delete.json" \
  "$state_dir/freed-slot-delete.headers" -H 'Idempotency-Key: pr009-delete-freed-slot')"
test "$freed_slot_delete_status" = 200

./node_modules/.bin/wrangler d1 execute DB --local --env dev --persist-to "$state_dir" \
  --command "DELETE FROM funnels WHERE id LIKE 'pr009-cap-seed-%' OR id = 'pr009-cap-overflow';" > /dev/null
final_empty_status="$(request GET /api/funnels "$state_dir/final-empty-list.json" "$state_dir/final-empty-list.headers")"
test "$final_empty_status" = 200
python3 - "$state_dir/final-empty-list.json" <<'PY'
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

printf 'PR-014=publish-replay,cross-funnel-rejected,cas-race-one-winner,v2-render,etag-change,weak-304,unpublish-404,late-replay-no-reapply,missing-field-classification,route-receipt-precedence,unsupported-schema-400\n'
printf 'list=200 five-blueprints=created replay=201 payload-mismatch=409 receipt-cleanup=create,delete delete=200 replay-delete=200 workspace-cap=100 list-bound=100 over-cap-list-bound=100 create-at-cap=409 direct-insert=rejected host-denial=403\n'
