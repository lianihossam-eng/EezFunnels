# Eez Funnel - Status

Current phase: PR-002 implementation (`feat/002_worker`)
Last updated: 2026-10-13

## Phases

- [x] PR-000 Bootstrap (merged as PR #1 into `main`)
- [x] PR-001 xtask (merged as PR #2; merge commit `79d85c7`)
- [ ] PR-002 Worker + D1 + R2 + KV (implementation in progress)
- [ ] PR-003 Schema + seed themes
- [ ] PR-004 Domain model
- [ ] PR-005 Render engine
- [ ] PR-006 5 sections
- [ ] PR-007 5 themes
- [ ] PR-008 Dashboard shell
- [ ] PR-009 Funnel list + creation
- [ ] PR-010 Visual editor
- [ ] PR-011 Inspector
- [ ] PR-012 Undo/redo
- [ ] PR-013 Responsive preview
- [ ] PR-014 Publication /f/:slug
- [ ] PR-015 Command registry
- [ ] PR-016 CLI
- [ ] PR-017 MCP server
- [ ] PR-018 Documentation
- [ ] PR-019 Tests E2E + quality
- [ ] PR-020 Release v0.1.0

## Verified locally

Proof status: PR-001 tooling is IMPLEMENTATION_VERIFIED on the merged main commit. PR-002 local
behavior is IMPLEMENTATION_VERIFIED; PR #3 has not yet been opened, so hosted CI is pending. These
results do not establish Cloudflare integration or production readiness.

- `cargo xtask ci` passed on PR-001 and was green in hosted CI before merge.
- The dashboard WASM artifact, workspace checks, audit, and PR-001 CLI behavior are recorded in
  `plan/verification/PR-001/execution.txt` and `results.json`.
- This PR-002 branch starts from merge commit `79d85c7` and was clean before implementation began.
- Node 24.19.0, npm 11.9.0, and Rust/Cargo 1.85.0 are available in the cloud environment.
- Wrangler 4.149.0 requires Node >=22 and has been installed and run locally without Cloudflare
  credentials; the project pins it as a local npm dependency.
- Wrangler's production `ALLOWED_HOSTS` contains only `eezfunnel.workers.dev`; `localhost:8787` is
  configured only under the dev environment. API tests reject an empty correlation ID by replacing
  it with a UUID and preserve a nonempty supplied ID.
- Worker 0.8.3 and worker-build 0.8.3 are pinned. Axum's default runtime/network features are off so
  the API builds to wasm32 under Rust 1.85. The helper binary is compiled with the auxiliary Rust
  1.88 toolchain; application builds still run with the repository's 1.85 toolchain.
- `worker-build --release` and Wrangler's custom build passed. `wrangler d1 migrations apply DB
  --local --env dev` applied the local runtime probe migration, and Wrangler dev exposed DB/R2/KV
  in local mode. `/health` returned `{"status":"ok"}`; `/__local/probe` verified D1, R2, KV
  write/read/delete and returned `ok` for all three with the supplied correlation ID. The probe
  requires a per-run token passed only to `wrangler dev --local` and accepts POST only; smoke checks
  verify it returns 404 for GET and for a missing token.
- `cargo xtask ci` passed in a fresh target directory. Its WASM gates now build both `eez-api` and
  `eez-dashboard`, so adding incompatible Tokio runtime/network features to the Worker breaks CI.
  The run passed fmt, Clippy, workspace tests, layers, both WASM builds, and cargo audit (1,308
  advisories loaded; 105 locked dependencies scanned).
- `bash scripts/smoke-pr002.sh` passed using local Wrangler 4.149.0 and local-only D1/R2/KV bindings.
  The script checks the production host allowlist and denial envelope, `/health`, structured
  unknown-route and hidden-probe 404s, D1/R2/KV round-trip, and the token/method gates.
  Its isolated process-group trap exits without a live listener/process; this container's PID 1 does
  not reap the two defunct `workerd` children Wrangler leaves per run. The evidence is in
  `plan/verification/PR-002/execution.txt` and `results.json`.
- C-04 registers `authorization/host_not_allowed` as HTTP 403 and `not_found/route` as HTTP 404.
  The Worker rejects hosts outside `ALLOWED_HOSTS` before routing and returns a structured
  correlation-ID envelope for denied hosts, unknown Axum routes, and the hidden local probe.
  Wrangler smoke confirms the production host is accepted and `localhost:8787` is denied in the
  production environment.
- A direct request using POST against the GET-only `/health` path still gets Axum's empty HTTP 405.
  C-04 has no approved code for this condition; `not_found/route` would be inaccurate. This is a
  separate contract decision from the two mappings approved for PR-002.
- The C-04 registry tests cover all listed codes, category/status stability, safe storage failures,
  correlation IDs, and the alert marker for unclassified errors.

## Blockers

- No real Cloudflare account binding IDs or credentials are available; the configured UUIDs are
  local placeholders. Remote deployment and production binding checks remain outside this local
  PR-002 run.
- The C-04 invariant that every error has a structured envelope is not yet met for a method mismatch
  on a known path. A new method-not-allowed code requires a separate contract decision; PR-002 does
  not mislabel the condition with `not_found/route`.

## Next action

Open PR #3 after the final local gates and record hosted CI on its current head. Resolve the
method-not-allowed C-04 mapping before broadening runtime error coverage. Production Cloudflare
binding IDs and credentials remain prerequisites for a remote deployment.
