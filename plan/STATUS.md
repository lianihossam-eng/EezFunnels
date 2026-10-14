# Eez Funnel - Status

Current phase: PR-008 Dashboard shell (`feat/008_dashboard`)
Last updated: 2026-10-14

## Phases

- [x] PR-000 Bootstrap (merged as PR #1 into `main`)
- [x] PR-001 xtask (merged as PR #2; merge commit `79d85c7`)
- [x] PR-002 Worker + D1 + R2 + KV (merged as PR #3; merge commit `bc641cad`)
- [x] PR-003 Schema + seed themes (merged as PR #4; merge commit `5140163`)
- [x] PR-004 Domain model (merged as PR #5; merge commit `c5961dfc77fb60198882fdd7f5909d4e0326933f`)
- [x] PR-005 Render engine (merged as PR #6; merge commit `2c67843b61c0eb266dee1219565d4e55d59738da`)
- [x] PR-006 5 sections (merged as PR #7; merge commit `96701762c384e47264cdd8092c5f0c3496c7adac`)
- [x] PR-007 5 themes (merged as PR #8; merge commit `7ade40e9ed3cb917d380d762beeb5a01916f1e83`)
- [ ] PR-008 Dashboard shell (implementation complete; local verification passed)
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
run `38431629403` passed on the exact merged PR head, and Sol/@codex reviews were clean. PR-004's
domain implementation and review fixes are verified at source commit
`6da34d09dae2dc0f958a02323c7f025641b86b30`. `Funnel::from_state` restores a published baseline
when hydrating a funnel, and C-04 registers C-01's `validation/schema_version_missing` code as
HTTP 400 with matching API mapping. The domain suite has 10 passing tests; `cargo xtask check` and
clean isolated `cargo xtask ci` passed with zero audit findings across 105 locked dependencies and
1,310 advisories. Hosted CI run `38439241409` passed on exact PR head
`41b5638175678d7349e4ba93600831d40e5fd099`; Sol and @codex reviews were clean on that head. PR #5
was merged as `c5961dfc77fb60198882fdd7f5909d4e0326933f`.

PR-005's sandboxed renderer merged as PR #6 (`2c67843b61c0eb266dee1219565d4e55d59738da`). On
the exact reviewed head `4be5365242558aa4b1ae1a9fcd019be0e60dfc93`, hosted CI run `38446229854`
passed and Sol/@codex reviews were clean. `ThemeTokens` binds a stable content-derived digest to
the immutable normalized token map, CSS filtering matches actual function-token boundaries, and
the deterministic 10,000-input test generator varies fragment sequences and insertion positions.
The render suite passed 12 tests; `cargo xtask check` and fresh-target `cargo xtask ci` passed.
RustSec loaded 1,310 advisories, scanned 113 locked dependencies, and found zero vulnerabilities.
Fingerprinted command/output evidence is in `plan/verification/PR-005/execution.txt` and
`results.json`.

PR-006 implements all five section content renderers. The domain `Section` remains the immutable
id/type/order record; `eez-render` owns the typed content map because C-01 leaves serialization
format out of scope. Rendering requires exact ID coverage and a matching content variant. It HTML-
escapes text and IDs, accepts only HTTPS or same-worker root-relative URLs, limits lists to 50 items,
and caps aggregate section IDs and content strings at 1 MiB. D10 records this boundary and its
relationship to the full serialized-payload limit. Review fixes in source commit
`92086219f005860ff336fbf03251b4fc80486050` bind `RenderManifest::config_hash` to a versioned
SHA-256 of canonical typed section content and reject HTTPS-looking relative spellings without an
explicit `://` authority. Both regressions failed against the prior PR head and pass after the fix.
The renderer suite passes 31 tests, `cargo xtask check` passes, and fresh-target `cargo xtask ci`
passes with 113 audited dependencies and zero findings. PR #7 merged at
`96701762c384e47264cdd8092c5f0c3496c7adac`; the final reviewed PR head was `9f46533`, with hosted
CI green and Sol/@codex reviews clean. Refreshed evidence is in
`plan/verification/PR-006/execution.txt` and `results.json`.

PR-007 added five closed, typed preset themes with stable IDs and JSON token maps, a migration that
updates the existing D1 seed rows, and a storage probe that checks exact catalog parity. `theme.set`
creates an immutable revision with a validated theme digest and renderer-v2; it rejects reuse of the
source revision ID. Renderer-v2 emits a trusted stylesheet that consumes the token set, while
renderer-v1 keeps its unstyled output and version metadata. D11 records the ownership and
compatibility decisions. Test-first regressions cover preset serialization, renderer snapshots,
command behavior, migration parity, renderer-v1 metadata, same-ID revision forks, and the nonempty
renderer-v2 footer selector against its emitted markup. The domain, command, renderer, and storage
suites pass 13, 2, 33, and 5 tests. `cargo xtask check`, fresh-target `cargo xtask ci` on source
commit `c4751dd9e7519c3c80b3cf136685d49673fbdd52`, and the local-only
`scripts/smoke-pr003.sh` all pass. RustSec scanned 113 locked dependencies against 1,310 advisories
with zero findings. Fingerprinted output is in `plan/verification/PR-007/execution.txt` and
`results.json`. PR #8 merged this work at `7ade40e9ed3cb917d380d762beeb5a01916f1e83`. The fix
addresses Sol's review finding that `.footer-links` matched no emitted footer elements; it styles
the existing `.section-footer nav ul` and anchor markup without changing renderer-v1 bytes.

PR-008 adds a Dioxus 0.7.10 shell with Worker-time SSR and same-tree browser hydration for exactly
`/`, `/funnels`, and validated `/funnels/:id` paths. D12 records the presentation-layer boundary,
hydration payload format, pinned CLI, and Rust 1.85/1.88 build split. Unknown API, health, MCP,
probe, and unmatched funnel paths remain with the existing Worker router. `ASSETS` serves the
generated browser bundle through Worker-first routing after the host allowlist; the shell contains
honest empty states until funnel APIs arrive. Five dashboard regressions cover the rendered routes,
invalid and unowned paths, and the nonempty base64 hydration data plus browser bootstrap globals.
`cargo +1.85.0 test -p eez-dashboard --locked` passes 5 tests, `cargo xtask check` passes, and a
fresh-target `cargo xtask ci` passes workspace checks/tests, the API wasm build, the browser wasm
and asset bundle, and audit (344 locked dependencies, 1,310 advisories, zero findings). The local
Wrangler smoke verifies SSR routes, parses the hydration payload, serves all bundle and generated
module assets, checks existing 404 fallback behavior and host denial, and confirms port cleanup.
The browser wasm builds and hydration payload is present, but no headless browser has verified
hydration end to end. Fingerprinted evidence is in
`plan/verification/PR-008/execution.txt` and `results.json`. The first test-first run failed because
`render_document` was not yet implemented; that exact failure is recorded in the execution log.
The final local smoke left no live Wrangler or Worker process; this executor may retain adopted
defunct `workerd` entries because PID 1 does not reap children, as documented in D7. The implementation
is committed at `1d0b741e48c0b637ab33b917d85dfb561b604b2f`; fingerprinted verification evidence is
finalized in a follow-up documentation commit. The branch is ready to push. Hosted CI and @codex/Sol
reviews remain required on the exact pushed PR head; no remote deployment was run.

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
- C-01's `validation/schema_version_missing` is now included in C-04 and mapped by `ApiError` to
  validation/400. Other domain-local diagnostics remain outside the C-04 registry until a later API
  integration defines their transport mapping.

## Next action

Push the PR-008 source and refreshed evidence, verify hosted CI on the final PR #9 head, then request
one @codex review and Sol review on that same head. Do not merge until both reviews and hosted CI
are clean on the exact SHA. Do not deploy remotely.
