# D7 - Worker runtime and Axum integration

Status: implemented and locally verified for PR-002.

## Intent

Run the API as a Cloudflare Worker while keeping Rust 1.85, using Axum for HTTP routing, and making
D1, R2, and KV available through the storage layer. Local verification must use Wrangler's local
runtime without an account or production credentials.

## Candidates

| Candidate | Shape | Result |
|:--|:--|:--|
| Worker SDK HTTP/Axum support | Pin `worker` 0.8.3, accept its HTTP request type, and call an Axum router directly. Storage owns binding access. | Selected. The SDK supports Axum and has Rust 1.75 MSRV. Version 0.8.6 requires `wasm-streams 0.6.0` (Rust 1.88), so it conflicts with the workspace's Rust 1.85; 0.8.3 uses `wasm-streams 0.5.0`. Axum's default Tokio/network features are disabled so the Worker builds to wasm32. |
| External `axum-cloudflare-adapter` 0.14 | Convert Worker requests and responses through the adapter, which depends on Worker 0.5. | Rejected. It introduces a second Worker SDK type family beside the workspace's Worker 0.8 dependency and requires extra request/response conversions. |

## Boundaries

- `apps/api` owns the Worker entry point, Axum routes, host policy parser, response errors, and request logs.
- `crates/storage` owns D1, R2, and KV binding operations and returns generic storage failures to the API.
- `wrangler.toml` declares dev and production bindings. The production host list contains only the
  Worker hostname; localhost is scoped to the dev environment. Wrangler 4.149.0 is pinned in the
  local npm lockfile.
- `worker-build` 0.8.3 is installed with its published lock using an auxiliary Rust 1.88 toolchain because the tool's locked transitive crates exceed Rust 1.85. It invokes the workspace build with the repository-pinned Rust 1.85; the application toolchain remains unchanged.
- The local binding probe requires the development environment, a per-run token passed only to
  `wrangler dev --local`, and a POST request. The token is not stored in Wrangler config, so a
  remote `--env dev` deployment does not expose the probe. The smoke test creates its probe table
  in local D1; PR-003 owns the product schema and migrations.
- C-04 registers `authorization/host_not_allowed` as an authorization error with HTTP 403 and
  `not_found/route` as a not-found error with HTTP 404. The Worker applies the configured host
  allowlist before routing and returns the registered envelope for denied hosts and unmatched
  Axum routes. Production and local-denial behavior are exercised by the smoke script.
- A method mismatch on a known Axum path still returns Axum's empty HTTP 405. C-04 has no approved
  code for that condition; `not_found/route` would misstate it. The current scope does not add a
  third code, so this remains a separately authorized contract change.
- The smoke trap starts Wrangler in an isolated process group, verifies it is not the shell's group,
  and asks Wrangler to stop gracefully. Local Wrangler still leaves two defunct `workerd` children
  adopted by this container's PID 1 per run; the PID 1 process does not reap them. The listener is
  closed and no live Wrangler or Worker process remains after exit. No subreaper helper is added.
- The PR does not run a remote deploy. Production IDs and account credentials remain user setup.

## Throughput checkpoint

- **Checkpoint result:** Worker 0.8.3 and the locked dependency graph build under the repository's Rust 1.85 toolchain; `worker-build` 0.8.3 uses auxiliary Rust 1.88.0. The host matcher, production host allow/deny responses, C-04 unmatched-route envelope, and local binding smoke pass. The contract owner approved both C-04 codes for this implementation. A separate known-path method mismatch still returns a raw 405 and needs a future C-04 decision.
- **Independent workstreams:** N/A. Runtime, storage bindings, local smoke tests, and Wrangler configuration share one branch and one lockfile.
- **Shared mutable state:** Serialize Cargo manifest/lockfile, Worker runtime, storage bindings, and Wrangler changes because the interfaces and local binding names must match.
- **Smallest safe decomposition:** One owner handles the feature because the Worker request boundary and D1/R2/KV smoke path must be validated together; the model restriction also keeps implementation in Luna xhigh.

## Verification target

The local smoke run builds the Worker and checks the production host allowlist's accepted hostname
and denied localhost response, then checks the dev `/health` route and a D1/R2/KV
write/read/delete round-trip. It also checks that Axum unmatched routes and the hidden probe return
the registered C-04 404 envelope, while a valid per-run token enables the probe. A separate local
request confirmed that POST to the known GET-only `/health` path still returns an empty 405; that
contract case remains outside the two approved mappings.
`cargo xtask ci` checks the full Rust workspace, API Worker and dashboard WASM targets, and audit.
The API WASM build keeps incompatible Tokio runtime/network features out of the Worker dependency
graph. These results establish implementation behavior only; they do not establish Cloudflare
integration or production readiness.
