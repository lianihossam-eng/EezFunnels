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
import signal
import socket
import subprocess
import sys
import tempfile
import time

root = Path(sys.argv[1])
wrangler = root / "node_modules" / ".bin" / "wrangler"
with socket.socket() as probe:
    probe.bind(("127.0.0.1", 0))
    port = probe.getsockname()[1]

with tempfile.TemporaryDirectory(prefix="eezfunnel-pr024-stage-") as directory:
    state = Path(directory)
    log_path = state / "wrangler.log"
    subprocess.run(
        [
            str(wrangler), "d1", "migrations", "apply", "DB", "--local", "--env", "dev",
            "--persist-to", str(state / "persist"),
        ],
        cwd=root,
        check=True,
        timeout=120,
        stdout=subprocess.DEVNULL,
    )
    log = log_path.open("w", encoding="utf-8")
    def worker_command_for(worker_port):
        return [
            str(wrangler), "dev", "--local", "--env", "dev", "--port", str(worker_port),
            "--ip", "127.0.0.1", "--var", f"ALLOWED_HOSTS:localhost:{worker_port}",
            "--persist-to", str(state / "persist"), "--log-level", "error",
        ]

    worker_command = worker_command_for(port)
    worker = subprocess.Popen(
        [*worker_command, "--var", "V2_STAGE_ORPHAN_BYTE_BUDGET:0"],
        cwd=root,
        stdout=log,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )

    def request(method, path, body=None, headers=None, include_headers=False):
        connection = http.client.HTTPConnection("127.0.0.1", port, timeout=20)
        request_headers = {"Host": f"localhost:{port}"}
        request_headers.update(headers or {})
        connection.request(method, path, body=body, headers=request_headers)
        response = connection.getresponse()
        content = response.read()
        status = response.status
        response_headers = {name.lower(): value for name, value in response.getheaders()}
        connection.close()
        if include_headers:
            return status, content, response_headers
        return status, content

    def wait_for_worker():
        deadline = time.monotonic() + 240
        while time.monotonic() < deadline:
            if worker.poll() is not None:
                raise RuntimeError("Local Worker exited during startup")
            try:
                status, _ = request("GET", "/health")
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

    def create_funnel(key):
        payload = {"name": f"PR-024 stage {key}", "blueprint": "lead_gen"}
        status, content = post_json(
            "/api/funnels/v2",
            payload,
            key,
        )
        assert status == 201, (status, content)
        result = json.loads(content)
        replay_status, replay_content = post_json("/api/funnels/v2", payload, key)
        assert replay_status == 201, (replay_status, replay_content)
        replay_result = json.loads(replay_content)
        assert {k: v for k, v in replay_result.items() if k != "correlation_id"} == {
            k: v for k, v in result.items() if k != "correlation_id"
        }, (result, replay_result)
        return (
            result["funnel"]["id"],
            result["initial_revision"]["id"],
            result["funnel"]["slug"],
        )

    def mint_stage(funnel_id, revision_id, data, key):
        status, content = post_json(
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

    def save_v2(funnel_id, revision_id, stage_id, data, key):
        return post_json(
            f"/api/funnels/{funnel_id}/revisions-v2",
            {
                "base_revision_id": revision_id,
                "stage_id": stage_id,
                "expected_length": len(data),
                "expected_sha256": hashlib.sha256(data).hexdigest(),
            },
            key,
        )

    def edit_v2(funnel_id, value):
        return request(
            "POST",
            f"/api/funnels/{funnel_id}/edit-v2",
            json.dumps(value, separators=(",", ":")).encode(),
            {"Content-Type": "application/json"},
        )

    def active_group_members(process_group):
        members = []
        proc_root = Path("/proc")
        for entry in proc_root.iterdir():
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
        no_stage_control_funnel, _, _no_stage_control_slug = create_funnel(
            "stage-control-unavailable-create"
        )
        assert no_stage_control_funnel
        os.killpg(worker.pid, signal.SIGINT)
        worker.wait(timeout=10)
        if active_group_members(worker.pid):
            os.killpg(worker.pid, signal.SIGTERM)
            deadline = time.monotonic() + 5
            while active_group_members(worker.pid) and time.monotonic() < deadline:
                time.sleep(0.1)
            if active_group_members(worker.pid):
                os.killpg(worker.pid, signal.SIGKILL)
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        worker_command = worker_command_for(port)
        worker = subprocess.Popen(
            worker_command,
            cwd=root,
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        wait_for_worker()
        status, content = request("POST", "/__local/stage-control/bootstrap")
        assert status == 204, (status, content)
        status, content = request("GET", "/api/v2/stage-capacity")
        assert status == 200, (status, content)
        initial_capacity = json.loads(content)
        assert initial_capacity["orphan_bytes_held"] == 0, initial_capacity
        assert initial_capacity["orphan_keys_held"] == 0, initial_capacity

        legacy_create_status, legacy_create_content = post_json(
            "/api/funnels",
            {"name": "PR-024 conversion", "blueprint": "lead_gen"},
            "stage-legacy-conversion-create",
        )
        assert legacy_create_status == 201, (legacy_create_status, legacy_create_content)
        legacy_created = json.loads(legacy_create_content)
        legacy_funnel = legacy_created["funnel"]["id"]
        legacy_initial_revision = legacy_created["initial_revision"]["id"]
        initial_publish_status, initial_publish_content = request(
            "POST",
            f"/api/funnels/{legacy_funnel}/publish",
            json.dumps(
                {
                    "revision_id": legacy_initial_revision,
                    "expected_published_revision_id": None,
                },
                separators=(",", ":"),
            ).encode(),
            {"Content-Type": "application/json", "Idempotency-Key": "stage-legacy-publish"},
        )
        assert initial_publish_status == 200, (initial_publish_status, initial_publish_content)
        preview_path = f"/api/funnels/{legacy_funnel}/convert-v2/preview"
        preview_body = {"base_revision_id": legacy_initial_revision}
        blocked_status, blocked_content = post_json(
            preview_path, preview_body, "stage-legacy-preview-blocked"
        )
        assert blocked_status == 200, (blocked_status, blocked_content)
        blocked_preview = json.loads(blocked_content)
        assert blocked_preview["conversion_available"] is False, blocked_preview
        blocked_paths = [blocker["path"] for blocker in blocked_preview["blockers"]]
        assert blocked_paths == [
            "contents.hero.cta_url",
            "contents.cta.cta_url",
            "contents.footer.links[0].url",
        ], blocked_preview

        legacy_edit_body = {
            "base_revision_id": legacy_initial_revision,
            "section_ids": ["hero", "features", "cta", "footer"],
            "settings": {
                "schema_version": 1,
                "changes": [
                    {
                        "id": "section.hero.cta_url",
                        "value": {"kind": "text", "value": "https://example.com/offer"},
                    },
                    {
                        "id": "section.cta.cta_url",
                        "value": {"kind": "text", "value": "https://example.com/offer"},
                    },
                    {
                        "id": "section.footer.links.0.url",
                        "value": {"kind": "text", "value": "https://example.com/contact"},
                    },
                ],
            },
        }
        legacy_edit_status, legacy_edit_content = request(
            "PUT",
            f"/api/funnels/{legacy_funnel}/revisions",
            json.dumps(legacy_edit_body, separators=(",", ":")).encode(),
            {"Content-Type": "application/json", "Idempotency-Key": "stage-legacy-links"},
        )
        assert legacy_edit_status == 200, (legacy_edit_status, legacy_edit_content)
        legacy_revision = json.loads(legacy_edit_content)["revision_id"]
        ready_status, ready_content = post_json(
            preview_path,
            {"base_revision_id": legacy_revision},
            "stage-legacy-preview-ready",
        )
        assert ready_status == 200, (ready_status, ready_content)
        ready_preview = json.loads(ready_content)
        assert ready_preview["conversion_available"] is True, ready_preview
        assert ready_preview["blockers"] == [], ready_preview
        assert ready_preview["warnings"], ready_preview
        assert "PR-024 conversion" in ready_preview["preview_html"], ready_preview

        legacy_stage_data = (
            root / "crates/domain/tests/fixtures/document_v2_minimal.json"
        ).read_bytes().replace(b'"funnel-01"', f'"{legacy_funnel}"'.encode())
        legacy_stage_status, legacy_stage_content = post_json(
            f"/api/funnels/{legacy_funnel}/stages-v2",
            {
                "base_revision_id": legacy_revision,
                "expected_length": len(legacy_stage_data),
                "expected_sha256": hashlib.sha256(legacy_stage_data).hexdigest(),
            },
            "stage-legacy-v2-stage-rejected",
        )
        assert legacy_stage_status == 409, (legacy_stage_status, legacy_stage_content)
        legacy_save_status, legacy_save_content = post_json(
            f"/api/funnels/{legacy_funnel}/revisions-v2",
            {
                "base_revision_id": legacy_revision,
                "stage_id": "0" * 64,
                "expected_length": len(legacy_stage_data),
                "expected_sha256": hashlib.sha256(legacy_stage_data).hexdigest(),
            },
            "stage-legacy-v2-save-rejected",
        )
        assert legacy_save_status == 409, (legacy_save_status, legacy_save_content)

        conversion_body = {"base_revision_id": legacy_revision}
        conversion_status, conversion_content = post_json(
            f"/api/funnels/{legacy_funnel}/convert-v2",
            conversion_body,
            "stage-legacy-conversion-commit",
        )
        assert conversion_status == 201, (conversion_status, conversion_content)
        converted_revision = json.loads(conversion_content)
        assert converted_revision["schema_version"] == 2, converted_revision
        conversion_replay_status, conversion_replay_content = post_json(
            f"/api/funnels/{legacy_funnel}/convert-v2",
            conversion_body,
            "stage-legacy-conversion-commit",
        )
        assert conversion_replay_status == 201, (
            conversion_replay_status,
            conversion_replay_content,
        )
        assert json.loads(conversion_replay_content)["revision_id"] == converted_revision[
            "revision_id"
        ], conversion_replay_content
        converted_read_status, converted_read_content = request(
            "GET", f"/api/funnels/{legacy_funnel}"
        )
        assert converted_read_status == 200, converted_read_content
        converted_read = json.loads(converted_read_content)
        assert converted_read["schema_version"] == 2, converted_read
        assert converted_read["published_revision_id"] == legacy_initial_revision, converted_read
        legacy_history_status, legacy_history_content = request(
            "GET", f"/api/funnels/{legacy_funnel}/revisions-v2"
        )
        assert legacy_history_status == 200, legacy_history_content
        legacy_history = json.loads(legacy_history_content)["revisions"]
        assert any(
            revision["revision_id"] == legacy_revision and revision["schema_version"] == 1
            for revision in legacy_history
        ), legacy_history
        assert any(
            revision["revision_id"] == converted_revision["revision_id"]
            and revision["schema_version"] == 2
            for revision in legacy_history
        ), legacy_history
        stale_conversion_status, stale_conversion_content = post_json(
            f"/api/funnels/{legacy_funnel}/convert-v2",
            {"base_revision_id": legacy_revision},
            "stage-legacy-conversion-stale",
        )
        assert stale_conversion_status == 409, (
            stale_conversion_status,
            stale_conversion_content,
        )

        first_funnel, first_revision, first_slug = create_funnel("stage-first-create")
        status, content, response_headers = request(
            "PATCH",
            f"/api/funnels/{first_funnel}/stages-v2",
            include_headers=True,
        )
        assert status == 405, (status, content)
        assert response_headers.get("allow") == "POST, PUT", response_headers
        first_document = json.loads(
            (root / "crates/domain/tests/fixtures/document_v2_minimal.json").read_bytes()
        )
        first_document["funnel_id"] = first_funnel
        first_document["pages"][0]["sections"][0]["blocks"].append(
            {
                "id": "block-next",
                "kind": "button",
                "label": "Next",
                "action": {"kind": "navigate_page", "page_id": "page-next"},
            }
        )
        first_document["pages"].append(
            {
                "id": "page-next",
                "slug": "next",
                "title": "Next step",
                "sections": [
                    {
                        "id": "section-next",
                        "type": "hero",
                        "blocks": [
                            {
                                "id": "block-next-title",
                                "kind": "heading",
                                "level": 1,
                                "text": "Next step content",
                            }
                        ],
                    }
                ],
            }
        )
        first_data = json.dumps(first_document, separators=(",", ":")).encode()
        first_stage = mint_stage(first_funnel, first_revision, first_data, "stage-first-mint")
        assert len(first_stage) == 64
        first_upload = upload(first_funnel, first_stage, first_data)
        assert first_upload[0] == 204, first_upload
        retry_upload = upload(first_funnel, first_stage, first_data)
        assert retry_upload[0] == 204, retry_upload
        assert upload(first_funnel, first_stage, first_data + b" changed")[0] == 400
        assert mint_stage(first_funnel, first_revision, first_data, "stage-first-mint") == first_stage

        save_status, save_content = save_v2(
            first_funnel, first_revision, first_stage, first_data, "stage-first-save"
        )
        assert save_status == 201, (save_status, save_content)
        saved_revision = json.loads(save_content)
        assert saved_revision["schema_version"] == 2, saved_revision
        assert saved_revision["funnel_id"] == first_funnel, saved_revision
        assert saved_revision["base_revision_id"] == first_revision, saved_revision
        assert saved_revision["revision_id"] != first_revision, saved_revision
        replay_status, replay_content = save_v2(
            first_funnel, first_revision, first_stage, first_data, "stage-first-save"
        )
        assert replay_status == 201, (replay_status, replay_content)
        replayed_revision = json.loads(replay_content)
        assert replayed_revision["revision_id"] == saved_revision["revision_id"], replayed_revision
        assert replayed_revision["schema_version"] == saved_revision["schema_version"], replayed_revision
        assert save_v2(
            first_funnel,
            first_revision,
            first_stage,
            first_data + b" changed",
            "stage-first-save",
        )[0] == 409

        edit_request = {
            "base_revision_id": saved_revision["revision_id"],
            "page_id": "page-home",
            "document_json": first_data.decode(),
            "operation_id": "ab" * 16,
            "edit": {
                "operation": "add_section",
                "page": "page-home",
                "after": "section-hero",
                "section_type": "features",
            },
        }
        edit_status, edit_content = edit_v2(first_funnel, edit_request)
        assert edit_status == 200, (edit_status, edit_content)
        edited = json.loads(edit_content)
        edited_data = edited["document_json"].encode()
        assert edited["base_revision_id"] == saved_revision["revision_id"], edited
        assert edited["preview_html"], edited
        edit_replay_status, edit_replay_content = edit_v2(first_funnel, edit_request)
        assert edit_replay_status == 200, (edit_replay_status, edit_replay_content)
        assert json.loads(edit_replay_content)["document_json"] == edited["document_json"]

        second_save_stage = mint_stage(
            first_funnel,
            saved_revision["revision_id"],
            edited_data,
            "stage-first-second-mint",
        )
        assert upload(first_funnel, second_save_stage, edited_data)[0] == 204
        second_save_status, second_save_content = save_v2(
            first_funnel,
            saved_revision["revision_id"],
            second_save_stage,
            edited_data,
            "stage-first-second-save",
        )
        assert second_save_status == 201, (second_save_status, second_save_content)
        second_saved_revision = json.loads(second_save_content)
        assert second_saved_revision["base_revision_id"] == saved_revision["revision_id"]

        publish_status, publish_content = request(
            "POST",
            f"/api/funnels/{first_funnel}/publish",
            json.dumps(
                {
                    "revision_id": second_saved_revision["revision_id"],
                    "expected_published_revision_id": None,
                },
                separators=(",", ":"),
            ).encode(),
            {"Content-Type": "application/json", "Idempotency-Key": "stage-first-publish"},
        )
        assert publish_status == 200, (publish_status, publish_content)
        entry_status, entry_html = request("GET", f"/f/{first_slug}")
        assert entry_status == 200, entry_html
        assert b'<a class="v2-button" href="/f/' + first_slug.encode() + b'/next">Next</a>' in entry_html
        entry_etag = f'"{hashlib.sha256(entry_html).hexdigest()}"'
        next_status, next_html = request(
            "GET",
            f"/f/{first_slug}/next",
            headers={"If-None-Match": entry_etag},
        )
        assert next_status == 200, (next_status, next_html)
        assert b"<title>Next step</title>" in next_html, next_html
        assert b"Next step content" in next_html, next_html
        next_etag = f'"{hashlib.sha256(next_html).hexdigest()}"'
        assert next_etag != entry_etag
        next_cached_status, next_cached_body = request(
            "GET",
            f"/f/{first_slug}/next",
            headers={"If-None-Match": next_etag},
        )
        assert next_cached_status == 304, (next_cached_status, next_cached_body)
        assert next_cached_body == b"", next_cached_body
        missing_page_status, missing_page_content = request(
            "GET", f"/f/{first_slug}/not-a-page"
        )
        assert missing_page_status == 404, (missing_page_status, missing_page_content)
        assert json.loads(missing_page_content)["code"] == "not_found/slug", missing_page_content

        history_status, history_content = request(
            "GET", f"/api/funnels/{first_funnel}/revisions-v2"
        )
        assert history_status == 200, (history_status, history_content)
        history = json.loads(history_content)["revisions"]
        assert len(history) == 3, history
        assert all(revision["schema_version"] == 2 for revision in history), history
        old_document_status, old_document = request(
            "GET",
            f"/api/funnels/{first_funnel}/document?revision_id={first_revision}",
        )
        assert old_document_status == 200, old_document
        assert json.loads(old_document)["funnel_id"] == first_funnel

        status, content = request("POST", "/__local/stage-control/bootstrap")
        assert status == 204, (status, content)

        second_funnel, second_revision, _second_slug = create_funnel("stage-second-create")
        second_data = b"second bounded stage snapshot"
        second_stage = mint_stage(second_funnel, second_revision, second_data, "stage-second-mint")
        second_upload = upload(second_funnel, second_stage, second_data)
        assert second_upload[0] == 204, second_upload

        status, content = request("POST", "/__local/stage-sweep")
        assert status == 200, (status, content)
        progress = json.loads(content)
        assert progress["deleted"] == 0, progress

        rows = subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(state / "persist"), "--json", "--command",
                f"SELECT COUNT(*) AS attempts, COUNT(DISTINCT sequence) AS distinct_sequences, MIN(sequence) AS first_sequence, MAX(sequence) AS last_sequence, (SELECT COUNT(*) FROM revisions WHERE funnel_id = '{first_funnel}') AS first_funnel_revisions, (SELECT revision_id FROM funnel_editor_heads WHERE funnel_id = '{first_funnel}') AS first_funnel_head FROM v2_document_stage_attempts WHERE state = 'complete'",
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        ).stdout
        payload = json.loads(rows)

        def find_result(value):
            if isinstance(value, dict):
                if "distinct_sequences" in value:
                    return value
                for child in value.values():
                    result = find_result(child)
                    if result is not None:
                        return result
            elif isinstance(value, list):
                for child in value:
                    result = find_result(child)
                    if result is not None:
                        return result
            return None

        result = find_result(payload)
        assert result is not None, rows
        assert result["attempts"] == 1, result
        assert result["distinct_sequences"] == 1, result
        assert result["last_sequence"] == "00000000000000000003", result
        assert result["first_funnel_revisions"] == 3, result
        assert result["first_funnel_head"] == second_saved_revision["revision_id"], result
        status, content = request("GET", "/api/v2/stage-capacity")
        assert status == 200, (status, content)
        capacity = json.loads(content)
        assert capacity["orphan_bytes_held"] == len(second_data), capacity
        assert capacity["orphan_keys_held"] == 1, capacity
        assert capacity["worst_case_orphan_bytes"] == capacity["orphan_byte_budget"], capacity
        assert capacity["worst_case_orphan_keys"] == capacity["orphan_key_budget"], capacity
        print("PR-024 local D1/R2 stage, permanent orphan budget, and immutable save flow passed")
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
