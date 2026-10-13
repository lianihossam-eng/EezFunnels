# Eez Funnel - Status

Current phase: PR-004 domain model (`feat/004_domain`)
Last updated: 2026-10-13

## Phases

- [x] PR-000 Bootstrap (merged as PR #1 into `main`)
- [x] PR-001 xtask (merged as PR #2; merge commit `79d85c7`)
- [x] PR-002 Worker + D1 + R2 + KV (merged as PR #3; merge commit `bc641cad`)
- [x] PR-003 Schema + seed themes (merged as PR #4; merge commit `5140163`)
- [ ] PR-004 Domain model (implementation verified locally; PR #5 preparation in progress)
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

Proof status: PR-001 tooling and PR-002 Worker/storage bindings are merged and locally
implementation-verified. PR-003's product schema, theme seeds, typed storage, and repository
behavior are integration-verified at merged main commit
`5140163ab122d38554b001299faf6b0eb93fd77d` (PR #4 head `fb0ffa53a15d586ce283e2ff3d3cba5670b11f4f`).
The insert regression tests were committed first at `b8c7e968b20cf8bd2267951eeab84f134eb4845d`;
the trigger fix follows at `d3e1898f86403bb5c834f2ed736b072440e0d21e`. The final fresh-target
`cargo xtask ci`, `cargo xtask check`, local PR-002 and PR-003 Wrangler smokes passed; hosted CI
run `38431629403` passed on the exact merged PR head, and Sol/@codex reviews were clean. PR-004
domain implementation is verified at source commit `e0c38979d88923e570020624d91ce7e181b742f9`.
Its final `cargo xtask check` and clean isolated `cargo xtask ci` passed; the domain suite has 8
passing tests, and the audit scanned 105 locked dependencies against 1,310 advisories with no
findings. The exact commands, target path, source hash, TDD observation, and self-audit are in
`plan/verification/PR-004/`.

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
- PR-003 merged as PR #4 at
  https://github.com/lianihossam-eng/EezFunnels/pull/4. Hosted CI run `38431629403` passed on
  head `fb0ffa53a15d586ce283e2ff3d3cba5670b11f4f`; Sol and @codex reviews were clean before merge.
- A prior measured PR-003 smoke increased adopted defunct `workerd` processes from 58 to 60
  because this container's PID 1 does not reap them. That run left zero live Worker processes and
  closed port 8787; no zombie entries were hidden.

## Blockers

- Cloudflare production binding IDs and credentials are unavailable. No remote deployment or
  production binding check has been run.
- The C-04 registry is unchanged in PR-004. C-01's listed domain errors have exact regression
  assertions; domain-local checks outside those listed scenarios are not new C-04 promises. The API
  integration must map domain errors to the approved C-04 registry when it begins consuming them.

## Next action

Commit and push the PR-004 execution evidence/status, open PR #5, then record its exact final head
and hosted CI state. Require hosted CI and Sol review on that same head before merge.
