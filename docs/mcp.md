# MCP setup

Eez Funnel serves an optional stateless Model Context Protocol endpoint at `/mcp`. It uses Streamable HTTP POST requests with JSON responses and requires a Bearer token configured as the Worker secret `EEZ_MCP_TOKEN`.

## Enable the endpoint

From the repository root, set a high-entropy secret in the intended Cloudflare account:

```sh
npx wrangler secret put EEZ_MCP_TOKEN
```

Wrangler prompts for the token. Do not put it in `wrangler.toml`, source control, or a command saved in shell history. The endpoint fails closed with HTTP 503 when the Worker secret is missing and rejects missing or invalid client tokens with HTTP 401.

The endpoint URL is the exact deployed Worker host plus `/mcp`, for example:

```text
https://eezfunnel.<account-subdomain>.workers.dev/mcp
```

The Worker also checks the request `Host` against `ALLOWED_HOSTS`. If a client supplies `Origin`, it must match the request host and scheme. Native clients can omit `Origin`.

## Available tools

The MCP server exposes four tools:

- `list_commands` discovers the nine executable command IDs and their actor and idempotency rules.
- `execute_command` runs a registered command through the shared command session.
- `list_funnels` lists funnels in the workspace.
- `get_funnel` reads an editable funnel and current revision by `funnel_id`.

For example, this `tools/call` request creates a webinar funnel:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "execute_command",
    "arguments": {
      "command_id": "funnel.create",
      "parameters": {"name": "October webinar", "blueprint": "webinar"},
      "idempotency_key": "550e8400-e29b-41d4-a716-446655440000"
    }
  }
}
```

Use the `list_commands` result for the current command inventory and the [Commands reference](commands.md) for parameter shapes. The MCP caller cannot select a command actor; the registry determines it. Mutations require an idempotency key. Reuse the same key and parameters for a retry.

## Claude Desktop

Claude Desktop can launch an HTTP-to-stdio bridge for the remote endpoint when its installed version does not accept a static Bearer header directly. One bridge option is the separately installed npm package `mcp-remote`; it is not a dependency of this repository.

Add an MCP server entry to Claude Desktop's `claude_desktop_config.json` (use the config path for your OS). Replace the URL and token placeholder locally. This config stores the token in clear text, so keep the file private and do not commit or share it.

```json
{
  "mcpServers": {
    "eezfunnel": {
      "command": "npx",
      "args": [
        "--yes",
        "mcp-remote",
        "https://eezfunnel.<account-subdomain>.workers.dev/mcp",
        "--header",
        "Authorization: Bearer ${EEZ_MCP_TOKEN}"
      ],
      "env": {
        "EEZ_MCP_TOKEN": "replace-with-the-same-token-set-on-the-worker"
      }
    }
  }
}
```

Restart Claude Desktop after editing its config. Confirm that the server connects, then try `list_commands` and `list_funnels` before asking it to make changes. The bridge package runs on your computer and must be available through `npx`.

CI also checks the endpoint with the official TypeScript SDK v1.32.1 `Client` and `StreamableHTTPClientTransport` against a local Wrangler Worker. The smoke verifies initialization, `tools/list`, and the read-only `list_commands` call using a throwaway Bearer token. It does not exercise Claude Desktop or the `mcp-remote` bridge, so their end-to-end compatibility remains unverified. If your Desktop version or bridge does not pass the custom Bearer header, use an MCP client that supports remote Streamable HTTP with a static Authorization header.

## Transport details

- The endpoint accepts POST at the exact `/mcp` path; it does not provide GET-based SSE or persistent MCP sessions.
- Send `Content-Type: application/json` and an `Accept` header that allows both `application/json` and `text/event-stream`.
- It supports protocol versions `2025-03-26` and `2025-06-18`. Batch requests are supported only with `2025-03-26`; `2025-06-18` requests contain one JSON-RPC message.
- The complete request is limited to 32 KiB, command parameters to 16 KiB, and the complete response to 256 KiB.
- Command and MCP HTTP errors include the C-04 `code`, `category`, `message`, and `correlation_id` fields. JSON-RPC protocol errors use JSON-RPC error codes.

For a local request check, put a throwaway token in `.dev.vars` beside `wrangler.toml`:

```text
EEZ_MCP_TOKEN=replace-with-a-local-test-token
```

That file is git-ignored. Start the local Worker using [Getting started](getting-started.md), then send `initialize` or `tools/list` to `http://localhost:8787/mcp` with the same token in the Bearer header. Do not use a production token for local development.
