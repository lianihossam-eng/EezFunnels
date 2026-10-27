#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-/tmp/eezfunnel-xdg}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-/tmp/eezfunnel-cache}"
export NPM_CONFIG_CACHE="${NPM_CONFIG_CACHE:-/tmp/eezfunnel-npm-cache}"
export WRANGLER_SEND_METRICS=false

python3 "$repo_root/scripts/test-pr057-migration.py"
python3 "$repo_root/scripts/test-pr067-interaction-migration.py"
python3 "$repo_root/scripts/test-pr069-interaction-report-query.py"

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
import secrets
import uuid
import xml.etree.ElementTree as ElementTree
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
    interaction_env_file = state / "interaction.env"
    interaction_env_file.write_text(
        f"INTERACTION_REPLAY_KEY={secrets.token_hex(32)}\n", encoding="utf-8"
    )
    interaction_env_file.chmod(0o600)
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
            "--var", "PUBLIC_BASE_URL:https://funnels.example",
            "--var", "PUBLIC_FORM_SUBMISSIONS_PER_MINUTE:3",
            "--var", "PUBLIC_FORM_RETENTION_DAYS:1",
            "--var", "PUBLIC_INTERACTION_RETENTION_DAYS:1",
            "--var", "PUBLIC_INTERACTION_EVENTS_PER_MINUTE:10",
            "--env-file", str(interaction_env_file),
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

    def d1_query(sql, key):
        output = subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--json", "--command", sql,
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        ).stdout
        result = find_mapping(json.loads(output), key)
        assert result is not None, output
        return result

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
        status, empty_sitemap, sitemap_headers = request("GET", "/sitemap.xml")
        assert status == 200, (status, empty_sitemap)
        assert sitemap_headers.get("content-type") == "application/xml; charset=utf-8", sitemap_headers
        assert ElementTree.fromstring(empty_sitemap).findall(
            "{http://www.sitemaps.org/schemas/sitemap/0.9}url"
        ) == []
        status, robots, robots_headers = request("GET", "/robots.txt")
        assert status == 200, (status, robots)
        assert robots_headers.get("content-type") == "text/plain; charset=utf-8", robots_headers
        assert b"Disallow: /api/" in robots and b"Disallow: /mcp" in robots, robots
        assert b"Allow: /sitemap.xml" in robots, robots
        assert b"Allow: /f/" in robots and b"Allow: /assets/" in robots, robots
        assert b"Sitemap: https://funnels.example/sitemap.xml" in robots, robots
        assert request("HEAD", "/sitemap.xml")[0] == 200
        assert request("POST", "/robots.txt")[0] == 405

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

        status, unpublished_content, _ = post_json(
            "/api/funnels/v2",
            {"name": "PR-059 unpublished sitemap smoke", "blueprint": "lead_gen"},
            "pr059-create-unpublished-v2",
        )
        assert status == 201, (status, unpublished_content)
        unpublished_slug = json.loads(unpublished_content)["funnel"]["slug"]

        status, legacy_content, _ = post_json(
            "/api/funnels",
            {"name": "PR-059 legacy sitemap smoke", "blueprint": "lead_gen"},
            "pr059-create-published-v1",
        )
        assert status == 201, (status, legacy_content)
        legacy_created = json.loads(legacy_content)
        legacy_slug = legacy_created["funnel"]["slug"]
        status, legacy_publish_content, _ = request(
            "POST",
            f"/api/funnels/{legacy_created['funnel']['id']}/publish",
            json.dumps(
                {
                    "revision_id": legacy_created["initial_revision"]["id"],
                    "expected_published_revision_id": None,
                },
                separators=(",", ":"),
            ).encode(),
            {"Content-Type": "application/json", "Idempotency-Key": "pr059-publish-v1"},
        )
        assert status == 200, (status, legacy_publish_content)
        legacy_page_status, legacy_page_html, _ = request("GET", f"/f/{legacy_slug}")
        assert legacy_page_status == 200, legacy_page_html
        assert b'data-legacy-page-id="legacy-entry"' in legacy_page_html, legacy_page_html
        legacy_event = {
            "schema_version": 1,
            "event_id": str(uuid.uuid4()),
            "kind": "page_viewed",
            "observed_at_unix_seconds": 1,
            "funnel_id": legacy_created["funnel"]["id"],
            "revision_id": legacy_created["initial_revision"]["id"],
            "page_id": "legacy-entry",
        }
        legacy_event_status, legacy_event_body, _ = request(
            "POST",
            f"/api/public/funnels/{legacy_slug}/interaction-events",
            json.dumps(legacy_event, separators=(",", ":")).encode(),
            {
                "Content-Type": "application/json",
                "Origin": f"http://localhost:{port}",
                "X-EezFunnel-Interaction-Consent": "granted",
            },
        )
        assert legacy_event_status == 204, (legacy_event_status, legacy_event_body)
        assert d1_query(
            "SELECT COUNT(*) AS events FROM public_interaction_events "
            f"WHERE funnel_id = '{legacy_created['funnel']['id']}'",
            "events",
        )["events"] == 1

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

        def edit_document(
            operation_id, edit, page_id="page-home", base_revision_id=saved_base_revision_id
        ):
            edit_request = {
                "base_revision_id": base_revision_id,
                "page_id": page_id,
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
            "1" * 32,
            {
                "operation": "add_page",
                "from": "page-home",
                "title": "Thank you",
                "slug": "thank-you",
            },
        )
        thank_you_page_id = document["pages"][1]["id"]
        document = edit_document(
            "2" * 32,
            {
                "operation": "add_page",
                "from": thank_you_page_id,
                "title": "Internal step",
                "slug": "internal-step",
            },
            thank_you_page_id,
        )
        internal_page_id = document["pages"][2]["id"]
        edit_document(
            "3" * 32,
            {
                "operation": "set_page_no_index",
                "page": internal_page_id,
                "noindex": True,
            },
            internal_page_id,
        )

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

        status, sitemap, _ = request(
            "GET", "/sitemap.xml", headers={"X-Forwarded-Host": "attacker.example"}
        )
        assert status == 200, sitemap
        sitemap_urls = [
            item.text
            for item in ElementTree.fromstring(sitemap).findall(
                "{http://www.sitemaps.org/schemas/sitemap/0.9}url/"
                "{http://www.sitemaps.org/schemas/sitemap/0.9}loc"
            )
        ]
        assert f"https://funnels.example/f/{legacy_slug}" in sitemap_urls, sitemap_urls
        assert f"https://funnels.example/f/{slug}" in sitemap_urls, sitemap_urls
        assert f"https://funnels.example/f/{slug}/thank-you" in sitemap_urls, sitemap_urls
        assert not any("internal-step" in url for url in sitemap_urls), sitemap_urls
        assert not any(unpublished_slug in url for url in sitemap_urls), sitemap_urls
        assert all("attacker.example" not in url for url in sitemap_urls), sitemap_urls
        assert len(sitemap_urls) == 3, sitemap_urls

        noindex_status, noindex_html, _ = request("GET", f"/f/{slug}/internal-step")
        assert noindex_status == 200, noindex_html
        assert b'<meta name="robots" content="noindex">' in noindex_html, noindex_html

        status, html, headers = request(
            "GET", f"/f/{slug}?utm_medium=organic&utm_campaign=campaign-fixture"
        )
        assert status == 200, html
        assert headers.get("cache-control") == "no-store", headers
        assert "etag" not in headers, headers
        assert b"type=\"email\"" in html, html
        assert b"request-v1" not in html, html
        assert b"Consulting" in html and b"Training" in html, html
        action = re.search(
            rb'action="(/api/public/funnels/[^\"]+/pages/page-home/forms/'
            + re.escape(form_id.encode())
            + rb'/submissions\?key=([0-9a-f-]{36})&revision_id=([A-Za-z0-9_-]{1,64})&channel=(unattributed|search|social|email|paid|partner|other))"',
            html,
        )
        assert action is not None, html
        assert action.group(4) == b"search", action.group(4)
        assert b"campaign-fixture" not in html and b"utm_medium" not in html
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

        document = edit_document(
            "4" * 32,
            {
                "operation": "add_page",
                "from": internal_page_id,
                "title": "Draft only",
                "slug": "draft-only",
            },
            internal_page_id,
            saved_revision_id,
        )
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
        status, draft_sitemap, _ = request("GET", "/sitemap.xml")
        assert status == 200, draft_sitemap
        assert b"draft-only" not in draft_sitemap, draft_sitemap
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
        status, published_sitemap, _ = request("GET", "/sitemap.xml")
        assert status == 200, published_sitemap
        assert b"draft-only" in published_sitemap, published_sitemap

        stale_status, stale_body, _ = request_form_with_transport_retry(
            action_path,
            payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert stale_status == 409, (stale_status, stale_body)
        assert b"conflict/public_form_stale_revision" in stale_body

        latest_status, latest_html, latest_headers = request(
            "GET", f"/f/{slug}?utm_medium=social&utm_campaign=campaign-fixture"
        )
        assert latest_status == 200, latest_html
        assert latest_headers.get("cache-control") == "no-store", latest_headers
        assert b"/_eez/interaction.js" in latest_html, latest_html
        assert b'data-eezfunnel-page-id="page-home"' in latest_html, latest_html
        assert b'data-eezfunnel-form-id="' + form_id.encode() + b'"' in latest_html, latest_html
        latest_action = re.search(
            rb'action="(/api/public/funnels/[^\"]+/pages/page-home/forms/'
            + re.escape(form_id.encode())
            + rb'/submissions\?key=([0-9a-f-]{36})&revision_id=([A-Za-z0-9_-]{1,64})&channel=(unattributed|search|social|email|paid|partner|other))"',
            latest_html,
        )
        assert latest_action is not None, latest_html
        assert latest_action.group(4) == b"social", latest_action.group(4)
        assert b"campaign-fixture" not in latest_html and b"utm_medium" not in latest_html
        action_path = latest_action.group(1).decode()
        submission_key = latest_action.group(2).decode()
        assert latest_action.group(3).decode() == updated_revision_id, latest_action.group(3)
        second_page_status, second_page_html, second_page_headers = request(
            "GET", f"/f/{slug}"
        )
        assert second_page_status == 200, second_page_html
        assert second_page_headers.get("cache-control") == "no-store", second_page_headers
        second_action = re.search(
            rb'submissions\?key=([0-9a-f-]{36})&revision_id=([A-Za-z0-9_-]{1,64})&channel=(unattributed|search|social|email|paid|partner|other)',
            second_page_html,
        )
        assert second_action is not None, second_page_html
        assert second_action.group(1).decode() != submission_key
        assert second_action.group(2).decode() == updated_revision_id
        assert second_action.group(3) == b"unattributed", second_action.group(3)

        interaction_path = f"/api/public/funnels/{slug}/interaction-events"
        unconsented_event = {
            "schema_version": 1,
            "event_id": str(uuid.uuid4()),
            "kind": "page_viewed",
            "observed_at_unix_seconds": 1,
            "funnel_id": funnel_id,
            "revision_id": updated_revision_id,
            "page_id": "page-home",
            "campaign_id": "campaign-fixture",
        }
        unconsented_body = json.dumps(unconsented_event, separators=(",", ":")).encode()
        interaction_headers = {
            "Content-Type": "application/json",
            "Origin": f"http://localhost:{port}",
        }
        assert request("POST", interaction_path, unconsented_body, interaction_headers)[0] == 204
        refused_headers = {
            **interaction_headers,
            "X-EezFunnel-Interaction-Consent": "refused",
        }
        assert request("POST", interaction_path, b"not-json", refused_headers)[0] == 204
        assert request("GET", interaction_path, headers=interaction_headers)[0] == 405
        assert d1_query(
            "SELECT COUNT(*) AS events FROM public_interaction_events "
            f"WHERE funnel_id = '{funnel_id}'",
            "events",
        )["events"] == 0

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

        absent_form_page_path = (
            f"/api/public/funnels/{slug}/pages/{internal_page_id}/forms/{form_id}/submissions?"
            + urlencode(
                {
                    "key": str(uuid.uuid4()),
                    "revision_id": updated_revision_id,
                    "channel": "unattributed",
                }
            )
        )
        absent_form_page_status, absent_form_page_body, _ = request_form_with_transport_retry(
            absent_form_page_path,
            payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert absent_form_page_status == 404, (
            absent_form_page_status,
            absent_form_page_body,
        )
        assert b"not_found/slug" in absent_form_page_body

        duplicate_status, duplicate_body, _ = request_form_with_transport_retry(
            action_path,
            payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert duplicate_status == 303, (duplicate_status, duplicate_body)
        changed_channel_path = action_path.replace("&channel=social", "&channel=other")
        changed_channel_status, changed_channel_body, _ = request_form_with_transport_retry(
            changed_channel_path,
            payload,
            {"Content-Type": "application/x-www-form-urlencoded"},
        )
        assert changed_channel_status == 409, (changed_channel_status, changed_channel_body)
        assert b"conflict/idempotency_payload_mismatch" in changed_channel_body

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
            r"(?<=key=)[0-9a-f-]{36}",
            second_action.group(1).decode(),
            action_path.replace("&channel=social", "&channel=unattributed"),
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
                    f"http://localhost:{port}/f/{slug}?utm_medium=social&utm_campaign=campaign-fixture",
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

        interaction_headers_granted = {
            "Content-Type": "application/json",
            "Origin": f"http://localhost:{port}",
            "X-EezFunnel-Interaction-Consent": "granted",
        }

        invalid_event_status, invalid_event_body, _ = request(
            "POST", interaction_path, b"not-json", interaction_headers_granted
        )
        assert invalid_event_status == 400, (invalid_event_status, invalid_event_body)

        def make_interaction_event(event_id, *, revision_id=updated_revision_id, page_id="page-home", kind="page_viewed"):
            result = {
                "schema_version": 1,
                "event_id": event_id,
                "kind": kind,
                "observed_at_unix_seconds": 1,
                "funnel_id": funnel_id,
                "revision_id": revision_id,
                "page_id": page_id,
            }
            if kind in ("form_viewed", "form_started"):
                result["form_id"] = form_id
            return result

        explicit_event_id = str(uuid.uuid4())
        explicit_event = make_interaction_event(explicit_event_id)
        explicit_event["campaign_id"] = "unregistered-campaign"
        explicit_body = json.dumps(explicit_event, separators=(",", ":")).encode()
        accepted_status, accepted_body, _ = request(
            "POST", interaction_path, explicit_body, interaction_headers_granted
        )
        assert accepted_status == 204, (accepted_status, accepted_body)
        for kind in ("form_viewed", "form_started"):
            interaction_event = make_interaction_event(str(uuid.uuid4()), kind=kind)
            event_status, event_body, _ = request(
                "POST",
                interaction_path,
                json.dumps(interaction_event, separators=(",", ":")).encode(),
                interaction_headers_granted,
            )
            assert event_status == 204, (kind, event_status, event_body)
        event_summary = d1_query(
            "SELECT COUNT(*) AS events, SUM(event_kind = 'page_viewed') AS page_views, "
            "SUM(event_kind = 'form_viewed') AS form_views, "
            "SUM(event_kind = 'form_started') AS form_starts, "
            "SUM(campaign_id IS NOT NULL) AS attributed, "
            "MIN(length(replay_fingerprint)) AS fingerprint_length, "
            "MIN(expires_at - received_at) AS retention_seconds, "
            "COUNT(DISTINCT event_id) AS distinct_ids FROM public_interaction_events "
            f"WHERE funnel_id = '{funnel_id}'",
            "events",
        )
        browser_events = 3 if os.environ.get("PR033_BROWSER") == "1" else 0
        assert event_summary["events"] >= 3 + browser_events, event_summary
        assert event_summary["page_views"] >= 1 + (browser_events > 0), event_summary
        assert event_summary["form_views"] >= 1 + (browser_events > 0), event_summary
        assert event_summary["form_starts"] >= 1 + (browser_events > 0), event_summary
        assert event_summary["attributed"] == 0, event_summary
        assert event_summary["fingerprint_length"] == 64, event_summary
        assert event_summary["retention_seconds"] == 86_400, event_summary
        assert event_summary["distinct_ids"] == event_summary["events"], event_summary

        interaction_report_path = f"/api/funnels/{funnel_id}/interaction-report"
        interaction_report_status, interaction_report_body, interaction_report_headers = request(
            "GET", f"{interaction_report_path}?period=7"
        )
        assert interaction_report_status == 200, interaction_report_body
        assert interaction_report_headers.get("cache-control") == "private, no-store", interaction_report_headers
        interaction_report = json.loads(interaction_report_body)
        assert interaction_report["command_id"] == "interaction.report.read", interaction_report
        assert interaction_report["funnel_id"] == funnel_id, interaction_report
        assert interaction_report["period_days"] == 7, interaction_report
        assert interaction_report["total_events"] == event_summary["events"], interaction_report
        assert len(interaction_report["days"]) == 7, interaction_report
        assert interaction_report["days"][-1]["is_partial_today"] is True, interaction_report
        assert all(not day["is_partial_today"] for day in interaction_report["days"][:-1]), interaction_report
        assert sum(
            day["page_viewed"] + day["form_viewed"] + day["form_started"]
            for day in interaction_report["days"]
        ) == event_summary["events"], interaction_report
        assert interaction_report["campaigns"] == [
            {"campaign_id": None, "events": event_summary["events"]}
        ], interaction_report
        for private_value in (
            "event_id",
            "replay_fingerprint",
            "received_at",
            "visitor_id",
            "session_id",
            "form_id",
            "email",
            "unregistered-campaign",
        ):
            assert private_value.encode() not in interaction_report_body, private_value
        assert set(interaction_report) == {
            "command_id", "correlation_id", "funnel_id", "period_days",
            "total_events", "days", "campaigns"
        }, interaction_report

        for period_days in (30, 90):
            period_status, period_body, period_headers = request(
                "GET", f"{interaction_report_path}?period={period_days}"
            )
            assert period_status == 200, period_body
            assert period_headers.get("cache-control") == "private, no-store", period_headers
            period_report = json.loads(period_body)
            assert len(period_report["days"]) == period_days, period_report
            assert period_report["total_events"] == event_summary["events"], period_report

        for invalid_query in (
            "",
            "period=1",
            "period=07",
            "period=7&period=30",
            "period=30&other=value",
        ):
            invalid_status, _, invalid_headers = request(
                "GET", f"{interaction_report_path}?{invalid_query}"
            )
            assert invalid_status == 400, (invalid_query, invalid_status)
            assert invalid_headers.get("cache-control") == "private, no-store", invalid_headers

        method_status, _, method_headers = request(
            "POST", f"{interaction_report_path}?period=7"
        )
        assert method_status == 405, method_status
        assert method_headers.get("cache-control") == "private, no-store", method_headers
        missing_status, _, missing_headers = request(
            "GET", "/api/funnels/missing-funnel/interaction-report?period=7"
        )
        assert missing_status == 404, missing_status
        assert missing_headers.get("cache-control") == "private, no-store", missing_headers
        denied_status, _, denied_headers = request(
            "GET",
            f"{interaction_report_path}?period=7",
            headers={"Host": "not-allowed.example"},
        )
        assert denied_status == 403, denied_status
        assert denied_headers.get("cache-control") == "private, no-store", denied_headers
        public_host_status, _, public_host_headers = request(
            "GET",
            f"{interaction_report_path}?period=7",
            headers={"Host": "funnels.example"},
        )
        assert public_host_status == 403, public_host_status
        assert public_host_headers.get("cache-control") == "private, no-store", public_host_headers

        replay_state_before = d1_query(
            "SELECT COUNT(*) AS events, MIN(expires_at) AS expires_at "
            f"FROM public_interaction_events WHERE event_id = '{explicit_event_id}'",
            "events",
        )
        duplicate_status, duplicate_body, _ = request(
            "POST", interaction_path, explicit_body, interaction_headers_granted
        )
        assert duplicate_status == 204, (duplicate_status, duplicate_body)
        replay_state_after = d1_query(
            "SELECT COUNT(*) AS events, MIN(expires_at) AS expires_at "
            f"FROM public_interaction_events WHERE event_id = '{explicit_event_id}'",
            "events",
        )
        assert replay_state_after == replay_state_before, (replay_state_before, replay_state_after)

        conflicting_event = {**explicit_event, "observed_at_unix_seconds": 2}
        conflict_status, _, _ = request(
            "POST",
            interaction_path,
            json.dumps(conflicting_event, separators=(",", ":")).encode(),
            interaction_headers_granted,
        )
        assert conflict_status == 409, conflict_status

        stale_event = make_interaction_event(str(uuid.uuid4()), revision_id=saved_revision_id)
        stale_event_status, _, _ = request(
            "POST",
            interaction_path,
            json.dumps(stale_event, separators=(",", ":")).encode(),
            interaction_headers_granted,
        )
        assert stale_event_status == 404, stale_event_status
        missing_page_event = make_interaction_event(
            str(uuid.uuid4()), page_id="page-missing"
        )
        missing_page_status, _, _ = request(
            "POST",
            interaction_path,
            json.dumps(missing_page_event, separators=(",", ":")).encode(),
            interaction_headers_granted,
        )
        assert missing_page_status == 404, missing_page_status
        cross_origin_event = make_interaction_event(str(uuid.uuid4()))
        cross_origin_status, _, _ = request(
            "POST",
            interaction_path,
            json.dumps(cross_origin_event, separators=(",", ":")).encode(),
            {**interaction_headers_granted, "Origin": "https://attacker.example"},
        )
        assert cross_origin_status == 403, cross_origin_status
        oversized_status, _, _ = request(
            "POST", interaction_path, b"x" * 2049, interaction_headers_granted
        )
        assert oversized_status == 400, oversized_status

        event_count = d1_query(
            "SELECT COUNT(*) AS events FROM public_interaction_events "
            f"WHERE funnel_id = '{funnel_id}' AND received_at >= unixepoch() - 60",
            "events",
        )["events"]
        assert event_count <= 10, event_count
        while event_count < 10:
            next_event = make_interaction_event(str(uuid.uuid4()))
            next_status, next_body, _ = request(
                "POST",
                interaction_path,
                json.dumps(next_event, separators=(",", ":")).encode(),
                interaction_headers_granted,
            )
            assert next_status == 204, (next_status, next_body)
            event_count += 1
        limited_event = make_interaction_event(str(uuid.uuid4()))
        limited_status, _, limited_headers = request(
            "POST",
            interaction_path,
            json.dumps(limited_event, separators=(",", ":")).encode(),
            interaction_headers_granted,
        )
        assert limited_status == 429, limited_status
        assert limited_headers.get("retry-after") == "60", limited_headers

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
                "SELECT COUNT(*) AS submissions, SUM(marketing_opt_in) AS opted_in, MIN(request_consent_acknowledged) AS acknowledged, MIN(length(request_consent_copy)) AS consent_copy_length, MIN(request_consent_copy_version) AS consent_copy_version, MIN(expires_at - submitted_at) AS retention_seconds, COUNT(DISTINCT submission_key) AS unique_keys, MIN(json_extract(field_values_json, '$.email')) AS stored_email, SUM(submission_channel = 'social') AS social_submissions, SUM(submission_channel = 'unattributed') AS unattributed_submissions FROM public_form_submissions",
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
        assert result["social_submissions"] == 2, result
        assert result["unattributed_submissions"] == 1, result

        inbox_status, inbox_body, inbox_headers = request("GET", "/api/submissions")
        assert inbox_status == 200, inbox_body
        assert inbox_headers.get("cache-control") == "private, no-store", inbox_headers
        inbox = json.loads(inbox_body)
        assert len(inbox["items"]) == 3, inbox
        assert inbox["next_cursor"] is None, inbox
        assert b"submission_key" not in inbox_body and b"request_fingerprint" not in inbox_body
        assert inbox["items"][0]["request_consent_acknowledged"] is True, inbox
        assert inbox["items"][0]["request_consent_copy_version"] == "request-v2", inbox
        assert all(item["status"] == "new" for item in inbox["items"]), inbox

        triage_id = inbox["items"][0]["submission_id"]
        status_path = f"/api/submissions/{triage_id}/status"
        status_payload = json.dumps({"status": "contacted"}, separators=(",", ":")).encode()
        status_headers = {"Content-Type": "application/json"}
        update_status, update_body, update_headers = request(
            "POST", status_path, status_payload, status_headers
        )
        assert update_status == 200, update_body
        assert update_headers.get("cache-control") == "private, no-store", update_headers
        assert json.loads(update_body)["status"] == "contacted", update_body
        repeated_status, repeated_body, _ = request(
            "POST", status_path, status_payload, status_headers
        )
        assert repeated_status == 200, repeated_body
        assert json.loads(repeated_body)["status"] == "contacted", repeated_body

        contacted_status, contacted_body, contacted_headers = request(
            "GET", "/api/submissions?status=contacted"
        )
        assert contacted_status == 200, contacted_body
        assert contacted_headers.get("cache-control") == "private, no-store", contacted_headers
        contacted = json.loads(contacted_body)
        assert [item["submission_id"] for item in contacted["items"]] == [triage_id], contacted
        assert contacted["items"][0]["status"] == "contacted", contacted
        original_submission = next(item for item in inbox["items"] if item["submission_id"] == triage_id)
        for retained_key in (
            "fields",
            "request_consent_acknowledged",
            "request_consent_copy_version",
            "request_consent_copy",
            "request_consent_recorded_at_label",
            "marketing_opt_in",
            "marketing_opt_in_copy_version",
            "marketing_opt_in_copy",
            "marketing_opt_in_recorded_at_label",
        ):
            assert contacted["items"][0][retained_key] == original_submission[retained_key], (
                retained_key, contacted["items"][0], original_submission
            )
        new_status, new_body, _ = request("GET", "/api/submissions?status=new")
        assert new_status == 200, new_body
        assert len(json.loads(new_body)["items"]) == 2, new_body

        for invalid_query in (
            "status=unknown",
            "status=new&status=closed",
            "status=new&other=value",
            "funnel_id=",
            "funnel_id=not-a-uuid",
            f"funnel_id={funnel_id}&funnel_id={funnel_id}",
        ):
            invalid_status, invalid_body, invalid_headers = request(
                "GET", f"/api/submissions?{invalid_query}"
            )
            assert invalid_status == 400, (invalid_query, invalid_status, invalid_body)
            assert invalid_headers.get("cache-control") == "private, no-store", invalid_headers

        wrong_content_status, _, wrong_content_headers = request(
            "POST", status_path, b'{"status":"closed"}', {"Content-Type": "text/plain"}
        )
        assert wrong_content_status == 400, wrong_content_status
        assert wrong_content_headers.get("cache-control") == "private, no-store", wrong_content_headers
        invalid_status, _, _ = request(
            "POST",
            status_path,
            b'{"status":"closed","status":"new"}',
            status_headers,
        )
        assert invalid_status == 400, invalid_status
        unknown_status, _, unknown_headers = request(
            "POST",
            "/api/submissions/11111111-1111-4111-8111-111111111111/status",
            b'{"status":"closed"}',
            status_headers,
        )
        assert unknown_status == 404, unknown_status
        assert unknown_headers.get("cache-control") == "private, no-store", unknown_headers
        status_method, _, status_method_headers = request("GET", status_path)
        assert status_method == 405, status_method
        assert status_method_headers.get("cache-control") == "private, no-store", status_method_headers
        status_host_denied, _, status_host_denied_headers = request(
            "POST", status_path, status_payload,
            {"Host": "not-allowed.example", "Content-Type": "application/json"},
        )
        assert status_host_denied == 403, status_host_denied
        assert status_host_denied_headers.get("cache-control") == "private, no-store", status_host_denied_headers

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
                "UPDATE public_form_submissions SET submitted_at = unixepoch() - 300, expires_at = unixepoch() WHERE submission_id = '90000000-0000-4000-8000-000000000002'",
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )

        # Anchor every retained row to fully elapsed UTC days. The smoke can
        # cross midnight while its browser flow runs, so using unixepoch()-100
        # here would make the daily bucket assertion depend on wall-clock timing.
        historical_report_fixtures_sql = f"""
            UPDATE public_form_submissions
            SET submitted_at = CASE
                WHEN submission_id = '90000000-0000-4000-8000-000000000001'
                    THEN unixepoch(date('now', '-2 days')) + 43200
                ELSE unixepoch(date('now', '-1 day')) + 43200
            END
            WHERE funnel_id = {sql_quote(funnel_id)}
                AND expires_at > unixepoch()
        """
        subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--command", historical_report_fixtures_sql,
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
        assert {
            item["channel"]: item["completions"]
            for item in completion_report["channels"]
        } == {"social": 2, "unattributed": 26}, completion_report
        assert all(
            item["label"] in {"Social", "Unattributed"}
            for item in completion_report["channels"]
        ), completion_report
        assert set(completion_report) == {
            "command_id", "correlation_id", "funnel_id", "period_days",
            "total_completions", "days", "forms", "channels"
        }, completion_report
        assert b"ada@example.test" not in completion_body
        assert b"campaign-fixture" not in completion_body
        assert b"utm_medium" not in completion_body
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
        assert isolated_report["channels"] == []
        isolated_interaction_status, isolated_interaction_body, isolated_interaction_headers = request(
            "GET", f"/api/funnels/{isolated_funnel_id}/interaction-report?period=7"
        )
        assert isolated_interaction_status == 200, isolated_interaction_body
        assert isolated_interaction_headers.get("cache-control") == "private, no-store", isolated_interaction_headers
        isolated_interaction_report = json.loads(isolated_interaction_body)
        assert isolated_interaction_report["total_events"] == 0, isolated_interaction_report
        assert len(isolated_interaction_report["days"]) == 7, isolated_interaction_report
        assert all(
            day["page_viewed"] == day["form_viewed"] == day["form_started"] == 0
            for day in isolated_interaction_report["days"]
        ), isolated_interaction_report
        assert isolated_interaction_report["campaigns"] == [], isolated_interaction_report

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
        assert all(item["status"] == "new" for item in second_page["items"]), second_page

        scoped_contacted_status, scoped_contacted_body, scoped_contacted_headers = request(
            "GET", f"/api/submissions?status=contacted&funnel_id={funnel_id}"
        )
        assert scoped_contacted_status == 200, scoped_contacted_body
        assert scoped_contacted_headers.get("cache-control") == "private, no-store", scoped_contacted_headers
        scoped_contacted = json.loads(scoped_contacted_body)
        assert [item["submission_id"] for item in scoped_contacted["items"]] == [triage_id], scoped_contacted
        scoped_page_one_status, scoped_page_one_body, _ = request(
            "GET", f"/api/submissions?funnel_id={funnel_id}"
        )
        assert scoped_page_one_status == 200, scoped_page_one_body
        scoped_page_one = json.loads(scoped_page_one_body)
        assert all(item["funnel_name"] == "PR-033 local form smoke" for item in scoped_page_one["items"]), scoped_page_one
        assert scoped_page_one["next_cursor"], scoped_page_one
        scoped_page_two_status, scoped_page_two_body, _ = request(
            "GET", f"/api/submissions?funnel_id={funnel_id}&cursor={scoped_page_one['next_cursor']}"
        )
        assert scoped_page_two_status == 200, scoped_page_two_body
        scoped_page_two = json.loads(scoped_page_two_body)
        assert scoped_page_two["next_cursor"] is None, scoped_page_two
        assert all(item["funnel_name"] == "PR-033 local form smoke" for item in scoped_page_two["items"]), scoped_page_two
        combined_page_one_status, combined_page_one_body, _ = request(
            "GET", f"/api/submissions?status=new&funnel_id={funnel_id}"
        )
        assert combined_page_one_status == 200, combined_page_one_body
        combined_page_one = json.loads(combined_page_one_body)
        assert len(combined_page_one["items"]) == 25, combined_page_one
        assert combined_page_one["next_cursor"], combined_page_one
        combined_page_two_status, combined_page_two_body, _ = request(
            "GET", f"/api/submissions?status=new&funnel_id={funnel_id}&cursor={combined_page_one['next_cursor']}"
        )
        assert combined_page_two_status == 200, combined_page_two_body
        combined_page_two = json.loads(combined_page_two_body)
        assert combined_page_two["next_cursor"] is None, combined_page_two
        assert all(item["status"] == "new" for item in combined_page_two["items"]), combined_page_two
        assert all(item["funnel_name"] == "PR-033 local form smoke" for item in combined_page_two["items"]), combined_page_two
        mismatched_cursor_status, mismatched_cursor_body, mismatched_cursor_headers = request(
            "GET", f"/api/submissions?status=contacted&funnel_id={funnel_id}&cursor={combined_page_one['next_cursor']}"
        )
        assert mismatched_cursor_status == 400, mismatched_cursor_body
        assert mismatched_cursor_headers.get("cache-control") == "private, no-store", mismatched_cursor_headers
        empty_scope_status, empty_scope_body, _ = request(
            "GET", f"/api/submissions?funnel_id={isolated_funnel_id}"
        )
        assert empty_scope_status == 200, empty_scope_body
        assert json.loads(empty_scope_body)["items"] == [], empty_scope_body

        expired_status, _, expired_headers = request(
            "POST",
            "/api/submissions/90000000-0000-4000-8000-000000000002/status",
            b'{"status":"closed"}',
            {"Content-Type": "application/json"},
        )
        assert expired_status == 404, expired_status
        assert expired_headers.get("cache-control") == "private, no-store", expired_headers

        if os.environ.get("PR033_BROWSER") == "1":
            subprocess.run(
                [
                    "node",
                    str(root / "scripts/test-pr069-interaction-report-browser.mjs"),
                    f"http://localhost:{port}/funnels/{funnel_id}/interaction-report",
                ],
                cwd=root,
                env={key: value for key, value in os.environ.items() if key != "XDG_CACHE_HOME"},
                check=True,
                timeout=90,
            )
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
                    funnel_id,
                    isolated_funnel_id,
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
                "UPDATE public_form_submissions SET submitted_at = unixepoch() - 86401, expires_at = unixepoch() - 1; UPDATE public_interaction_events SET received_at = unixepoch() - 86401, expires_at = unixepoch() - 1",
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
                "SELECT (SELECT COUNT(*) FROM public_form_submissions) AS submissions, (SELECT COUNT(*) FROM public_interaction_events) AS interaction_events",
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
        assert cleanup_result["interaction_events"] == 0, cleanup_result

        log.close()
        assert "ada@example.test" not in log_path.read_text(encoding="utf-8"), log_path.read_text(encoding="utf-8")
        print("PR-067/PR-033 local Worker/D1 interaction and public form flows passed")
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
