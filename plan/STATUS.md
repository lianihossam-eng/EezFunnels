# Eez Funnel - Status

Current phase: PR-003 fix re-review (`feat/003_schema`)
Last updated: 2026-10-13

## Phases

- [x] PR-000 Bootstrap (merged as PR #1 into `main`)
- [x] PR-001 xtask (merged as PR #2; merge commit `79d85c7`)
- [x] PR-002 Worker + D1 + R2 + KV (merged as PR #3; merge commit `bc641cad`)
- [ ] PR-003 Schema + seed themes (PR #4 open; insert ownership guard and publication regressions fixed locally; final-head CI and Sol review pending)
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
implementation-verified locally. PR-003's product schema, theme seeds, typed storage, and
repository behavior are implementation-verified locally at source commit
`d3e1898f86403bb5c834f2ed736b072440e0d21e`. The insert regression tests were committed first at
`b8c7e968b20cf8bd2267951eeab84f134eb4845d`; the trigger fix follows at `d3e1898f`. Earlier
publication guards are at `35246f2481c7bc9402233b9679e527cda13c3742`. The final fresh-target
`cargo xtask ci`, `cargo xtask check`, local PR-002 and PR-003 Wrangler smokes all passed. Hosted
CI and Sol review on the next pushed PR #4 head remain pending.

- Project toolchain is Rust/Cargo 1.85.0; Node 24.19.0, npm 11.9.0, and local
  Wrangler 4.149.0 are available. Wrangler runs with local bindings only for the current
  verification.
- PR-002 was merged at `bc641cad138706c023c254fcfdaaf782b5b7329d`. Its hosted CI
  and Sol review gates were completed before merge.
- The PR-002 evidence contains a pre-existing fingerprint inconsistency:
  all 19 individual source hashes in `results.json` match source commit
  `9006b207b701ec71d6a8121dfdf21c1b5dc4b8b5`; recomputing the sorted
  path/hash lines gives `3b4844681883458eb1086c41a5abddf0b39e3194c515829fb290136a2787c5d3`,
  matching `results.json`. Its `execution.txt` instead records
  `ebbaa869aad93c9a35d3bdb1ccdeff4704eca039290c0f2a6b4979ad3cd56d88`. The already-merged
  PR-002 evidence is unchanged; this discrepancy is recorded here rather than silently rewriting
  historical evidence.
- PR-002 introduced pinned Worker 0.8.3 / worker-build 0.8.3 and a local Wrangler smoke.
  `cargo xtask ci` includes `eez-api` and dashboard WASM builds plus pinned cargo-audit; the
  application remains on Rust 1.85.
- PR-003 is scoped to five product tables (`funnels`, `revisions`, `themes`, `assets`,
  `audit_logs`), seeds `minimal`, `bold`, `elegant`, `playful`, and `corporate`, and keeps database
  row types in storage per D8. No domain-layer package or product API behavior is introduced
  ahead of PR-004.
- The publication update guard remains one atomic D1 `UPDATE` that requires schema_version 1 and
  a target distinct from the current published pointer while comparing the caller's expected
  baseline. The update ownership guard is now complemented by a `BEFORE INSERT` trigger with the
  same ownership check; NULL published pointers are explicitly allowed. Fresh local D1 smoke
  rejects a cross-funnel pointer insert and confirms the NULL insert succeeds.
- The same D1 smoke rejects unsupported publication without changing the pointer and rejects
  no-op publication. It yields exactly one winner for both a 100-way initial publication race and
  two concurrent candidates against an already-published baseline.
- The PR-003 local smoke also verifies 404/405 correlation IDs (absent, empty, overlength, supplied)
  and polls Wrangler logs for up to five seconds before asserting matching api_error and request
  records, avoiding a transient log-flush race.
- `scripts/smoke-pr002.sh` remains unchanged. Its final rerun passed host denial, health, D1/R2/KV,
  and all 404/405 correlation-ID cases.
- PR-003 is open as PR #4 at
  https://github.com/lianihossam-eng/EezFunnels/pull/4. CI run `38428983108` succeeded on prior
  head `0eeeb3584b30c7741350eca6e2f2ca1d34c5015e`. A new source and evidence head will run hosted CI
  after push; merge requires CI green on that exact head and Sol review.
- A prior measured PR-003 smoke increased adopted defunct `workerd` processes from 58 to 60
  because this container's PID 1 does not reap them. That run left zero live Worker processes and
  closed port 8787; no zombie entries were hidden.

## Blockers

- Hosted CI has not yet passed on the next pushed PR #4 head. Sol must review that exact head after
  CI passes.
- Cloudflare production binding IDs and credentials are unavailable. No remote deployment or
  production binding check has been run.

## Next action

Push the insert-ownership regression test, migration trigger, and refreshed evidence/status to PR
#4. Wait for hosted CI and Sol review on that exact latest head before merge. Continue with PR-004
after PR-003 is merged.
