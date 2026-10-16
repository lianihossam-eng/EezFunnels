#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
source .project-tools/env.sh
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

    def assert_no_store(status, headers):
        assert headers.get("cache-control") == "no-store", (status, headers)
        assert "etag" not in headers, (status, headers)

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

    def execute_d1(command, json_output=False, timeout=30):
        arguments = [
            str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
            "--persist-to", str(persist),
        ]
        if json_output:
            arguments.append("--json")
        arguments.extend(["--command", command])
        return subprocess.run(
            arguments,
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=timeout,
        ).stdout

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
            rb'action="(/api/public/funnels/[^\"]+/pages/page-home/forms/([^/]+)/submissions\?key=([0-9a-f-]{36}))"',
            html,
        )
        assert action is not None, html
        action_path = action.group(1).decode()
        form_id = action.group(2).decode()
        submission_key = action.group(3).decode()
        second_status, second_html, second_headers = request("GET", f"/f/{slug}")
        assert second_status == 200, second_html
        assert second_headers.get("cache-control") == "no-store", second_headers
        second_action = re.search(rb"submissions\?key=([0-9a-f-]{36})", second_html)
        assert second_action is not None, second_html
        assert second_action.group(1).decode() != submission_key

        payload = urlencode(
            {"name": "Ada Lovelace", "email": "ada@example.test", "_request_consent": "on"}
        ).encode()
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

        status, body, response_headers = request(
            "POST",
            action_path,
            payload,
            {"Content-Type": "application/x-www-form-urlencoded; charset=UTF-8"},
        )
        assert status == 303, (status, body)
        assert response_headers.get("location") == f"/f/{slug}#form-success-{form_id}", response_headers
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
        second_key_path = re.sub(
            r"key=[0-9a-f-]{36}$", f"key={second_action.group(1).decode()}", action_path
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
                re.sub(r"key=[0-9a-f-]{36}$", f"key={third_key}", action_path),
                payload,
                {"Content-Type": "application/x-www-form-urlencoded"},
            )
            assert third_status == 303, (third_status, third_body)

        rate_limit_key = "3da91eb0-7f65-49c1-9341-a28eb9d10c27"
        rate_status, rate_body, rate_headers = request(
            "POST",
            re.sub(r"key=[0-9a-f-]{36}$", f"key={rate_limit_key}", action_path),
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
            re.sub(rf"/forms/{re.escape(form_id)}/", "/forms/form-unknown/", action_path),
            payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert unknown_form_status == 404, (unknown_form_status, unknown_form_body)

        query = subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--json", "--command",
                "SELECT COUNT(*) AS submissions, SUM(marketing_opt_in) AS opted_in, MIN(request_consent_acknowledged) AS acknowledged, MIN(length(request_consent_copy)) AS consent_copy_length, MIN(expires_at - submitted_at) AS retention_seconds, COUNT(DISTINCT submission_key) AS unique_keys, MIN(json_extract(field_values_json, '$.email')) AS stored_email, (SELECT COUNT(*) FROM public_form_contexts) AS context_count, (SELECT context_json FROM public_form_contexts LIMIT 1) AS context_json FROM public_form_submissions",
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
        assert result["retention_seconds"] == 86_400, result
        assert result["unique_keys"] == 3, result
        assert result["stored_email"] == "ada@example.test", result
        context = json.loads(result["context_json"])
        assert result["context_count"] == 1, result
        assert context["page_title"] == "Home", context
        assert context["form_purpose"], context
        assert [field["name"] for field in context["fields"]] == ["name", "email", "interest"], context
        assert all(set(field) == {"name", "label", "kind"} for field in context["fields"]), context
        assert "ada@example.test" not in result["context_json"], context
        assert "options" not in result["context_json"], context

        seed_id_query = subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--json", "--command",
                "SELECT submission_id FROM public_form_submissions WHERE marketing_opt_in = 0 LIMIT 1",
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        ).stdout
        seed_result = find_mapping(json.loads(seed_id_query), "submission_id")
        assert seed_result is not None, seed_id_query
        seed_id = seed_result["submission_id"]
        seed_command = f"""
            WITH RECURSIVE sequence(n) AS (
                SELECT 1 UNION ALL SELECT n + 1 FROM sequence WHERE n < 102
            )
            INSERT INTO public_form_submissions (
                submission_id, funnel_id, revision_id, page_id, form_id, submission_key,
                field_values_json, request_consent_acknowledged,
                request_consent_copy_version, request_consent_copy,
                request_consent_recorded_at, marketing_opt_in,
                marketing_opt_in_copy_version, marketing_opt_in_copy,
                marketing_opt_in_recorded_at, submitted_at, expires_at
            )
            SELECT
                printf('00000000-0000-4000-8000-%012x', sequence.n), source.funnel_id,
                source.revision_id, source.page_id, source.form_id,
                printf('00000000-0000-4000-8000-%012x', sequence.n + 1000),
                source.field_values_json, source.request_consent_acknowledged,
                source.request_consent_copy_version, source.request_consent_copy,
                source.request_consent_recorded_at, source.marketing_opt_in,
                source.marketing_opt_in_copy_version, source.marketing_opt_in_copy,
                source.marketing_opt_in_recorded_at, unixepoch(), unixepoch() + 86400
            FROM sequence CROSS JOIN (
                SELECT * FROM public_form_submissions WHERE submission_id = '{seed_id}'
            ) AS source;
            UPDATE public_form_submissions SET submitted_at = unixepoch()
            WHERE funnel_id = '{funnel_id}';
        """
        subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--command", seed_command,
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )

        leads_path = f"/api/funnels/{funnel_id}/leads"
        page_status, page_body, page_headers = request(
            "GET", leads_path, headers={"If-None-Match": '"stale"'}
        )
        assert page_status == 200, (page_status, page_body)
        assert_no_store(page_status, page_headers)
        page_one = json.loads(page_body)
        leads_one = page_one["leads"]
        assert len(leads_one) == 50, page_one
        assert page_one["next_cursor"], page_one
        assert len({lead["submission_id"] for lead in leads_one}) == 50, page_one
        assert all(lead["submitted_at"] == leads_one[0]["submitted_at"] for lead in leads_one), page_one
        assert [
            (lead["submitted_at"], lead["submission_id"]) for lead in leads_one
        ] == sorted(
            [(lead["submitted_at"], lead["submission_id"]) for lead in leads_one],
            reverse=True,
        ), page_one
        page_json = page_body.decode()
        for forbidden in [
            "ada@example.test",
            "Ada Lovelace",
            "I agree to be contacted about this request.",
            "Send me occasional product news.",
            "revision_id",
            "submission_key",
            "answers",
        ]:
            assert forbidden not in page_json, (forbidden, page_json)

        page_two_status, page_two_body, page_two_headers = request(
            "GET", f"{leads_path}?cursor={page_one['next_cursor']}"
        )
        assert page_two_status == 200, (page_two_status, page_two_body)
        assert_no_store(page_two_status, page_two_headers)
        page_two = json.loads(page_two_body)
        leads_two = page_two["leads"]
        assert len(leads_two) == 50, page_two
        assert page_two["next_cursor"], page_two
        assert not ({lead["submission_id"] for lead in leads_one} & {lead["submission_id"] for lead in leads_two})
        assert all(lead["submitted_at"] == leads_one[0]["submitted_at"] for lead in leads_two), page_two

        unchecked_lead = next(
            lead for lead in leads_one + leads_two if not lead["marketing_opt_in"]
        )
        detail_id = unchecked_lead["submission_id"]
        detail_path = f"/api/funnels/{funnel_id}/leads/{detail_id}"
        detail_status, detail_body, detail_headers = request(
            "GET", detail_path, headers={"If-None-Match": '"stale"'}
        )
        assert detail_status == 200, (detail_status, detail_body)
        assert_no_store(detail_status, detail_headers)
        detail = json.loads(detail_body)
        assert [answer["name"] for answer in detail["answers"]] == ["name", "email", "interest"], detail
        assert detail["answers"][0]["value"] == "Ada Lovelace", detail
        assert detail["answers"][1]["value"] == "ada@example.test", detail
        assert detail["answers"][2]["value"] is None, detail
        assert detail["request_consent"]["acknowledged"] is True, detail
        assert detail["request_consent"]["copy"] == "I agree to be contacted about this request.", detail
        assert detail["request_consent"]["recorded_at"] > 0, detail
        assert detail["marketing_opt_in"] is False, detail
        assert detail["marketing_consent"]["copy"] == "Send me occasional product news.", detail
        assert detail["marketing_consent"]["recorded_at"] > 0, detail
        assert "revision_id" not in detail and "submission_key" not in detail, detail

        for period in (7, 30, 90):
            analytics_status, analytics_body, analytics_headers = request(
                "GET",
                f"/api/funnels/{funnel_id}/analytics/forms?days={period}",
                headers={"If-None-Match": '"stale"'},
            )
            assert analytics_status == 200, (analytics_status, analytics_body)
            assert_no_store(analytics_status, analytics_headers)
            analytics = json.loads(analytics_body)
            assert set(analytics) == {
                "period_days", "as_of", "total_submissions", "days", "forms",
                "other_form_submissions", "correlation_id",
            }, analytics
            assert re.fullmatch(r"[0-9a-f-]{36}", analytics["correlation_id"]), analytics
            assert analytics["period_days"] == period, analytics
            assert analytics["total_submissions"] == 105, analytics
            assert len(analytics["days"]) == period, analytics
            assert analytics["days"][-1]["date"] == time.strftime(
                "%Y-%m-%d", time.gmtime(analytics["as_of"])
            ), analytics
            assert all(set(day) == {"date", "submissions"} for day in analytics["days"]), analytics
            assert sum(day["submissions"] for day in analytics["days"]) == 105, analytics
            assert len(analytics["forms"]) == 1, analytics
            assert set(analytics["forms"][0]) == {"page_title", "form_purpose", "submissions"}, analytics
            assert analytics["forms"][0]["submissions"] == 105, analytics
            assert sum(form["submissions"] for form in analytics["forms"]) + analytics[
                "other_form_submissions"
            ] == 105, analytics
            for forbidden in [
                "submission_id", "revision_id", "submission_key", "field_values_json",
                "request_consent", "marketing_consent", "ada@example.test", "Ada Lovelace",
            ]:
                assert forbidden not in analytics_body.decode(), (forbidden, analytics_body)

        for invalid_query in [
            "",
            "days=07",
            "days=7&days=30",
            "days=7&unused=1",
            "days=%37",
            "days=8",
        ]:
            invalid_status, _, invalid_headers = request(
                "GET", f"/api/funnels/{funnel_id}/analytics/forms?{invalid_query}"
            )
            assert invalid_status == 400, (invalid_query, invalid_status)
            assert_no_store(invalid_status, invalid_headers)

        invalid_path_status, _, invalid_path_headers = request(
            "GET", f"/api/funnels/{funnel_id}/analytics/forms/extra?days=7"
        )
        assert invalid_path_status == 404, invalid_path_status
        assert_no_store(invalid_path_status, invalid_path_headers)
        missing_funnel_status, _, missing_funnel_headers = request(
            "GET", "/api/funnels/missing-funnel/analytics/forms?days=7"
        )
        assert missing_funnel_status == 404, missing_funnel_status
        assert_no_store(missing_funnel_status, missing_funnel_headers)
        denied_host_status, _, denied_host_headers = request(
            "GET",
            f"/api/funnels/{funnel_id}/analytics/forms?days=7",
            headers={"Host": "blocked.invalid"},
        )
        assert denied_host_status == 403, denied_host_status
        assert_no_store(denied_host_status, denied_host_headers)
        analytics_method_status, _, analytics_method_headers = request(
            "POST", f"/api/funnels/{funnel_id}/analytics/forms?days=7"
        )
        assert analytics_method_status == 405, analytics_method_status
        assert analytics_method_headers.get("allow") == "GET", analytics_method_headers
        assert_no_store(analytics_method_status, analytics_method_headers)

        if os.environ.get("PR040_BROWSER") == "1":
            subprocess.run(
                [
                    str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                    "--persist-to", str(persist), "--command",
                    f"UPDATE public_form_submissions SET field_values_json = json_set(field_values_json, '$.name', '<img src=x onerror=alert(1)>') WHERE submission_id = '{detail_id}'",
                ],
                cwd=root,
                check=True,
                capture_output=True,
                text=True,
                timeout=30,
            )
            subprocess.run(
                [
                    "node",
                    str(root / "scripts/test-pr040-lead-inbox-browser.mjs"),
                    f"http://localhost:{port}/funnels/{funnel_id}/leads",
                    detail_id,
                ],
                cwd=root,
                check=True,
                timeout=120,
            )

            subprocess.run(
                [
                    "node",
                    str(root / "scripts/test-pr041-form-analytics-browser.mjs"),
                    f"http://localhost:{port}/funnels/{funnel_id}/analytics",
                ],
                cwd=root,
                check=True,
                timeout=120,
            )

        invalid_cursor_status, invalid_cursor_body, invalid_cursor_headers = request(
            "GET", f"{leads_path}?cursor=invalid"
        )
        assert invalid_cursor_status == 400, invalid_cursor_status
        assert json.loads(invalid_cursor_body)["code"] == "validation/invalid_query", invalid_cursor_body
        assert_no_store(invalid_cursor_status, invalid_cursor_headers)
        detail_query_status, detail_query_body, detail_query_headers = request(
            "GET", f"{detail_path}?unexpected=true"
        )
        assert detail_query_status == 400, detail_query_status
        assert json.loads(detail_query_body)["code"] == "validation/invalid_query", detail_query_body
        assert_no_store(detail_query_status, detail_query_headers)
        method_status, _, method_headers = request("POST", leads_path)
        assert method_status == 405, method_status
        assert_no_store(method_status, method_headers)
        malformed_path_status, _, malformed_path_headers = request(
            "GET", f"{detail_path}/extra"
        )
        assert malformed_path_status == 404, malformed_path_status
        assert_no_store(malformed_path_status, malformed_path_headers)
        host_status, _, host_headers = request(
            "GET", leads_path, headers={"Host": "not-allowed.invalid"}
        )
        assert host_status == 403, host_status
        assert_no_store(host_status, host_headers)

        other_funnel_status, _, other_funnel_headers = request(
            "GET", f"/api/funnels/other-funnel/leads/{detail_id}"
        )
        assert other_funnel_status == 404, other_funnel_status
        assert_no_store(other_funnel_status, other_funnel_headers)

        no_marketing_id = leads_two[0]["submission_id"]
        subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--command",
                f"UPDATE public_form_submissions SET marketing_opt_in = 0, marketing_opt_in_copy_version = NULL, marketing_opt_in_copy = NULL, marketing_opt_in_recorded_at = NULL WHERE submission_id = '{no_marketing_id}'",
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        no_marketing_status, no_marketing_body, no_marketing_headers = request(
            "GET", f"/api/funnels/{funnel_id}/leads/{no_marketing_id}"
        )
        assert no_marketing_status == 200, (no_marketing_status, no_marketing_body)
        assert_no_store(no_marketing_status, no_marketing_headers)
        no_marketing_detail = json.loads(no_marketing_body)
        assert no_marketing_detail["marketing_opt_in"] is False, no_marketing_detail
        assert no_marketing_detail["marketing_consent"] is None, no_marketing_detail

        expired_id = leads_two[1]["submission_id"]
        subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--command",
                f"UPDATE public_form_submissions SET submitted_at = unixepoch() - 10, expires_at = unixepoch() - 1 WHERE submission_id = '{expired_id}'",
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        expired_status, _, expired_headers = request(
            "GET", f"/api/funnels/{funnel_id}/leads/{expired_id}"
        )
        assert expired_status == 404, expired_status
        assert_no_store(expired_status, expired_headers)
        next_page_status, next_page_body, next_page_headers = request(
            "GET", f"{leads_path}?cursor={page_one['next_cursor']}"
        )
        assert next_page_status == 200, (next_page_status, next_page_body)
        assert_no_store(next_page_status, next_page_headers)
        next_page = json.loads(next_page_body)
        assert expired_id not in {lead["submission_id"] for lead in next_page["leads"]}, next_page

        recent_id, older_id, oldest_id = [lead["submission_id"] for lead in leads_two[2:5]]
        execute_d1(
            f"UPDATE public_form_submissions SET submitted_at = unixepoch() - 86400, expires_at = unixepoch() + 86400 WHERE submission_id = '{recent_id}'; "
            f"UPDATE public_form_submissions SET submitted_at = unixepoch() - 8 * 86400, expires_at = unixepoch() + 86400 WHERE submission_id = '{older_id}'; "
            f"UPDATE public_form_submissions SET submitted_at = unixepoch() - 31 * 86400, expires_at = unixepoch() + 86400 WHERE submission_id = '{oldest_id}'"
        )
        for period, expected_total in ((7, 102), (30, 103), (90, 104)):
            range_status, range_body, range_headers = request(
                "GET", f"/api/funnels/{funnel_id}/analytics/forms?days={period}"
            )
            assert range_status == 200, (range_status, range_body)
            assert_no_store(range_status, range_headers)
            range_report = json.loads(range_body)
            assert range_report["total_submissions"] == expected_total, range_report
            assert len(range_report["days"]) == period, range_report
            assert sum(day["submissions"] for day in range_report["days"]) == expected_total, range_report

        repository_source = (root / "crates/storage/src/repository.rs").read_text(encoding="utf-8")
        analytics_sql = re.search(
            r'const PUBLIC_FORM_COMPLETION_ANALYTICS_SQL: &str = r#"(.*?)"#;',
            repository_source,
            re.DOTALL,
        ).group(1)
        plan_bindings = iter(["90", f"'{funnel_id}'"])
        query_plan = execute_d1(
            "EXPLAIN QUERY PLAN " + re.sub(r"\?", lambda _match: next(plan_bindings), analytics_sql),
            json_output=True,
        )
        assert "public_form_submissions_rate_idx" in query_plan, query_plan

        exact_limit_seed = f"""
            WITH RECURSIVE sequence(n) AS (
                SELECT 1 UNION ALL SELECT n + 1 FROM sequence WHERE n < 9895
            )
            INSERT INTO public_form_submissions (
                submission_id, funnel_id, revision_id, page_id, form_id, submission_key,
                field_values_json, request_consent_acknowledged,
                request_consent_copy_version, request_consent_copy,
                request_consent_recorded_at, marketing_opt_in,
                marketing_opt_in_copy_version, marketing_opt_in_copy,
                marketing_opt_in_recorded_at, submitted_at, expires_at
            )
            SELECT
                printf('00000000-0000-4000-8000-%012x', sequence.n + 30000), source.funnel_id,
                source.revision_id, source.page_id, source.form_id,
                printf('00000000-0000-4000-8000-%012x', sequence.n + 40000),
                source.field_values_json, source.request_consent_acknowledged,
                source.request_consent_copy_version, source.request_consent_copy,
                source.request_consent_recorded_at, source.marketing_opt_in,
                source.marketing_opt_in_copy_version, source.marketing_opt_in_copy,
                source.marketing_opt_in_recorded_at, unixepoch() - 20, unixepoch() - 10
            FROM sequence CROSS JOIN (
                SELECT * FROM public_form_submissions WHERE submission_id = '{seed_id}'
            ) AS source
        """
        execute_d1(exact_limit_seed, timeout=60)
        exact_limit_status, exact_limit_body, exact_limit_headers = request(
            "GET", f"/api/funnels/{funnel_id}/analytics/forms?days=90"
        )
        assert exact_limit_status == 200, (exact_limit_status, exact_limit_body)
        assert_no_store(exact_limit_status, exact_limit_headers)
        assert json.loads(exact_limit_body)["total_submissions"] == 104, exact_limit_body

        execute_d1(
            f"INSERT INTO public_form_submissions SELECT '00000000-0000-4000-8000-000000030000', funnel_id, revision_id, page_id, form_id, '00000000-0000-4000-8000-000000040000', field_values_json, request_consent_acknowledged, request_consent_copy_version, request_consent_copy, request_consent_recorded_at, marketing_opt_in, marketing_opt_in_copy_version, marketing_opt_in_copy, marketing_opt_in_recorded_at, unixepoch() - 20, unixepoch() - 10 FROM public_form_submissions WHERE submission_id = '{seed_id}'"
        )
        over_limit_status, over_limit_body, over_limit_headers = request(
            "GET", f"/api/funnels/{funnel_id}/analytics/forms?days=90"
        )
        assert over_limit_status == 422, (over_limit_status, over_limit_body)
        assert json.loads(over_limit_body)["code"] == "analytics/report_too_large", over_limit_body
        assert "total_submissions" not in over_limit_body.decode(), over_limit_body
        assert_no_store(over_limit_status, over_limit_headers)

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
                "SELECT COUNT(*) AS submissions, (SELECT COUNT(*) FROM public_form_contexts) AS contexts FROM public_form_submissions",
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
        assert cleanup_result["contexts"] == 0, cleanup_result

        log.close()
        assert "ada@example.test" not in log_path.read_text(encoding="utf-8"), log_path.read_text(encoding="utf-8")
        print("PR-033/039/041 local Worker/D1 form, lead, analytics bounds, retention, and privacy flow passed")
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
