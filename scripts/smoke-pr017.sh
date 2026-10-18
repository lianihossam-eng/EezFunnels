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
export WRANGLER_SEND_METRICS=false

python3 - "$repo_root" <<'PY'
import http.client
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import subprocess
import sys
import tempfile
import time
import uuid

root = Path(sys.argv[1])
wrangler = root / "node_modules" / ".bin" / "wrangler"
port = 18789
missing_secret_port = 18790
token = secrets.token_urlsafe(32)
marker = "mcp-request-secret-marker"

for candidate in (port, missing_secret_port):
    with socket.socket() as probe:
        if probe.connect_ex(("127.0.0.1", candidate)) == 0:
            raise SystemExit(f"Port {candidate} is occupied")

with tempfile.TemporaryDirectory(prefix="eezfunnel-pr017-") as directory:
    isolated = Path(directory)
    storage = isolated / "d1"
    token_file = isolated / "mcp.env"
    token_file.write_text(f"EEZ_MCP_TOKEN={token}\n", encoding="utf-8")
    token_file.chmod(0o600)
    empty_env_file = isolated / "empty.env"
    empty_env_file.write_text("", encoding="utf-8")
    log_path = isolated / "wrangler.log"

    subprocess.run(
        [str(wrangler), "d1", "migrations", "apply", "DB", "--local", "--env", "dev", "--persist-to", str(storage)],
        cwd=root,
        check=True,
        timeout=120,
        stdout=subprocess.DEVNULL,
    )

    def launch(worker_port, env_file):
        log = log_path.open("a", encoding="utf-8")
        process = subprocess.Popen(
            [
                str(wrangler), "dev", "--local", "--env", "dev", "--port", str(worker_port),
                "--ip", "127.0.0.1", "--var",
                f"ALLOWED_HOSTS:localhost:{worker_port},127.0.0.1:{worker_port}",
                "--env-file", str(env_file), "--persist-to", str(storage), "--log-level", "error",
            ],
            cwd=root,
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        return process, log

    def stop(process, log):
        def live_group():
            rows = subprocess.run(
                ["ps", "-eo", "pgid=,stat="], capture_output=True, text=True, check=True, timeout=5
            ).stdout.splitlines()
            return any(
                len(fields := row.split()) == 2
                and fields[0] == str(process.pid)
                and not fields[1].startswith("Z")
                for row in rows
            )

        for sig, limit in ((signal.SIGINT, 10), (signal.SIGTERM, 5), (signal.SIGKILL, 5)):
            if not live_group():
                break
            try:
                os.killpg(process.pid, sig)
            except ProcessLookupError:
                break
            deadline = time.monotonic() + limit
            while live_group() and time.monotonic() < deadline:
                time.sleep(0.1)
        process.wait(timeout=5)
        log.close()
        assert not live_group(), "Worker process group has live members after cleanup"

    def wait_for_worker(worker, worker_port):
        deadline = time.monotonic() + 180
        while True:
            connection = http.client.HTTPConnection("127.0.0.1", worker_port, timeout=1)
            try:
                connection.request("GET", "/health", headers={"Host": f"localhost:{worker_port}"})
                response = connection.getresponse()
                response.read()
                if response.status == 200:
                    return
            except OSError:
                pass
            finally:
                connection.close()
            if worker.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError("Local Worker failed to start; Wrangler output was captured")
            time.sleep(0.5)

    def request(worker_port, method, path, body=None, auth=None, auth_scheme="Bearer", host=None, content_type="application/json", duplicate_host=False, protocol_version=None, accept="application/json, text/event-stream", origin=None, correlation_id=None, response_headers=False):
        connection = http.client.HTTPConnection("127.0.0.1", worker_port, timeout=10)
        headers = []
        headers.append(("Host", host or f"localhost:{worker_port}"))
        headers.append(("Connection", "close"))
        if duplicate_host:
            headers.append(("Host", "attacker.example"))
        if auth is not None:
            headers.append(("Authorization", f"{auth_scheme} {auth}"))
        if content_type is not None:
            headers.append(("Content-Type", content_type))
        if accept is not None:
            headers.append(("Accept", accept))
        if protocol_version is not None:
            headers.append(("MCP-Protocol-Version", protocol_version))
        if isinstance(origin, (tuple, list)):
            headers.extend(("Origin", value) for value in origin)
        elif origin is not None:
            headers.append(("Origin", origin))
        if correlation_id is not None:
            headers.append(("X-Correlation-ID", correlation_id))
        connection.putrequest(method, path, skip_host=True, skip_accept_encoding=True)
        for name, value in headers:
            connection.putheader(name, value)
        if body is not None:
            raw = body.encode("utf-8") if isinstance(body, str) else body
            connection.putheader("Content-Length", str(len(raw)))
            connection.endheaders(raw)
        else:
            connection.endheaders()
        response = connection.getresponse()
        status = response.status
        payload = response.read()
        received_headers = {name.lower(): value for name, value in response.getheaders()}
        connection.close()
        if response_headers:
            return status, payload, received_headers
        return status, payload

    def rpc(worker_port, message, auth=token, **kwargs):
        return request(worker_port, "POST", "/mcp", json.dumps(message, separators=(",", ":")), auth=auth, **kwargs)

    def assert_rpc_success(response, expected_id):
        status, body = response
        assert status == 200, (status, body[:500])
        message = json.loads(body)
        assert message["jsonrpc"] == "2.0" and message["id"] == expected_id, message
        assert "result" in message and "error" not in message, message
        return message["result"]

    def tool_call(name, arguments, request_id):
        result = assert_rpc_success(
            rpc(port, {
                "jsonrpc": "2.0", "id": request_id, "method": "tools/call",
                "params": {"name": name, "arguments": arguments},
            }),
            request_id,
        )
        content = result.get("content", [])
        assert content and content[0].get("type") == "text", result
        return result, json.loads(content[0]["text"])

    worker, log = launch(port, token_file)
    try:
        wait_for_worker(worker, port)

        client_env = os.environ.copy()
        client_env["EEZ_MCP_SMOKE_URL"] = f"http://127.0.0.1:{port}/mcp"
        client_env["EEZ_MCP_SMOKE_TOKEN"] = token
        subprocess.run(
            ["npm", "run", "--silent", "test:pr063:mcp-client"],
            cwd=root,
            env=client_env,
            check=True,
            timeout=60,
        )

        status, body = request(port, "GET", "/mcp", auth="wrong-token")
        assert status == 401 and b"wrong-token" not in body, (status, body)
        status, body = request(port, "GET", "/mcp", auth=token, host="not-allowed.example")
        assert status == 403, (status, body)
        try:
            duplicate_status, duplicate_body = request(
                port,
                "POST",
                "/mcp",
                json.dumps({"jsonrpc": "2.0", "id": "duplicate-host", "method": "ping"}),
                auth=token,
                duplicate_host=True,
            )
            assert duplicate_status != 200, (duplicate_status, duplicate_body)
        except (http.client.HTTPException, OSError):
            pass

        status, body = rpc(
            port,
            {"jsonrpc": "2.0", "id": "mixed-case-bearer", "method": "ping"},
            auth_scheme="bEaReR",
            origin=f"http://localhost:{port}",
        )
        assert_rpc_success((status, body), "mixed-case-bearer")
        status, body = request(port, "GET", "/mcp", auth=token)
        assert status == 405, (status, body)
        status, body = request(port, "POST", "/mcp?x=1", auth=token)
        assert status == 400, (status, body)
        status, body = request(port, "POST", "/mcp", auth=token, content_type="text/plain")
        assert status == 415, (status, body)
        status, body = request(
            port,
            "POST",
            "/mcp",
            auth=token,
            content_type="application/json; charset=utf-8, text/plain",
        )
        assert status == 415, (status, body)
        status, body = request(port, "POST", "/mcp", auth=token, accept=None)
        assert status == 406, (status, body)
        status, body = request(port, "POST", "/mcp", auth=token, accept="application/json")
        assert status == 406, (status, body)
        status, body = request(port, "POST", "/mcp", b" " * (32 * 1024 + 1), auth=token)
        assert status == 413, (status, body)

        initialize = {
            "jsonrpc": "2.0", "id": "initialize-1", "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "PR-017 smoke", "version": "1"},
            },
        }
        result = assert_rpc_success(rpc(port, initialize), "initialize-1")
        assert result["protocolVersion"] == "2025-06-18", result
        assert "tools" in result["capabilities"], result
        status, body, response_headers = request(
            port,
            "POST",
            "/mcp",
            json.dumps({
                "jsonrpc": "2.0", "id": "initialize-legacy", "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26", "capabilities": {},
                    "clientInfo": {"name": "PR-017 smoke", "version": "1"},
                },
            }, separators=(",", ":")),
            auth=token,
            response_headers=True,
        )
        assert status == 200, (status, body[:500])
        assert json.loads(body)["result"]["protocolVersion"] == "2025-03-26", body
        assert response_headers.get("mcp-protocol-version") == "2025-03-26", response_headers

        march_batch = [
            {"jsonrpc": "2.0", "id": "batch-ping", "method": "ping"},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": "batch-tools", "method": "tools/list"},
        ]
        status, body = request(
            port,
            "POST",
            "/mcp",
            json.dumps(march_batch, separators=(",", ":")),
            auth=token,
            protocol_version="2025-03-26",
        )
        responses = json.loads(body)
        assert status == 200 and [response["id"] for response in responses] == ["batch-ping", "batch-tools"], (status, body)
        status, body = request(
            port,
            "POST",
            "/mcp",
            json.dumps(march_batch, separators=(",", ":")),
            auth=token,
            protocol_version="2025-06-18",
        )
        assert status == 200 and json.loads(body)["error"]["code"] == -32600, (status, body)

        unknown_tool = {
            "jsonrpc": "2.0", "id": "unknown-tool", "method": "tools/call",
            "params": {"name": "does-not-exist", "arguments": {}},
        }
        status, body = rpc(port, unknown_tool)
        unknown_response = json.loads(body)
        assert status == 200 and unknown_response["error"]["code"] == -32602, (status, body)

        command_error_id = "mcp-command-error-123"
        status, body, response_headers = request(
            port,
            "POST",
            "/mcp",
            json.dumps({
                "jsonrpc": "2.0", "id": "command-error", "method": "tools/call",
                "params": {
                    "name": "execute_command",
                    "arguments": {
                        "command_id": "funnel.create",
                        "parameters": {"name": "Missing key smoke", "blueprint": "lead_gen"},
                    },
                },
            }, separators=(",", ":")),
            auth=token,
            protocol_version="2025-06-18",
            correlation_id=command_error_id,
            response_headers=True,
        )
        command_error = json.loads(body)
        tool_result = command_error["result"]
        payload = json.loads(tool_result["content"][0]["text"])
        assert status == 200 and tool_result["isError"] is True, (status, body)
        assert payload == {
            "code": "validation/missing_field",
            "category": "validation",
            "message": "The request is invalid.",
            "correlation_id": command_error_id,
        }, payload

        status, body, response_headers = request(
            port,
            "POST",
            "/mcp",
            json.dumps({"jsonrpc": "2.0", "id": "header-version", "method": "tools/list"}),
            auth=token,
            protocol_version="2025-06-18",
            response_headers=True,
        )
        assert status == 200 and json.loads(body)["id"] == "header-version", (status, body)
        assert response_headers.get("mcp-protocol-version") == "2025-06-18", response_headers

        status, body, response_headers = request(
            port,
            "POST",
            "/mcp",
            json.dumps({"jsonrpc": "2.0", "id": "legacy-default", "method": "tools/list"}),
            auth=token,
            response_headers=True,
        )
        assert status == 200 and json.loads(body)["id"] == "legacy-default", (status, body)
        assert response_headers.get("mcp-protocol-version") == "2025-03-26", response_headers

        status, body = request(
            port,
            "POST",
            "/mcp",
            "not-json",
            auth=token,
            protocol_version="2025-11-25",
        )
        assert status == 400 and b"2025-11-25" not in body, (status, body)

        status, body = rpc(port, {"jsonrpc": "2.0", "method": "notifications/initialized"})
        assert status == 202 and body == b"", (status, body)
        assert_rpc_success(rpc(port, {"jsonrpc": "2.0", "id": 17, "method": "ping"}), 17)

        listed_tools = assert_rpc_success(
            rpc(port, {"jsonrpc": "2.0", "id": "tools", "method": "tools/list"}), "tools"
        )["tools"]
        assert {tool["name"] for tool in listed_tools} == {
            "list_commands", "execute_command", "list_funnels", "get_funnel"
        }, listed_tools

        _, commands = tool_call("list_commands", {}, "commands")
        assert len(commands["commands"]) == 9, commands
        assert {command["id"] for command in commands["commands"]} >= {
            "funnel.create", "funnel.list", "funnel.editor.read", "funnel.public.read"
        }, commands
        _, listed = tool_call("list_funnels", {}, "funnels-before")
        assert listed["items"] == [], listed

        idempotency_key = "pr017-mcp-" + uuid.uuid4().hex
        arguments = {
            "command_id": "funnel.create",
            "parameters": {"name": "MCP smoke funnel", "blueprint": "lead_gen"},
            "idempotency_key": idempotency_key,
        }
        _, created = tool_call("execute_command", arguments, "create-1")
        _, replayed = tool_call("execute_command", arguments, "create-2")
        assert created == replayed, (created, replayed)
        funnel_id = created["funnel"]["id"]
        _, listed = tool_call("list_funnels", {}, "funnels-after")
        assert len(listed["items"]) == 1 and listed["items"][0]["id"] == funnel_id, listed
        _, fetched = tool_call("get_funnel", {"funnel_id": funnel_id}, "funnel-read")
        assert fetched["funnel"]["id"] == funnel_id, fetched

        _, oversized_funnel_id = tool_call(
            "get_funnel",
            {"funnel_id": "x" * (16 * 1024)},
            "oversize-get-funnel-parameters",
        )
        assert oversized_funnel_id["code"] == "request/parameters_too_large", oversized_funnel_id

        _, parameters_too_large = tool_call(
            "execute_command",
            {
                "command_id": "funnel.create",
                "parameters": {"name": "x" * (16 * 1024), "blueprint": "lead_gen"},
                "idempotency_key": "pr017-mcp-oversize",
            },
            "oversize-parameters",
        )
        assert parameters_too_large["code"] == "request/parameters_too_large", parameters_too_large

        duplicate = (
            '{"jsonrpc":"2.0","id":"duplicate","method":"ping",'
            f'"params":{{"marker":"{marker}","marker":"discarded"}}}}'
        )
        status, body = request(port, "POST", "/mcp", duplicate, auth=token)
        assert status == 200, (status, body)
        assert json.loads(body)["error"]["code"] == -32700, body
        status, body = rpc(port, {"jsonrpc": "2.0", "id": None, "method": "ping"})
        assert status == 200 and json.loads(body)["error"]["code"] == -32600, (status, body)

        status, body = request(port, "POST", "/mcp", "not-json", auth=None)
        assert status == 401, (status, body)

    finally:
        stop(worker, log)

    missing_secret_worker, missing_secret_log = launch(missing_secret_port, empty_env_file)
    try:
        wait_for_worker(missing_secret_worker, missing_secret_port)
        status, body = request(missing_secret_port, "POST", "/mcp", "not-json", auth=None)
        assert status == 503, (status, body)
        error = json.loads(body)
        assert error["code"] == "external/unavailable" and token not in body.decode("utf-8"), error
    finally:
        stop(missing_secret_worker, missing_secret_log)

    log_text = log_path.read_text(encoding="utf-8", errors="replace")
    assert token not in log_text, "MCP bearer token appeared in Wrangler output"
    assert marker not in log_text, "MCP request payload appeared in Wrangler output"

for candidate in (port, missing_secret_port):
    with socket.socket() as probe:
        assert probe.connect_ex(("127.0.0.1", candidate)) != 0, f"Worker port {candidate} remains open"

print("PR-017=worker-mcp,auth-before-parse,fail-closed-secret,host,origin,framing,accept,bounds,protocol,batches,c04-errors,registry,tool-wrappers,replay,safe-logs")
PY
