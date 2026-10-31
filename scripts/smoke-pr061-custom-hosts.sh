#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-/tmp/eezfunnel-xdg}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-/tmp/eezfunnel-cache}"
export NPM_CONFIG_CACHE="${NPM_CONFIG_CACHE:-/tmp/eezfunnel-npm-cache}"
export WRANGLER_SEND_METRICS=false

python3 - "$repo_root" <<'PY'
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import time
import uuid
import xml.etree.ElementTree as ElementTree

root = Path(sys.argv[1])
wrangler = root / "node_modules" / ".bin" / "wrangler"
slug = "pr061-smoke"
with socket.socket() as probe:
    probe.bind(("127.0.0.1", 0))
    port = probe.getsockname()[1]

with tempfile.TemporaryDirectory(prefix="eezfunnel-pr061-custom-hosts-") as directory:
    state = Path(directory)
    persist = state / "persist"
    log_path = state / "wrangler.log"
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
            str(wrangler), "dev", "--local", "--env", "dev", "--local-protocol", "https",
            "--port", str(port),
            "--ip", "127.0.0.1", "--var", f"ALLOWED_HOSTS:localhost:{port}",
            "--var", "PUBLIC_BASE_URL:https://platform.example",
            "--var", f"PUBLIC_FUNNEL_HOSTS:custom.example={slug}",
            "--var", "PUBLIC_INTERACTION_RETENTION_DAYS:1",
            "--var", "PUBLIC_INTERACTION_EVENTS_PER_MINUTE:10",
            "--env-file", str(interaction_env_file),
            "--persist-to", str(persist), "--log-level", "error",
        ],
        cwd=root,
        stdout=log,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )

    def request(method, path, body=None, headers=None, host=None):
        import http.client

        connection = http.client.HTTPSConnection(
            "127.0.0.1", port, timeout=20, context=ssl._create_unverified_context()
        )
        request_headers = {"Host": host or f"localhost:{port}"}
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

    try:
        wait_for_worker()
        status, body, _ = post_json(
            "/api/funnels",
            {"name": "PR-061 custom host smoke", "blueprint": "lead_gen"},
            "pr061-create",
        )
        assert status == 201, (status, body)
        created = json.loads(body)
        funnel_id = created["funnel"]["id"]
        revision_id = created["initial_revision"]["id"]

        # The host mapping must be present before the Worker starts. Pin this
        # disposable local funnel to its test slug so the smoke stays repeatable.
        subprocess.run(
            [
                str(wrangler), "d1", "execute", "DB", "--local", "--env", "dev",
                "--persist-to", str(persist), "--command",
                f"UPDATE funnels SET slug='{slug}' WHERE id='{funnel_id}'",
            ],
            cwd=root,
            check=True,
            timeout=30,
            stdout=subprocess.DEVNULL,
        )

        status, body, _ = request("GET", "/", host="custom.example")
        assert status == 404, (status, body)
        status, empty_sitemap, _ = request("GET", "/sitemap.xml", host="custom.example")
        assert status == 200, (status, empty_sitemap)
        assert ElementTree.fromstring(empty_sitemap).findall(
            "{http://www.sitemaps.org/schemas/sitemap/0.9}url"
        ) == []

        status, body, _ = post_json(
            f"/api/funnels/{funnel_id}/publish",
            {"revision_id": revision_id, "expected_published_revision_id": None},
            "pr061-publish",
        )
        assert status == 200, (status, body)

        status, page, headers = request("GET", "/", host="custom.example")
        assert status == 200, (status, page)
        assert headers.get("content-type") == "text/html; charset=utf-8", headers
        assert b"https://custom.example/f/pr061-smoke" in page, page
        assert request("HEAD", "/", host="custom.example")[0] == 200
        assert request("GET", "/f/pr061-smoke", host="custom.example")[0] == 200

        campaign_registry_path = f"/api/funnels/{funnel_id}/campaigns"
        register_status, register_body, _ = post_json(
            campaign_registry_path,
            {"campaign_id": "custom-campaign"},
            "pr061-campaign-register",
        )
        assert register_status == 200, (register_status, register_body)
        campaign_registry = json.loads(register_body)
        assert campaign_registry["campaign_links"] == [{
            "campaign_id": "custom-campaign",
            "url": "https://custom.example/f/pr061-smoke?campaign_id=custom-campaign",
            "relative_path": "/f/pr061-smoke?campaign_id=custom-campaign",
        }], campaign_registry

        interaction_event_id = str(uuid.uuid4())
        interaction_event = {
            "schema_version": 1,
            "event_id": interaction_event_id,
            "kind": "page_viewed",
            "observed_at_unix_seconds": 1,
            "funnel_id": funnel_id,
            "revision_id": revision_id,
            "page_id": "legacy-entry",
            "campaign_id": "custom-campaign",
        }
        interaction_status, interaction_body, _ = request(
            "POST",
            f"/api/public/funnels/{slug}/interaction-events",
            json.dumps(interaction_event, separators=(",", ":")).encode(),
            {
                "Content-Type": "application/json",
                "Origin": "https://custom.example",
                "X-EezFunnel-Interaction-Consent": "granted",
            },
            host="custom.example",
        )
        assert interaction_status == 204, (interaction_status, interaction_body)
        assert d1_query(
            "SELECT COUNT(*) AS events, MIN(campaign_id) AS campaign_id "
            "FROM public_interaction_events "
            f"WHERE event_id = '{interaction_event_id}'",
            "events",
        ) == {"events": 1, "campaign_id": "custom-campaign"}

        for blocked in [
            "/api/funnels",
            f"/api/funnels/{funnel_id}/campaigns",
            "/api/submissions",
            "/mcp",
            "/admin",
            "/dashboard",
            "/health",
            "/assets/index.js",
            "/__local/probe",
        ]:
            assert request("GET", blocked, host="custom.example")[0] == 404, blocked
        assert request("GET", "/media/" + "0" * 64, host="custom.example")[0] == 404

        status, sitemap, _ = request("GET", "/sitemap.xml", host="custom.example")
        assert status == 200, (status, sitemap)
        sitemap_urls = [
            item.text
            for item in ElementTree.fromstring(sitemap).findall(
                "{http://www.sitemaps.org/schemas/sitemap/0.9}url/"
                "{http://www.sitemaps.org/schemas/sitemap/0.9}loc"
            )
        ]
        assert sitemap_urls == ["https://custom.example/f/pr061-smoke"], sitemap_urls
        status, platform_sitemap, _ = request("GET", "/sitemap.xml")
        assert status == 200, (status, platform_sitemap)
        assert b"pr061-smoke" not in platform_sitemap, platform_sitemap

        status, robots, _ = request("GET", "/robots.txt", host="custom.example")
        assert status == 200, (status, robots)
        assert b"Sitemap: https://custom.example/sitemap.xml" in robots, robots
        assert b"Disallow: /api/" in robots and b"Disallow: /mcp" in robots, robots

        status, body, _ = post_json(
            f"/api/funnels/{funnel_id}/unpublish",
            {"expected_published_revision_id": revision_id},
            "pr061-unpublish",
        )
        assert status == 200, (status, body)
        assert request("GET", "/", host="custom.example")[0] == 404
        status, sitemap, _ = request("GET", "/sitemap.xml", host="custom.example")
        assert status == 200, (status, sitemap)
        assert ElementTree.fromstring(sitemap).findall(
            "{http://www.sitemaps.org/schemas/sitemap/0.9}url"
        ) == []
        print("PR-061 local custom-host routing, SEO isolation, and publish lifecycle passed")
    except Exception:
        if log_path.exists():
            print(log_path.read_text(encoding="utf-8"), file=sys.stderr)
        raise
    finally:
        if worker.poll() is None:
            try:
                os.killpg(worker.pid, signal.SIGINT)
                worker.wait(timeout=10)
            except ProcessLookupError:
                worker.wait(timeout=5)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(worker.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                worker.wait(timeout=5)
        log.close()
PY
