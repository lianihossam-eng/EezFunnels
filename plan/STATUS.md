# Eez Funnel - Status

Current phase: PR-003 implementation (`feat/003_schema`)
Last updated: 2026-10-13

## Phases

- [x] PR-000 Bootstrap (merged as PR #1 into `main`)
- [x] PR-001 xtask (merged as PR #2; merge commit `79d85c7`)
- [x] PR-002 Worker + D1 + R2 + KV (merged as PR #3; merge commit `bc641cad`)
- [ ] PR-003 Schema + seed themes (implementation in progress; no PR opened)
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

Proof status: PR-001 tooling and PR-002 Worker/storage bindings are merged and
implementation-verified locally. PR-003 implementation is locally
IMPLEMENTATION_VERIFIED on its source tree: it adds the product D1 schema, five
seeded themes, typed storage records and repository operations. Its local D1
smoke covers CRUD and immutable revisions, schema-version rejection,
conditional publication, the 100-revision limit, and a 100-way publication
race. Local verification does not establish Cloudflare production readiness.

- Project toolchain is Rust/Cargo 1.85.0; Node 24.19.0, npm 11.9.0, and local
  Wrangler 4.149.0 are available. Wrangler runs with local bindings only for
  the current verification.
- PR-002 was merged at `bc641cad138706c023c254fcfdaaf782b5b7329d`. Its hosted CI
  and Sol review gates were completed before merge.
- The PR-002 evidence contains a pre-existing fingerprint inconsistency:
  all 19 individual source hashes in `results.json` match source commit
  `9006b207b701ec71d6a8121dfdf21c1b5dc4b8b5`; recomputing the sorted
  path/hash lines gives `3b4844681883458eb1086c41a5abddf0b39e3194c515829fb290136a2787c5d3`,
  matching `results.json`. Its `execution.txt` instead records
  `ebbaa869aad93c9a35d3bdb1ccdeff4704eca039290c0f2a6b4979ad3cd56d88`.
  The already-merged PR-002 evidence is unchanged; this discrepancy is recorded
  here rather than silently rewriting historical evidence.
- PR-002 introduced the pinned Worker 0.8.3 / worker-build 0.8.3 runtime and
  local Wrangler smoke. `cargo xtask ci` includes `eez-api` and dashboard WASM
  builds plus pinned cargo-audit; the application remains on Rust 1.85.
- PR-003 is scoped to five product tables (`funnels`, `revisions`, `themes`,
  `assets`, `audit_logs`), seeds `minimal`, `bold`, `elegant`, `playful`, and
  `corporate`, and keeps database row types in storage per D8. No domain-layer
  package or product API behavior is introduced ahead of PR-004.
- The new local-only PR-003 smoke applies both migrations to a fresh D1 store,
  verifies repository round trips and refusal cases, and races 100 conditional
  publications with exactly one winner. It also repeats the PR-002 404/405
  correlation-ID cases (absent, empty, overlength, and supplied) and polls
  Wrangler logs for up to five seconds before asserting the matching
  `api_error` and request records, avoiding a transient log-flush race.
- `scripts/smoke-pr002.sh` remains unchanged. Its final rerun passed the same
  404/405 cases and the D1/R2/KV probe after one earlier transient missing-log
  assertion; PR-003's regression smoke uses the bounded poll described above.

## Blockers

- Cloudflare production binding IDs and credentials are unavailable. No remote
  deployment or production binding check has been run.
- PR-003 has not been opened. Its clean CI, local smoke, and self-audits have
  passed; final evidence and the PR handoff remain to be committed.

## Next action

Commit the PR-003 evidence on `feat/003_schema` and open its PR against `main`.
Sol review and hosted CI must be green on the exact PR head before merge.
Continue with PR-004 after PR-003 is merged.
