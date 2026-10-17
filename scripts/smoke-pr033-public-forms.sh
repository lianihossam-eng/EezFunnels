#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-/tmp/eezfunnel-xdg}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-/tmp/eezfunnel-cache}"
export NPM_CONFIG_CACHE="${NPM_CONFIG_CACHE:-/tmp/eezfunnel-npm-cache}"
export WRANGLER_SEND_METRICS=false

python3 - "$repo_root" <<'PY'
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlencode

root = Path(sys.argv[1])
wrangler = root / "node_modules" / ".bin" / "wrangler"
with socket.socket() as probe:
    probe.bind(("127.0.0.1", 0))
    port = probe.getsockname()[1]

with tempfile.TemporaryDirectory(prefix="eezfunnel-pr033-forms-") as directory:
    state = Path(directory)
    log_path = state / "wrangler.log"
    persist = state / "persist"
    subprocess.run(
        [
            str(wrangler), "d1", "migrations", "apply", "DB", "--local", "--env", "dev",
            "--persist-to", str(persist),
        ],
        cwd=root,
        check=True,
        timeout=120,
        stdout=subprocess.DEVNULL,
    )
    log = log_path.open("w", encoding="utf-8")
    worker = subprocess.Popen(
        [
            str(wrangler), "dev", "--local", "--env", "dev", "--port", str(port),
            "--ip", "127.0.0.1", "--var", f"ALLOWED_HOSTS:localhost:{port}",
            "--var", "PUBLIC_FORM_SUBMISSIONS_PER_MINUTE:3",
            "--var", "PUBLIC_FORM_RETENTION_DAYS:1",
            "--persist-to", str(persist), "--test-scheduled", "--log-level", "error",
        ],
        cwd=root,
        stdout=log,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )

    def request(method, path, body=None, headers=None):
        connection = http.client.HTTPConnection("127.0.0.1", port, timeout=20)
        request_headers = {"Host": f"localhost:{port}"}
        request_headers.update(headers or {})
        connection.request(method, path, body=body, headers=request_headers)
        response = connection.getresponse()
        content = response.read()
        status = response.status
        response_headers = {key.lower(): value for key, value in response.getheaders()}
        connection.close()
        return status, content, response_headers

    def wait_for_worker():
        deadline = time.monotonic() + 240
        while time.monotonic() < deadline:
            if worker.poll() is not None:
                raise RuntimeError("Local Worker exited during startup")
            try:
                status, _, _ = request("GET", "/health")
                if status == 200:
                    return
            except OSError:
                pass
            time.sleep(0.5)
        raise RuntimeError("Local Worker did not start")

    def post_json(path, value, key):
        return request(
            "POST",
            path,
            json.dumps(value, separators=(",", ":")).encode(),
            {"Content-Type": "application/json", "Idempotency-Key": key},
        )

    def mint_stage(funnel_id, revision_id, data, key):
        status, content, _ = post_json(
            f"/api/funnels/{funnel_id}/stages-v2",
            {
                "base_revision_id": revision_id,
                "expected_length": len(data),
                "expected_sha256": hashlib.sha256(data).hexdigest(),
            },
            key,
        )
        assert status == 201, (status, content)
        return json.loads(content)["stage_id"]

    def upload(funnel_id, stage_id, data):
        return request(
            "PUT",
            f"/api/funnels/{funnel_id}/stages-v2",
            data,
            {"Authorization": f"Bearer {stage_id}", "Content-Type": "application/octet-stream"},
        )

    def find_mapping(value, key):
        if isinstance(value, dict):
            if key in value:
                return value
            for child in value.values():
                result = find_mapping(child, key)
                if result is not None:
                    return result
        elif isinstance(value, list):
            for child in value:
                result = find_mapping(child, key)
                if result is not None:
                    return result
        return None

    def active_group_members(process_group):
        members = []
        for entry in Path("/proc").iterdir():
            if not entry.name.isdigit():
                continue
            try:
                stat = (entry / "stat").read_text(encoding="utf-8")
                fields = stat[stat.rfind(")") + 2 :].split()
                process_state = fields[0]
                process_group_id = int(fields[2])
            except (OSError, ValueError, IndexError):
                continue
            if process_group_id == process_group and process_state not in {"Z", "X"}:
                members.append(int(entry.name))
        return members

    def port_is_closed():
        with socket.socket() as probe:
            try:
                probe.bind(("127.0.0.1", port))
            except OSError:
                return False
            return True

    try:
        wait_for_worker()
        status, content, _ = post_json(
            "/api/funnels/v2",
            {"name": "PR-033 local form smoke", "blueprint": "lead_gen"},
            "pr033-create-funnel",
        )
        assert status == 201, (status, content)
        created = json.loads(content)
        funnel_id = created["funnel"]["id"]
        slug = created["funnel"]["slug"]
        base_revision_id = created["initial_revision"]["id"]

        document = json.loads(
            (root / "crates/domain/tests/fixtures/document_v2_minimal.json").read_bytes()
        )
        document["funnel_id"] = funnel_id
        document["pages"][0]["sections"][0]["blocks"].append(
            {"id": "block-form", "kind": "form", "form_id": "form-lead"}
        )
        document["forms"] = [
            {
                "id": "form-lead",
                "fields": [
                    {
                        "id": "field-name", "kind": "text", "name": "name",
                        "label": "Name", "required": True,
                        "validation": {"min_length": 2, "max_length": 40},
                    },
                    {
                        "id": "field-email", "kind": "email", "name": "email",
                        "label": "Email", "required": True,
                    },
                ],
                "purpose": "Contact request",
                "request_consent": {
                    "copy_version": "request-v1",
                    "copy": "I agree to be contacted about this request.",
                },
                "marketing_opt_in": {
                    "copy_version": "marketing-v1",
                    "copy": "Send me occasional product news.",
                },
                "result": {"kind": "inline", "message": "Thanks for getting in touch."},
            }
        ]
        data = json.dumps(document, separators=(",", ":")).encode()
        stage_id = mint_stage(funnel_id, base_revision_id, data, "pr033-form-stage")
        assert upload(funnel_id, stage_id, data)[0] == 204
        status, content, _ = post_json(
            f"/api/funnels/{funnel_id}/revisions-v2",
            {
                "base_revision_id": base_revision_id,
                "stage_id": stage_id,
                "expected_length": len(data),
                "expected_sha256": hashlib.sha256(data).hexdigest(),
            },
            "pr033-form-save",
        )
        assert status == 201, (status, content)
        saved_revision_id = json.loads(content)["revision_id"]
        status, content, _ = request(
            "POST",
            f"/api/funnels/{funnel_id}/publish",
            json.dumps(
                {
                    "revision_id": saved_revision_id,
                    "expected_published_revision_id": None,
                },
                separators=(",", ":"),
            ).encode(),
            {"Content-Type": "application/json", "Idempotency-Key": "pr033-form-publish"},
        )
        assert status == 200, (status, content)

        status, html, headers = request("GET", f"/f/{slug}")
        assert status == 200, html
        assert headers.get("cache-control") == "no-store", headers
        assert "etag" not in headers, headers
        assert b"type=\"email\"" in html, html
        assert b"request-v1" not in html, html
        action = re.search(
            rb'action="(/api/public/funnels/[^\"]+/pages/page-home/forms/form-lead/submissions\?key=([0-9a-f-]{36})&revision_id=([A-Za-z0-9_-]{1,64}))"',
            html,
        )
        assert action is not None, html
        action_path = action.group(1).decode()
        submission_key = action.group(2).decode()
        assert action.group(3).decode() == saved_revision_id, action.group(3)
        second_status, second_html, second_headers = request("GET", f"/f/{slug}")
        assert second_status == 200, second_html
        assert second_headers.get("cache-control") == "no-store", second_headers
        second_action = re.search(rb"submissions\?key=([0-9a-f-]{36})", second_html)
        assert second_action is not None, second_html
        assert second_action.group(1).decode() != submission_key

        payload = urlencode(
            {"name": "Ada Lovelace", "email": "ada@example.test", "_request_consent": "on"}
        ).encode()

        document["forms"][0]["request_consent"]["copy_version"] = "request-v2"
        document["forms"][0]["request_consent"]["copy"] = (
            "I agree to be contacted about this updated request."
        )
        updated_data = json.dumps(document, separators=(",", ":")).encode()
        updated_stage_id = mint_stage(
            funnel_id, saved_revision_id, updated_data, "pr033-form-stage-v2"
        )
        assert upload(funnel_id, updated_stage_id, updated_data)[0] == 204
        status, content, _ = post_json(
            f"/api/funnels/{funnel_id}/revisions-v2",
            {
                "base_revision_id": saved_revision_id,
                "stage_id": updated_stage_id,
                "expected_length": len(updated_data),
                "expected_sha256": hashlib.sha256(updated_data).hexdigest(),
            },
            "pr033-form-save-v2",
        )
        assert status == 201, (status, content)
        updated_revision_id = json.loads(content)["revision_id"]
        status, content, _ = request(
            "POST",
            f"/api/funnels/{funnel_id}/publish",
            json.dumps(
                {
                    "revision_id": updated_revision_id,
                    "expected_published_revision_id": saved_revision_id,
                },
                separators=(",", ":"),
            ).encode(),
            {"Content-Type": "application/json", "Idempotency-Key": "pr033-form-publish-v2"},
        )
        assert status == 200, (status, content)

        stale_status, stale_body, _ = request(
            "POST",
            action_path,
            payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert stale_status == 409, (stale_status, stale_body)
        assert b"conflict/public_form_stale_revision" in stale_body

        latest_status, latest_html, latest_headers = request("GET", f"/f/{slug}")
        assert latest_status == 200, latest_html
        assert latest_headers.get("cache-control") == "no-store", latest_headers
        latest_action = re.search(
            rb'action="(/api/public/funnels/[^\"]+/pages/page-home/forms/form-lead/submissions\?key=([0-9a-f-]{36})&revision_id=([A-Za-z0-9_-]{1,64}))"',
            latest_html,
        )
        assert latest_action is not None, latest_html
        action_path = latest_action.group(1).decode()
        submission_key = latest_action.group(2).decode()
        assert latest_action.group(3).decode() == updated_revision_id, latest_action.group(3)
        second_page_status, second_page_html, second_page_headers = request(
            "GET", f"/f/{slug}"
        )
        assert second_page_status == 200, second_page_html
        assert second_page_headers.get("cache-control") == "no-store", second_page_headers
        second_action = re.search(
            rb'submissions\?key=([0-9a-f-]{36})&revision_id=([A-Za-z0-9_-]{1,64})',
            second_page_html,
        )
        assert second_action is not None, second_page_html
        assert second_action.group(1).decode() != submission_key
        assert second_action.group(2).decode() == updated_revision_id

        invalid_payload = urlencode(
            {"name": "Ada Lovelace", "email": "ada@example.test"}
        ).encode()
        invalid_status, invalid_body, _ = request(
            "POST",
            action_path,
            invalid_payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert invalid_status == 400, (invalid_status, invalid_body)
        assert b"ada@example.test" not in invalid_body

        cross_origin_status, cross_origin_body, _ = request(
            "POST",
            action_path,
            bytes([0xFF]),
            {
                "Content-Type": "application/x-www-form-urlencoded",
                "Origin": "https://attacker.example",
            },
        )
        assert cross_origin_status == 403, (cross_origin_status, cross_origin_body)
        assert b"request/invalid_origin" in cross_origin_body

        status, body, response_headers = request(
            "POST",
            action_path,
            payload,
            {
                "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
                "Origin": f"http://localhost:{port}",
            },
        )
        assert status == 303, (status, body)
        assert response_headers.get("location") == f"/f/{slug}#form-success-form-lead", response_headers
        assert response_headers.get("cache-control") == "no-store", response_headers
        assert b"ada@example.test" not in body

        duplicate_status, duplicate_body, _ = request(
            "POST",
            action_path,
            payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert duplicate_status == 303, (duplicate_status, duplicate_body)

        marketing_payload = urlencode(
            {
                "name": "Ada Lovelace", "email": "ada@example.test",
                "_request_consent": "on", "_marketing_opt_in": "on",
            }
        ).encode()
        reused_key_status, reused_key_body, _ = request(
            "POST",
            action_path,
            marketing_payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert reused_key_status == 409, (reused_key_status, reused_key_body)
        assert b"conflict/idempotency_payload_mismatch" in reused_key_body

        second_key_path = re.sub(
            r"(?<=key=)[0-9a-f-]{36}", second_action.group(1).decode(), action_path
        )
        marketing_status, marketing_body, _ = request(
            "POST",
            second_key_path,
            marketing_payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert marketing_status == 303, (marketing_status, marketing_body)

        third_key = "8f5c8ca6-49b6-4e3d-a9a1-f680f8d78ac2"
        if os.environ.get("PR033_BROWSER") == "1":
            subprocess.run(
                [
                    "node",
                    str(root / "scripts/test-pr033-public-forms-browser.mjs"),
                    f"http://localhost:{port}/f/{slug}",
                ],
                cwd=root,
                env={key: value for key, value in os.environ.items() if key != "XDG_CACHE_HOME"},
                check=True,
                timeout=90,
            )
        else:
            third_status, third_body, _ = request(
                "POST",
                re.sub(r"(?<=key=)[0-9a-f-]{36}", third_key, action_path),
                payload,
                {"Content-Type": "application/x-www-form-urlencoded"},
            )
            assert third_status == 303, (third_status, third_body)

        rate_limit_key = "3da91eb0-7f65-49c1-9341-a28eb9d10c27"
        rate_status, rate_body, rate_headers = request(
            "POST",
            re.sub(r"(?<=key=)[0-9a-f-]{36}", rate_limit_key, action_path),
            payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert rate_status == 429, (rate_status, rate_body)
        assert rate_headers.get("retry-after") == "60", rate_headers

        read_status, read_body, _ = request(
            "GET", action_path.split("?", 1)[0]
        )
        assert read_status == 405, (read_status, read_body)
        unknown_form_status, unknown_form_body, _ = request(
            "POST",
            re.sub(r"/forms/form-lead/", "/forms/form-unknown/", action_path),
            payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert unknown_form_status == 404, (unknown_form_status, unknown_form_body)

        query = subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--json", "--command",
                "SELECT COUNT(*) AS submissions, SUM(marketing_opt_in) AS opted_in, MIN(request_consent_acknowledged) AS acknowledged, MIN(length(request_consent_copy)) AS consent_copy_length, MIN(request_consent_copy_version) AS consent_copy_version, MIN(expires_at - submitted_at) AS retention_seconds, COUNT(DISTINCT submission_key) AS unique_keys, MIN(json_extract(field_values_json, '$.email')) AS stored_email FROM public_form_submissions",
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        ).stdout
        result = find_mapping(json.loads(query), "submissions")
        assert result is not None, query
        assert result["submissions"] == 3, result
        assert result["opted_in"] == 1, result
        assert result["acknowledged"] == 1, result
        assert result["consent_copy_length"] > 0, result
        assert result["consent_copy_version"] == "request-v2", result
        assert result["retention_seconds"] == 86_400, result
        assert result["unique_keys"] == 3, result
        assert result["stored_email"] == "ada@example.test", result

        subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--command",
                "UPDATE public_form_submissions SET submitted_at = unixepoch() - 86401, expires_at = unixepoch() - 1",
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        cleanup_status, cleanup_body, _ = request(
            "GET", "/__scheduled?cron=%2A+%2F15+*+*+*+*"
        )
        assert cleanup_status == 200, cleanup_body
        cleanup_query = subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--json", "--command",
                "SELECT COUNT(*) AS submissions FROM public_form_submissions",
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        ).stdout
        cleanup_result = find_mapping(json.loads(cleanup_query), "submissions")
        assert cleanup_result is not None, cleanup_query
        assert cleanup_result["submissions"] == 0, cleanup_result

        log.close()
        assert "ada@example.test" not in log_path.read_text(encoding="utf-8"), log_path.read_text(encoding="utf-8")
        print("PR-033 local Worker/D1 public form, consent, idempotency, retention, and rate-limit flow passed")
    except Exception:
        if log_path.exists():
            print(log_path.read_text(encoding="utf-8"), file=sys.stderr)
        raise
    finally:
        if worker.poll() is None:
            try:
                os.killpg(worker.pid, signal.SIGINT)
                worker.wait(timeout=10)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                try:
                    os.killpg(worker.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                worker.wait(timeout=5)
        if not log.closed:
            log.close()
        active_members = active_group_members(worker.pid)
        if active_members:
            try:
                os.killpg(worker.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            deadline = time.monotonic() + 5
            while active_group_members(worker.pid) and time.monotonic() < deadline:
                time.sleep(0.1)
            active_members = active_group_members(worker.pid)
            if active_members:
                try:
                    os.killpg(worker.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                deadline = time.monotonic() + 5
                while active_group_members(worker.pid) and time.monotonic() < deadline:
                    time.sleep(0.1)
                active_members = active_group_members(worker.pid)
        clean_shutdown_status = worker.returncode in (0, -signal.SIGINT, 130, 143)
        if not clean_shutdown_status or active_members or not port_is_closed():
            raise RuntimeError(f"Local Worker exited with status {worker.returncode}")
PY
