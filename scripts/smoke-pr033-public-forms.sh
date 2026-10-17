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

    def request_form_with_transport_retry(path, body, headers=None):
        if "key=" not in path:
            raise ValueError("form transport retries require the submission idempotency key")
        for attempt in range(3):
            result = request("POST", path, body, headers)
            status, content, _ = result
            if status != 500 or b"Network connection lost" not in content:
                return result
            if attempt == 2:
                return result
            print(
                "PR-033 retrying the same idempotent form submission after a transient "
                "local Worker transport failure",
                flush=True,
            )
            time.sleep(0.25 * (attempt + 1))
        raise AssertionError("bounded form transport retry exhausted unexpectedly")

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
            probe.settimeout(1)
            return probe.connect_ex(("127.0.0.1", port)) != 0

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
        document["forms"] = []
        data = json.dumps(document, separators=(",", ":")).encode()
        stage_id = mint_stage(funnel_id, base_revision_id, data, "pr034-form-base-stage")
        assert upload(funnel_id, stage_id, data)[0] == 204
        status, content, _ = post_json(
            f"/api/funnels/{funnel_id}/revisions-v2",
            {
                "base_revision_id": base_revision_id,
                "stage_id": stage_id,
                "expected_length": len(data),
                "expected_sha256": hashlib.sha256(data).hexdigest(),
            },
            "pr034-form-base-save",
        )
        assert status == 201, (status, content)
        saved_base_revision_id = json.loads(content)["revision_id"]
        current_document = [data.decode()]

        def edit_document(operation_id, edit):
            edit_request = {
                "base_revision_id": saved_base_revision_id,
                "page_id": "page-home",
                "document_json": current_document[0],
                "operation_id": operation_id,
                "edit": edit,
            }
            edit_status, edit_content, _ = request(
                "POST",
                f"/api/funnels/{funnel_id}/edit-v2",
                json.dumps(edit_request, separators=(",", ":")).encode(),
                {"Content-Type": "application/json"},
            )
            assert edit_status == 200, (operation_id, edit_status, edit_content)
            current_document[0] = json.loads(edit_content)["document_json"]
            return json.loads(current_document[0])

        document = edit_document(
            "a" * 32,
            {
                "operation": "create_form",
                "section": "section-hero",
                "after": "block-copy",
            },
        )
        form = document["forms"][0]
        form_id = form["id"]
        name_field, email_field = form["fields"]
        edit_document(
            "b" * 32,
            {
                "operation": "set_form_field",
                "form": form_id,
                "field": name_field["id"],
                "value": {
                    "kind": "text",
                    "name": "name",
                    "label": "Name",
                    "required": True,
                    "validation": {"min_length": 2, "max_length": 40},
                },
            },
        )
        edit_document(
            "c" * 32,
            {
                "operation": "set_form_field",
                "form": form_id,
                "field": email_field["id"],
                "value": {
                    "kind": "email",
                    "name": "email",
                    "label": "Email",
                    "required": True,
                },
            },
        )
        edit_document(
            "d" * 32,
            {
                "operation": "add_form_field",
                "form": form_id,
                "after": email_field["id"],
                "field": {
                    "kind": "select",
                    "name": "interest",
                    "label": "Topic",
                    "help": "Choose one topic.",
                    "required": False,
                    "options": ["Consulting", "Training"],
                },
            },
        )
        edit_document(
            "e" * 32,
            {
                "operation": "set_form_request_consent",
                "form": form_id,
                "copy": "I agree to be contacted about this request.",
            },
        )
        edit_document(
            "f" * 32,
            {
                "operation": "set_form_marketing_opt_in",
                "form": form_id,
                "copy": "Send me occasional product news.",
            },
        )

        edited_data = current_document[0].encode()
        stage_id = mint_stage(
            funnel_id,
            saved_base_revision_id,
            edited_data,
            "pr034-form-ready-stage",
        )
        assert upload(funnel_id, stage_id, edited_data)[0] == 204
        status, content, _ = post_json(
            f"/api/funnels/{funnel_id}/revisions-v2",
            {
                "base_revision_id": saved_base_revision_id,
                "stage_id": stage_id,
                "expected_length": len(edited_data),
                "expected_sha256": hashlib.sha256(edited_data).hexdigest(),
            },
            "pr034-form-ready-save",
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
        assert b"Consulting" in html and b"Training" in html, html
        action = re.search(
            rb'action="(/api/public/funnels/[^\"]+/pages/page-home/forms/'
            + re.escape(form_id.encode())
            + rb'/submissions\?key=([0-9a-f-]{36})&revision_id=([A-Za-z0-9_-]{1,64}))"',
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

        document = json.loads(current_document[0])
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

        stale_status, stale_body, _ = request_form_with_transport_retry(
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
            rb'action="(/api/public/funnels/[^\"]+/pages/page-home/forms/'
            + re.escape(form_id.encode())
            + rb'/submissions\?key=([0-9a-f-]{36})&revision_id=([A-Za-z0-9_-]{1,64}))"',
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
        invalid_status, invalid_body, _ = request_form_with_transport_retry(
            action_path,
            invalid_payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert invalid_status == 400, (invalid_status, invalid_body)
        assert b"ada@example.test" not in invalid_body

        cross_origin_status, cross_origin_body, _ = request_form_with_transport_retry(
            action_path,
            bytes([0xFF]),
            {
                "Content-Type": "application/x-www-form-urlencoded",
                "Origin": "https://attacker.example",
            },
        )
        assert cross_origin_status == 403, (cross_origin_status, cross_origin_body)
        assert b"request/invalid_origin" in cross_origin_body

        status, body, response_headers = request_form_with_transport_retry(
            action_path,
            payload,
            {
                "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
                "Origin": f"http://localhost:{port}",
            },
        )
        assert status == 303, (status, body)
        assert response_headers.get("location") == f"/f/{slug}#form-success-{form_id}", response_headers
        assert response_headers.get("cache-control") == "no-store", response_headers
        assert b"ada@example.test" not in body

        duplicate_status, duplicate_body, _ = request_form_with_transport_retry(
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
        reused_key_status, reused_key_body, _ = request_form_with_transport_retry(
            action_path,
            marketing_payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert reused_key_status == 409, (reused_key_status, reused_key_body)
        assert b"conflict/idempotency_payload_mismatch" in reused_key_body

        second_key_path = re.sub(
            r"(?<=key=)[0-9a-f-]{36}", second_action.group(1).decode(), action_path
        )
        marketing_status, marketing_body, _ = request_form_with_transport_retry(
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
            third_status, third_body, _ = request_form_with_transport_retry(
                re.sub(r"(?<=key=)[0-9a-f-]{36}", third_key, action_path),
                payload,
                {"Content-Type": "application/x-www-form-urlencoded"},
            )
            assert third_status == 303, (third_status, third_body)

        rate_limit_key = "3da91eb0-7f65-49c1-9341-a28eb9d10c27"
        rate_status, rate_body, rate_headers = request_form_with_transport_retry(
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
        unknown_form_status, unknown_form_body, _ = request_form_with_transport_retry(
            re.sub(rf"/forms/{re.escape(form_id)}/", "/forms/form-unknown/", action_path),
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

        inbox_status, inbox_body, inbox_headers = request("GET", "/api/submissions")
        assert inbox_status == 200, inbox_body
        assert inbox_headers.get("cache-control") == "private, no-store", inbox_headers
        inbox = json.loads(inbox_body)
        assert len(inbox["items"]) == 3, inbox
        assert inbox["next_cursor"] is None, inbox
        assert b"submission_key" not in inbox_body and b"request_fingerprint" not in inbox_body
        assert inbox["items"][0]["request_consent_acknowledged"] is True, inbox
        assert inbox["items"][0]["request_consent_copy_version"] == "request-v2", inbox

        invalid_inbox_status, invalid_inbox_body, invalid_inbox_headers = request(
            "GET", "/api/submissions?limit=10"
        )
        assert invalid_inbox_status == 400, invalid_inbox_status
        assert json.loads(invalid_inbox_body)["code"] == "validation/invalid_cursor", invalid_inbox_body
        assert invalid_inbox_headers.get("cache-control") == "private, no-store", invalid_inbox_headers
        method_status, _, method_headers = request("POST", "/api/submissions")
        assert method_status == 405, method_status
        assert method_headers.get("cache-control") == "private, no-store", method_headers

        def sql_quote(value):
            return "'" + value.replace("'", "''") + "'"

        fixture_sql = f"""
            WITH RECURSIVE fixtures(n) AS (
                SELECT 1 UNION ALL SELECT n + 1 FROM fixtures WHERE n < 26
            )
            INSERT INTO public_form_submissions (
                submission_id, funnel_id, revision_id, page_id, form_id,
                submission_key, request_fingerprint, field_values_json,
                request_consent_acknowledged, request_consent_copy_version,
                request_consent_copy, request_consent_recorded_at, marketing_opt_in,
                marketing_opt_in_copy_version, marketing_opt_in_copy,
                marketing_opt_in_recorded_at, submitted_at, expires_at
            )
            SELECT
                printf('90000000-0000-4000-8000-%012x', fixtures.n), funnels.id,
                revisions.id, 'page-home', {sql_quote(form_id)},
                printf('a0000000-0000-4000-8000-%012x', fixtures.n),
                lower(hex(randomblob(32))),
                json_object('name', printf('Inbox fixture %02d', fixtures.n),
                            'email', printf('inbox-%02d@example.test', fixtures.n)),
                1, 'request-v2',
                'I agree to be contacted about this updated request.',
                unixepoch() - 100, 0, NULL, NULL, NULL,
                unixepoch() - 100, unixepoch() + 86400
            FROM fixtures
            JOIN funnels ON funnels.id = {sql_quote(funnel_id)}
            JOIN revisions ON revisions.id = {sql_quote(updated_revision_id)}
                AND revisions.funnel_id = funnels.id
            WHERE revisions.schema_version = 2
        """
        subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--command", fixture_sql,
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )

        subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--command",
                "UPDATE public_form_submissions SET submitted_at = unixepoch(date('now', '-1 day')) + 43200, expires_at = unixepoch() + 86400 WHERE submission_id = '90000000-0000-4000-8000-000000000001'; UPDATE public_form_submissions SET submitted_at = unixepoch() - 300, expires_at = unixepoch() WHERE submission_id = '90000000-0000-4000-8000-000000000002'",
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )

        completion_status, completion_body, completion_headers = request(
            "GET", f"/api/funnels/{funnel_id}/completions?period=7"
        )
        assert completion_status == 200, completion_body
        assert completion_headers.get("cache-control") == "private, no-store", completion_headers
        completion_report = json.loads(completion_body)
        assert completion_report["funnel_id"] == funnel_id, completion_report
        assert completion_report["period_days"] == 7, completion_report
        assert completion_report["total_completions"] == 28, completion_report
        assert len(completion_report["days"]) == 7, completion_report
        assert completion_report["days"][-1]["is_partial_today"] is True, completion_report
        assert all(not day["is_partial_today"] for day in completion_report["days"][:-1]), completion_report
        assert [day["utc_day"] for day in completion_report["days"]] == sorted(
            day["utc_day"] for day in completion_report["days"]
        )
        nonzero_days = [day for day in completion_report["days"] if day["completions"]]
        assert [day["completions"] for day in nonzero_days] == [1, 27], nonzero_days
        assert len(completion_report["forms"]) == 1, completion_report
        assert completion_report["forms"][0] == {
            "form_id": form_id,
            "completions": 28,
        }, {"report": completion_report, "expected_form_id": form_id}
        assert set(completion_report) == {
            "command_id", "correlation_id", "funnel_id", "period_days",
            "total_completions", "days", "forms"
        }, completion_report
        assert b"ada@example.test" not in completion_body
        assert b"request_consent_copy" not in completion_body
        assert b"submission_id" not in completion_body

        for period_days in (30, 90):
            period_status, period_body, period_headers = request(
                "GET", f"/api/funnels/{funnel_id}/completions?period={period_days}"
            )
            assert period_status == 200, period_body
            assert period_headers.get("cache-control") == "private, no-store", period_headers
            period_report = json.loads(period_body)
            assert len(period_report["days"]) == period_days, period_report
            assert period_report["total_completions"] == 28, period_report

        for invalid_query in (
            "",
            "period=1",
            "period=07",
            "period=7&period=30",
            "period=30&other=value",
        ):
            invalid_status, invalid_body, invalid_headers = request(
                "GET", f"/api/funnels/{funnel_id}/completions?{invalid_query}"
            )
            assert invalid_status == 400, (invalid_query, invalid_status, invalid_body)
            assert invalid_headers.get("cache-control") == "private, no-store", invalid_headers

        completion_method_status, _, completion_method_headers = request(
            "POST", f"/api/funnels/{funnel_id}/completions?period=7"
        )
        assert completion_method_status == 405, completion_method_status
        assert completion_method_headers.get("cache-control") == "private, no-store", completion_method_headers

        missing_funnel_status, _, missing_funnel_headers = request(
            "GET", "/api/funnels/missing-funnel/completions?period=7"
        )
        assert missing_funnel_status == 404, missing_funnel_status
        assert missing_funnel_headers.get("cache-control") == "private, no-store", missing_funnel_headers

        host_denied_status, _, host_denied_headers = request(
            "GET",
            f"/api/funnels/{funnel_id}/completions?period=7",
            headers={"Host": "not-allowed.example"},
        )
        assert host_denied_status == 403, host_denied_status
        assert host_denied_headers.get("cache-control") == "private, no-store", host_denied_headers

        isolation_status, isolation_body, _ = post_json(
            "/api/funnels/v2",
            {"name": "PR-051 report isolation", "blueprint": "lead_gen"},
            "pr051-create-isolation-funnel",
        )
        assert isolation_status == 201, (isolation_status, isolation_body)
        isolated_funnel_id = json.loads(isolation_body)["funnel"]["id"]
        isolated_status, isolated_body, isolated_headers = request(
            "GET", f"/api/funnels/{isolated_funnel_id}/completions?period=7"
        )
        assert isolated_status == 200, isolated_body
        assert isolated_headers.get("cache-control") == "private, no-store", isolated_headers
        isolated_report = json.loads(isolated_body)
        assert isolated_report["total_completions"] == 0, isolated_report
        assert len(isolated_report["days"]) == 7, isolated_report
        assert all(day["completions"] == 0 for day in isolated_report["days"])
        assert isolated_report["forms"] == []

        first_page_status, first_page_body, first_page_headers = request("GET", "/api/submissions")
        assert first_page_status == 200, first_page_body
        assert first_page_headers.get("cache-control") == "private, no-store", first_page_headers
        first_page = json.loads(first_page_body)
        assert len(first_page["items"]) == 25, first_page
        assert first_page["next_cursor"], first_page
        second_page_status, second_page_body, second_page_headers = request(
            "GET", "/api/submissions?cursor=" + first_page["next_cursor"]
        )
        assert second_page_status == 200, second_page_body
        assert second_page_headers.get("cache-control") == "private, no-store", second_page_headers
        second_page = json.loads(second_page_body)
        assert len(second_page["items"]) == 3, second_page
        assert second_page["next_cursor"] is None, second_page
        assert second_page["items"][-1]["fields"][0]["value"] == "Inbox fixture 01", second_page

        if os.environ.get("PR033_BROWSER") == "1":
            subprocess.run(
                [
                    "node",
                    str(root / "scripts/test-pr051-form-completions-browser.mjs"),
                    f"http://localhost:{port}/funnels/{funnel_id}/completions",
                    form_id,
                    f"http://localhost:{port}/funnels/{isolated_funnel_id}/completions",
                ],
                cwd=root,
                env={key: value for key, value in os.environ.items() if key != "XDG_CACHE_HOME"},
                check=True,
                timeout=90,
            )
            subprocess.run(
                [
                    "node",
                    str(root / "scripts/test-pr050-submissions-browser.mjs"),
                    f"http://localhost:{port}/submissions",
                ],
                cwd=root,
                env={key: value for key, value in os.environ.items() if key != "XDG_CACHE_HOME"},
                check=True,
                timeout=90,
            )

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
        port_closed = port_is_closed()
        port_deadline = time.monotonic() + 5
        while not port_closed and time.monotonic() < port_deadline:
            time.sleep(0.1)
            port_closed = port_is_closed()
        clean_shutdown_status = worker.returncode in (0, -signal.SIGINT, 130, 143)
        if not clean_shutdown_status or active_members or not port_closed:
            print(
                f"worker-cleanup status={worker.returncode} active-members={active_members} "
                f"port={port} port-closed={port_closed}",
                file=sys.stderr,
            )
            raise RuntimeError(f"Local Worker exited with status {worker.returncode}")
PY
