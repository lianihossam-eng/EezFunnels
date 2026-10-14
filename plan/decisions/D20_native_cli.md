# D20 - Native CLI boundary and mutation recovery

Status: accepted for PR-016.

## Problem

The `eez` binary is currently a dependency-free help stub. Funnel commands need to use the configured Worker because the Worker adapts REST requests into the existing `Session::execute` dispatcher and D1 repository. Create requires a blueprint and idempotency key. Publish requires both the editor head and the current published pointer, then a compare-and-set request. A network failure after either POST leaves the outcome unknown, so the CLI must recover the exact request without making a later intentional create look like a retry.

## Usage

```sh
eez config set endpoint https://funnels.example.workers.dev
eez funnels list
eez funnels create 'October offer'                 # defaults to tripwire
eez funnels create 'Webinar' --blueprint webinar
eez funnels publish 0a432da2-ef3a-4e82-a9c8-5c89c76b0c10
eez funnels retry 67a5574e-6ed2-475c-aac1-7ec18311aa72

cd /path/to/worker-project
eez deploy --env production
```

Each create or publish invocation is a new operation. Before sending a mutation, `eez` durably saves its endpoint, exact route/body, operation UUID (also the API idempotency key), and creation time. It prints the explicit retry command to stderr. `funnels retry <uuid>` replays that immutable request without reading a new endpoint or refreshing a publish baseline. A later intentional create receives a fresh UUID even when its name and blueprint match an earlier create.

## Shape

Keep a small OS-native argument parser and complete-operation `Funnels` methods in the private CLI. `Funnels` owns the Worker protocol, typed response decoding, publication orchestration, and the only POST path. A separate local-state owner stores endpoint configuration and immutable operation tickets. The process entrypoint owns output, errors, and the Wrangler child process. No host database adapter, Cargo invocation, repository checkout, or public wire-type library is introduced.

Private semantic types include `Endpoint`, `FunnelId`, `RevisionId`, `FunnelName`, `OperationId`, `CreateInput`, `PublishInput`, and a versioned `RetryTicket`. Validate values at argument, configuration, ticket, and response boundaries. Reuse the closed `FunnelBlueprint` enum from `eez-domain`. Keep API DTOs private, decode only the fields the CLI needs, and tolerate additive response fields.

The operation methods expose a deep interface: `list()`, `create(input)`, `publish(id)`, and `retry(operation_id)`. A caller does not coordinate HTTP verbs, keys, or intermediate API reads. `publish(id)` reads `/api/funnels` for the published pointer and `/api/funnels/:id` for `base_revision_id`, then sends one CAS request. If the head is already published it reports that observation without a POST. A stale-pointer response is returned to the user; it never triggers a silent refresh/retry.

Accept only HTTPS origins, with HTTP reserved for literal loopback/local development hosts. Reject credentials, queries, fragments, and non-origin paths, and cap endpoint strings at 2 KiB. Disable redirects and automatic POST retries; use finite timeouts and bounded response reads. Successful editor-read responses have an 8 MiB bound to fit the bounded snapshot after HTML escaping; other successful API responses remain bounded at 2 MiB, and error responses at 16 KiB. A transport failure after POST is reported as an unknown outcome and points to the saved retry operation. Refuse retry tickets at least 23 hours old, before the API's 24-hour receipt retention can expire.

`eez deploy` runs Wrangler in the caller's current directory, forwards OS-native arguments, inherits stdio, and returns the child exit status. It must not invoke Cargo or construct an unsafe shell command. The implementation must handle the Windows npm Wrangler shim safely, with a platform-specific launcher and tests; missing Wrangler should produce an actionable error.

Persist config atomically in the platform user config directory (overridable with `EEZ_CONFIG_DIR` for isolated tests). Config contains no credentials. Enforce the same 16 KiB bound before config writes and reads. Persist each operation in a unique immutable file using no-clobber creation; a crash after the save and before receiving the response remains recoverable. Preserve tickets, including successful ones, while the server receipt can still be replayed; remove tickets at or beyond the 23-hour local retry window when the user retries one or saves a later mutation.

The L4 package may depend on `eez-domain` plus the existing workspace serialization, URL, and UUID dependencies. PR-016 adds `ureq` 2.12 with Rustls TLS, `directories` 6, and `tempfile` 3.23-compatible (the lockfile resolves 3.27.0). Keep the HTTP client synchronous, bounded, and configured not to follow redirects or retry mutations. The selected client has Rust 1.71 MSRV; the temporary-file helper has Rust 1.63 MSRV, both within the workspace's Rust 1.85 toolchain. Record the exact resolution in `Cargo.lock` and verify the full lockfile with `cargo xtask ci`; do not add an async runtime, CLI framework, or Cargo-backed deploy path.

## Synthesis decision

Two structurally distinct candidates were evaluated against the five design criteria. Candidate 1 (complete-operation HTTP client with immutable retry tickets) is the base. Candidate 2 (typed request plans and an interpreter with a fingerprint ledger) supplied the grafts: exact request-interpreter assertions for route/body/key/response limits and explicit fake-Wrangler process tests for working directory, argument forwarding, missing command, and exit propagation.

The cross-judge scored Candidate 1 at 5/5/5/4/4 and Candidate 2 at 4/4/3/4/4. Candidate 1's ticket pins the endpoint and original publish body/baseline across process restarts. Candidate 2's ordinary publish retry recomputes the editor and pointer snapshots, so its fingerprint can identify a different operation and it has no explicit replay path for the original request. Candidate 2's strict unknown-field response decoding was also rejected because additive server fields should not break this client.

## Tradeoffs accepted

- We accept an explicit retry subcommand and small persistent operation files in exchange for exact recovery after ambiguous network outcomes.
- We accept one additional API read before publish in exchange for honoring the existing split between published pointer and editor head.
- We accept a documented `tripwire` default and optional blueprint flag because the frozen create route requires a blueprint.
- We accept a synchronous HTTP client in exchange for a small runtime and a direct call chain for human-operated commands.
- We accept a conservative 23-hour retry window, an explicit system-clock assumption, and deferred local ticket pruning in exchange for avoiding replay after receipt expiry while bounding stale local state.
- We accept separate, safe Wrangler launch handling per platform in exchange for preserving non-UTF-8 arguments and avoiding shell interpretation.

## Alternatives considered

- A typed request-plan/interpreter plus fingerprint ledger hides route construction well, but a repeated publish command refreshes its CAS inputs and cannot replay the exact request after an ambiguous POST. Its extra ledger and locking interfaces also enlarge the internal surface.
- A caller-supplied idempotency key or content-derived key is insufficient: callers would still need to preserve the publish body, and content-derived keys would collapse intentional repeat creates.
- Calling `Session::execute` directly would require a host repository adapter that does not exist and would bypass the Worker boundary. Calling `cargo xtask deploy` would require a checkout and Rust toolchain in the user's project.

## Risks and verification

The implementation verifies redirect handling, bounded success/error bodies, atomic ticket persistence, Windows Wrangler shim selection, lost-response create replay, exact publish CAS replay, stale CAS visibility, config changes that do not redirect tickets, fresh keys for distinct creates, malformed/expired tickets that send nothing, and use of the local Worker from outside the source checkout. The release matrix builds and tests this PR natively on Linux, macOS, and Windows; artifact upload remains tag-only and PR-020 owns published GitHub Release assets.

## Next step

Open PR #17 against `main`, request an exact-head `@codex` review, and merge after hosted CI and review pass. Keep remote database operations, deployment, and cloud environment publication outside this PR.
