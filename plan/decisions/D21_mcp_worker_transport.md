# D21 — Worker-native stateless MCP transport

**Status:** Accepted  
**Date:** 2026-10-14

## Problem

PR-017 plans an RMCP HTTP server at `/mcp`, but the application is a Cloudflare Worker fetch handler rather than a Tokio server. RMCP's `StreamableHttpService` calls `tokio::spawn` in both stateful and stateless request paths. `worker-rs` 0.8.3 drives the handler with `wasm_bindgen_futures::future_to_promise` and does not enter a Tokio runtime. Matching HTTP and Tower types, or compiling to wasm, cannot make that task-spawn requirement safe. The command implementation is also Worker-bound: `execute_funnel_command` builds Worker D1/R2/KV bindings and calls the shared `Session::execute` dispatcher.

The SDK's model-only dependency path also conflicts with the repository's Rust 1.85.0 contract. RMCP 1.4.0 and 1.8.0 both compile unconditional `if let` chains in `model/elicitation_schema.rs`, which Rust 1.85 rejects with E0658. Its model module has no feature that excludes that source. Raising the workspace and hosted CI compiler for one endpoint would weaken the existing toolchain contract, so this decision does not add RMCP as a dependency.

## Usage (caller's view)

An operator sets `EEZ_MCP_TOKEN` as a Worker secret and configures an MCP client for the exact `/mcp` URL with a Bearer token. The endpoint uses stateless Streamable HTTP POST requests with JSON responses. It supports `initialize`, `ping`, `notifications/initialized`, `tools/list`, and `tools/call`; GET returns 405, and the endpoint does not create MCP sessions or SSE streams.

`tools/list` advertises `list_commands`, `execute_command`, `list_funnels`, and `get_funnel`. For example, a client calls `execute_command` with a registered command ID, its JSON parameters, and an idempotency key when required. The MCP caller does not choose the command actor. Retrying a mutation uses the same key and payload.

## Decision

Add a private, stateless MCP handler to `eez-api` and dispatch exact `/mcp` requests from `fetch` after the existing global Host allowlist. Use small private Serde DTOs for only the JSON-RPC and MCP messages this endpoint supports. Do not add RMCP or call its stock `StreamableHttpService` inside the Worker. The DTO adapter handles each request on the Worker event future and returns one bounded JSON response. This is a deliberate dependency deviation from the production-plan RMCP server item, made to preserve Rust 1.85 support and avoid relying on an unavailable Tokio runtime.

The route owns MCP bearer authentication and protocol framing. It reads `EEZ_MCP_TOKEN` only through `Env::secret`, authenticates before reading the body, and fails closed if the server secret is unavailable. Missing or invalid caller credentials receive a generic 401; missing server configuration receives a generic 503. Tokens, request bodies, and tool arguments never enter logs or error bodies. The existing exact `ALLOWED_HOSTS` policy remains the single Host-policy owner; reject repeated Host headers and malformed values before dispatch. If an `Origin` header is supplied, require exactly one syntactically valid HTTP(S) origin with the request's scheme and the allowlisted request host; reject opaque, malformed, repeated, or cross-origin values with 403. Native clients may omit `Origin`. Only the exact `/mcp` path without a query is accepted.

Bound the complete MCP request to 32 KiB and every serialized command `parameters` value to the existing 16 KiB API body limit. Apply that parameter bound to all wrappers, including the generated `funnel_id` object used by `get_funnel`, immediately before command execution. Bound complete responses to 256 KiB. Return explicit safe errors when a bound is exceeded; never truncate a JSON-RPC message. `get_funnel` maps to `funnel.editor.read` and may include its preview HTML, so an oversized response fails as a tool error. It does not read repository rows directly or infer the published pointer.

A single private `ToolSpec` table defines the four MCP tools and is used for both `tools/list` and tool-name lookup. `list_commands` projects `command_registry()` on each request, including all nine executable commands and their actor/idempotency metadata. `execute_command` does not accept an actor. `execute_command`, `list_funnels`, and `get_funnel` all use the existing `execute_funnel_command` adapter, which derives the actor from the registry and invokes `Session::execute`. Session retains command validation, idempotency, replay/conflict classification, publish pointer checks, and mutation ownership.

Support Streamable HTTP protocol versions `2025-03-26` and `2025-06-18`; the older `2024-11-05` transport predates Streamable HTTP. POST requests must include an `Accept` header that accepts both `application/json` and `text/event-stream`; missing or incompatible values receive HTTP 406. Negotiate the initialization body's version when it is in this explicit set, otherwise reply using `2025-06-18`. Accept one `MCP-Protocol-Version` request header only when it matches a supported version; an unsupported or repeated value receives HTTP 400. The initialization response carries the negotiated version, and subsequent JSON-RPC responses echo the supported request header. When a request omits the header and the connection version cannot otherwise be identified, use the Streamable HTTP compatibility default `2025-03-26`. Accept bounded JSON-RPC batches only for `2025-03-26`; `2025-06-18` requests must contain one message. The [2025-03-26 lifecycle specification](https://modelcontextprotocol.io/specification/2025-03-26/basic/lifecycle) forbids `initialize` in a batch. The [Streamable HTTP specification](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports) permits request/notification-only batches or response-only batches, but not mixtures; reject invalid composition before dispatch so no sibling request can execute. A JSON-RPC request ID must be a string or integer. An absent `id` is a notification; an explicit `null` ID is invalid. Notifications and response-only POSTs return HTTP 202 with an empty body. Authentication and HTTP framing failures use HTTP status codes; MCP HTTP errors and command failures carry the complete C-04 `code`, `category`, `message`, and request `correlation_id` fields with safe descriptions; JSON-RPC errors use protocol error messages (including `-32602` for an unknown tool).

## Shape

```text
apps/api/src/lib.rs
  fetch -> request_host_allowed -> exact /mcp -> mcp::handle
  execute_funnel_command -> command_registry -> Session::execute

apps/api/src/mcp.rs (private)
  authenticated bounded HTTP adapter
  private JSON-RPC and MCP request/response DTOs
  one ToolSpec table and MCP method dispatch
  list_commands projection and command tool wrappers

apps/api/src/json.rs (private)
  recursive duplicate-key-safe JSON parsing shared with existing API parsers

apps/api/src/host.rs
  exactly-one Host extraction and exact allowlist comparison

crates/commands/src/registry.rs
  executable command inventory and actor/key metadata

crates/commands/src/funnel.rs
  Session::execute and all user-visible command behavior
```

The public surface inside the API crate is one `mcp::handle(&Env, HttpRequest, &str) -> Response` operation. Protocol, JSON, and Worker types remain private. The handler authenticates first, consumes no more than the request cap, rejects duplicate JSON object keys, parses one supported JSON-RPC message into private DTOs, dispatches a supported method/tool, serializes a complete response DTO, enforces the response cap, and returns through the existing correlation-ID logger.

The tool operation flow is:

```text
execute_command(command_id, parameters, key)
  -> execute_funnel_command
  -> registry-derived actor + request-scoped Bindings/Repository
  -> Session::execute
  -> command handler and receipt/CAS rules
```

`list_funnels` maps to `funnel.list` with `{}`. `get_funnel` maps to `funnel.editor.read` with `funnel_id`. All nine registered command IDs, including the public read command, remain discoverable and callable through the bearer-protected MCP endpoint; the registry determines the actor for each.

## Synthesis decision

Candidate B is the base: it puts auth ordering, bounded parsing, one tool table, and protocol errors behind one private Worker handler while keeping the command registry and `Session` authoritative. Candidate A converged on the same Worker-native shape. Its explicit notification rule (missing ID versus invalid null ID, with 202 and no body) is grafted into this decision. Candidate B's shared duplicate-key JSON parser and real Worker smoke requirements are retained.

Candidate C is the distinct native MCP sidecar alternative. It preserves a stock SDK transport by adding a Tokio service and a signed callback to the Worker command boundary. It is rejected for this release because it adds a second service, backend credentials, a bridge protocol, network availability/latency, and session affinity while the target deployment is a single Worker. It does preserve `Session` as the sole command authority; the rejection is based on topology and operational cost, not command divergence.

## Tradeoffs accepted

- We accept owning and testing the stateless HTTP adapter in `eez-api` in exchange for running under the existing Worker event runtime.
- We accept a private DTO subset that must stay covered by spec-shaped tests and workerd protocol smoke in exchange for preserving the repository's Rust 1.85 baseline and avoiding a Tokio server runtime.
- We accept POST-only JSON responses and no server-initiated SSE/session state in exchange for request-scoped handling that matches the planned tool flow.
- We accept a generic command parameter object and discoverable command metadata in exchange for avoiding a second command schema and dispatcher.
- We accept a 256 KiB response cap and an explicit oversized-result error in exchange for bounded memory and complete protocol frames.
- We accept a small extraction of the duplicate-key JSON parser in exchange for one shared ambiguity policy across HTTP and MCP.

## Alternatives considered

- **RMCP `StreamableHttpService` in the Worker:** rejected because its stateful and stateless paths spawn Tokio tasks, and the Worker fetch entry point does not create a Tokio runtime. A wasm build alone cannot prove runtime safety.
- **RMCP model-only dependency:** rejected because RMCP 1.4.0 and 1.8.0 use Rust syntax unavailable to the pinned Rust 1.85 compiler, including in the unconditional model module. Updating CI and the repository toolchain is out of scope for this endpoint.
- **Native MCP SDK service plus Worker bridge:** rejected because the native process cannot use `worker::Env` or the existing D1/R2/KV bindings. A signed callback preserves `Session` but adds another deployed service, credentials, a bridge protocol, latency, availability coupling, and session-affinity requirements.
- **Direct native storage/command access:** rejected because it requires porting Worker bindings or implementing a second command/session path, risking divergence in actor, idempotency, receipt, and publication behavior.

## Open questions and risks

- The local Wrangler/workerd smoke completes initialize, tools/list, command discovery, funnel reads, creation, and replay. A manual run against the supported Claude Desktop clients remains unverified.
- Is the 256 KiB response cap sufficient for real `funnel.editor.read` previews? If not, define a compact projection through a registered command rather than bypassing `Session`.

## Implementation verification

PR-017 implements the Worker route, private DTO subset, recursive duplicate-key validation, registry-backed tool table, and local Wrangler/workerd smoke. The smoke exercises protocol negotiation and headers, March-version batch handling and June-version single-message handling, acceptance of a supplied same-origin value, case-insensitive Bearer auth, POST `Accept` validation, auth-before-body handling, missing-server-secret behavior, exact Host/query/method/content-type checks, actual request and every-wrapper command-parameter bounds, notifications, duplicate JSON keys, unknown-tool protocol errors, expected HTTP statuses for negative requests, all four C-04 fields on a command error, all nine registry entries, funnel read wrappers, idempotent create replay, and safe logs. Unit tests reject malformed, repeated, whitespace-contaminated, and cross-origin values. Dedicated Rust tests verify exact C-04 envelopes for every MCP HTTP error variant and require invalid batches containing `initialize` or mixed response/request-or-notification items to fail during parsing before dispatch; response-only and request/notification batches remain covered. The local workerd smoke avoids negative framing requests because workerd intermittently reports a lost connection instead of the Worker response.

`cargo fmt --all -- --check`, `cargo xtask check`, `bash scripts/smoke-pr017.sh`, and `cargo xtask ci` pass on the corrected local source. `cargo xtask ci` includes the wasm32 build, existing dashboard and funnel smoke, PR-017 Worker smoke, and RustSec audit. PR #18 is open. Hosted checks passed on exact head `60244b5a22742abedddd7440329bdd6d59248862` before the batch-shape correction; this correction awaits updated hosted checks and independent review. Manual Claude Desktop interop and merge remain pending.

No remote D1 operation, Cloudflare deployment, commit, push, or pull request creation is included in this implementation verification.
