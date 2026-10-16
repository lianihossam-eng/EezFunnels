# Eez Funnel - Status

Current phase: PR-023 V2 document and bounded transfer decision (save-key and receipt fixes pass full local CI; exact-head review and hosted checks pending)
Last updated: 2026-10-16

## Phases

- [x] PR-000 Bootstrap (merged as PR #1 into `main`)
- [x] PR-001 xtask (merged as PR #2; merge commit `79d85c7`)
- [x] PR-002 Worker + D1 + R2 + KV (merged as PR #3; merge commit `bc641cad`)
- [x] PR-003 Schema + seed themes (merged as PR #4; merge commit `5140163`)
- [x] PR-004 Domain model (merged as PR #5; merge commit `c5961dfc77fb60198882fdd7f5909d4e0326933f`)
- [x] PR-005 Render engine (merged as PR #6; merge commit `2c67843b61c0eb266dee1219565d4e55d59738da`)
- [x] PR-006 5 sections (merged as PR #7; merge commit `96701762c384e47264cdd8092c5f0c3496c7adac`)
- [x] PR-007 5 themes (merged as PR #8; merge commit `7ade40e9ed3cb917d380d762beeb5a01916f1e83`)
- [x] PR-008 Dashboard shell (merged as PR #9; merge commit `13472d2daa8047b166e1a9635434cc48e5b3b62e`)
- [x] PR-009 Funnel list + creation (final reviewed head `61cbfd519b9a5efb55709ae4cf816f5e9a00a042`; hosted CI run `38535569606` passed; Sol/@codex reviews clean, 12 threads resolved; merged as PR #10 at `cdd6c894fa7e3f6171b7d86bc518ce7bf14aaabc`)
- [x] PR-010 Visual editor (merged as PR #11 at `883ee7b98f0203d38654f15d2633010da9e420ee`)
- [x] PR-011 Inspector (merged as PR #12 at `d783e3f3a9e43ff7c74cb245250852cf74073600`)
- [x] PR-012 Undo/redo (PR #13 merged at `a71b1520d86358e38ed84a1ad6cc0b0ea20cd4d8`; hosted CI run `38611264446` passed on reviewed head `9a7944bf81ed66e183ed42369fd0ede7a06cba27`)
- [x] PR-013 Responsive preview (merged into `main` at `41f0df225650c32878dfb14000f7116784aad765`)
- [x] PR-014 Publication /f/:slug (PR #15 merged at `89fc2397233f67f5797bd8736cfd50b41a6b8eab`; hosted CI `38630162138` and exact-head Sol/@codex reviews passed; four review threads resolved)
- [x] PR-015 Command registry (PR #16 merged at `af25dec5e0469e796016906e9210e85f86ca2fd9`)
- [x] PR-016 CLI (merged)
- [x] PR-017 MCP server (PR #18 merged at `dc976f3ec6ffd4b18937cac23748b38b2ef38405`)
- [x] PR-018 Documentation (PR #19 merged at `756511c4aed7521249b65ee98f28d5ad6497a6d7`)
- [x] PR-019 Tests E2E + quality (merged as PR #20 at `f85cf8fe030024066149a0c4e9264bd277d7e9fa`)
- [x] PR-020 Release v0.1.0 (PR #21 merged; GitHub release published)
- [x] PR-021 Shared product boundary and roadmap (merged as GitHub PR #22 at `f747afb6fd0f1f19ef6054dd107e584c3226297f`; no feature implementation)
- [x] PR-053 Project-local skills and workflow (merged as PR #24 at `5ddb6ac1580d155a24ce13671b677dbfdf184489`)
- [x] PR-022 Dashboard publication controls (PR #25 merged at `e3088d370110f60a424a8c311ed2a7b148a8ae23`; exact-head reviews and hosted checks passed)
- [ ] PR-023 V2 document and bounded transfer decision (fresh-key stage recovery, cleanup tombstones, distinct per-stage save receipts, changed-payload rejection, and receipt expiry are modeled; local `cargo xtask ci` passes, exact-head review and hosted checks pending)

## Verified locally

PR-023 defines the immutable V2 document/manifest and bounded staged-transfer proposal without changing frozen C-01 or C-01_config. D22 selects full-snapshot transfer and persistence for initial V2 runtime use. Its 100-pair-per-fixture Node benchmark shows the typed-patch candidate passes local byte-saving, correctness, and p95 gates, but does not establish a production-equivalent winner. The latest 32-fixture run reports p95 transform ratios of 1.7800, 1.7936, 1.8506, and 1.8622 by document size, with all 6,400 candidate executions passing; the artifact also records nine negative fixture checks and three modeled client round trips per save. The lifecycle model checks fresh-key recovery after stale-head conflict/expiry, prior mint-receipt replay, cleanup after replacement save/expiry, retrying with an unresolved earlier upload while preserving both attempt tombstones, each late writer's completion, post-quiescence cleanup, and consecutive saves with distinct stable keys and replayable receipts; it does not prove Worker/D1/R2 runtime behavior. V2 decoding, stage routes, migrations, Cloudflare production readiness, and hosted review remain pending. V2 stage/save commands must stay out of MCP discovery and invocation until a bounded transfer path and a separately named, versioned D21 addendum are reviewed and approved; frozen D21 stays unchanged.

PR-014 adds atomic receipt-backed publish/unpublish and public `GET /f/:slug` rendering from the stored `render_manifest`. The local publication fixes require a newly claimed receipt in the exact D1 batch before pointer mutation, classify missing publish/unpublish fields and public-read slug before typed decode, map unsupported stored schema to HTTP 400, and classify an invalid route against active receipts before returning its route error. D18 records these boundaries; the 16 KiB request cap and C-01/C-04 receipt/CAS behavior remain unchanged. No frozen contract, migration, dependency, lockfile, or renderer changed.

PR-014 adds atomic receipt-backed publish/unpublish and public `GET /f/:slug` rendering from the stored `render_manifest`. Its late-replay, required-field, public-schema, and route-receipt review fixes are in source commit `07a02418eed6e111d1aa221bb07739b4da4df697`; no frozen contract, migration, dependency, lockfile, or renderer changed. Focused publication tests pass 7/7. Both local Worker/D1 replay interleavings and the publication smoke pass; RustSec found zero advisories across 346 locked dependencies. Hosted CI `38630162138` passed on exact head `b8db79f5e929e2fd0f1bc47a7f02901e53fc804d`; Sol and @codex reviews were clean and all four review threads were resolved. PR #15 merged at `89fc2397233f67f5797bd8736cfd50b41a6b8eab`. No remote D1 operation or deployment ran.

PR-015 adds a closed registry for the nine existing executable command IDs. Each entry defines its actor, short description, handler, and idempotency-key policy; the Worker dispatches through that table, and public command IDs are shared constants. Four focused tests cover inventory/handler parity, actors, C-04 rejection, and required/malformed idempotency keys. `cargo xtask check`, `cargo xtask ci`, and the PR-010–013 browser suite pass on the current source; local Worker/D1 smokes cover command create/list/delete and publication flows. No frozen contract, migration, dependency, lockfile, or renderer changed. The original plan's count of 15 has no definitions for six extra commands; D19 records why this PR registers only executable behavior and keeps section edits in `funnel.revision.save`.

PR-016 implements the native `eez` CLI: endpoint configuration, funnel list/create/publish through the Worker API, durable immutable retry tickets for ambiguous mutations, and `wrangler deploy` argument/exit forwarding. `cargo xtask check`, full `cargo xtask ci`, standalone `cargo +1.88.0 audit`, and the isolated local Worker/D1 smoke passed; the CLI has 6 unit and 11 process tests. RustSec scanned 355 locked dependencies against 1,329 advisories with no findings. The release matrix builds and tests natively on Linux, Apple Silicon macOS, and Windows, with artifact upload remaining tag-only. PR #17 checks passed on exact head `32dd5bd7240e37d865a0b7c6b9ee78a690c741ac` (CI run `38641945016`, release matrix run `38641945005`), and `@codex` reviewed that exact head with no major issues. No remote migration, deployment, or cloud environment publication ran. Fingerprinted evidence is in `plan/verification/PR-016/`.

PR-017 implements a Worker-native stateless Streamable HTTP endpoint with case-insensitive Bearer auth, bounded JSON-RPC/tool messages, supplied-Origin checks, the four-tool table, and command dispatch through the registry and `Session`. D21 records why the RMCP dependency is omitted to preserve the Rust 1.85 contract and avoid Tokio in the Worker. March-version JSON-RPC batches accept request/notification-only arrays and response-only arrays; they reject `initialize` and mixed response/request-or-notification shapes before dispatch. June-version requests remain single-message. Unknown tools return `-32602`; MCP HTTP errors and command failures preserve all four C-04 fields (`code`, `category`, `message`, and `correlation_id`). Regression tests assert exact status and C-04 fields for every MCP HTTP error path and prove prohibited batches, including those with a sibling tool call, fail during parsing. `cargo fmt --all -- --check`, `cargo xtask check`, `bash scripts/smoke-pr017.sh`, and `cargo xtask ci` pass on merged product source `a7fa901e448619312c176e99102f94a24857c1c0`. PR #18 merged at `dc976f3ec6ffd4b18937cac23748b38b2ef38405`; hosted CI run `38668412916` and release run `38668412849` passed on exact head `a7fa901e448619312c176e99102f94a24857c1c0`. Codex's exact-head review was clean, Sol's review found no findings, and all review threads were resolved. Claude Desktop interoperability and Cloudflare production setup/deployment remain unverified. No remote D1 operation, deployment, or environment publication ran.

PR-018 documentation merged as PR #19 at `756511c4aed7521249b65ee98f28d5ad6497a6d7`.

PR-019 adds a local-only Playwright create → edit → publish → visit workflow. Dashboard create/edit/save uses keyboard input (41 observed actions, zero pointer events); publication uses the local API because the dashboard has no publish control. Axe-core checks dashboard home, funnel dashboard, and the published funnel. The deterministic JSON fuzz suite exercises 10,000 inputs each through duplicate-key validation, editor/publication command parsing, and MCP message parsing without panics. The isolated local Worker/D1 load check creates and publishes 100 funnels, then makes 1,000 public reads; measured p95 was 9.78 ms (max 40.61 ms), below the 100 ms limit. Signal-aware, idempotent SIGINT/SIGTERM teardown removes the Worker process group, temporary D1 state, and Wrangler config during startup or after readiness; the browser runner closes Chromium before Worker teardown, and its finalizer always stops the Worker if browser close throws. Regressions cover both signal phases and synchronous browser-close failure. `cargo fmt --all -- --check`, `cargo xtask check`, `cargo xtask ci`, the PR-010 migration and browser suites, PR-011/012 browser aliases, and all PR-019 suites pass. Axe reported zero critical findings; the dashboard has one serious color-contrast rule affecting four nodes, recorded in the captured output. No application behavior, contract, migration, dependency, or lockfile changed. Fingerprinted evidence is in `plan/verification/PR-019/`. No remote D1 operation, deployment, or environment publication ran.

PR-020 prepared v0.1.0 release notes and Linux x86_64, macOS Apple Silicon, and Windows x86_64 CLI assets. PR #21 merged at 2026-10-15 03:35:40Z as `3d46ecaf4fc6dd0411ae047841b1001f1ab1f8e6`. Its hosted check run `38676533684` and native release matrix run `38676533683` passed. The `v0.1.0` tag workflow run `38677096705` passed on the merge commit. The GitHub Release was published at 03:38:27Z with `eez-v0.1.0-linux-x86_64`, `eez-v0.1.0-macos-aarch64`, `eez-v0.1.0-windows-x86_64.exe`, and `SHA256SUMS`. Fingerprinted local verification is in `plan/verification/PR-020/`. This release confirms published CLI artifacts only. It does not verify Cloudflare production bindings, deployment, or `PRODUCTION_READY` status.

PR-021 records the user-confirmed shared-product boundary, current release baseline, eight phases, 28 phase pairs, six combined journeys, version register, and canonical Task-ID ledger (`PR-021`–`PR-052`): this documentation task plus 31 future product tasks. The ledger specifies branches, conventional commit subjects, dependencies, UI/API/CLI/MCP surfaces, migrations/compatibility, tests, and evidence gates. Four independent first-pass reviews, four cross-reviews, and four exact-candidate pass-3 reports were integrated. Pass 3 returned `CORRECTIONS_REQUIRED`; the merged candidate added restore-surviving deletion gates, fail-closed domain routing and transfer, explicit path-alias behavior, source/projection/UTM policy, payment/email provider separation, job-detail recovery UX, concrete cost fixtures, immutable frozen-contract handling, and an external private overlay pinned to the public core. Closure: the revised plan passed exact-head PR review and merged as GitHub PR #22 at `f747afb6fd0f1f19ef6054dd107e584c3226297f`. PR-053 records follow-up journey and consent clarifications before feature work begins. Review provenance, P3 submission-versus-Lead arbitration, prototype outputs, report hashes, and limits remain in `plan/verification/PR-021/review-ledger.md`. The DeepSeek share redirected to sign-in; zero prompts were sent, and the user chose local review.
PR-053 installs the pinned Matt Pocock and Cursor pstack skills for this project, maps them to roadmap tasks, and documents the Impeccable pilot gate. Its installer regression fixtures and local checks are fingerprinted in `plan/verification/PR-053/`. PR #24 merged at `5ddb6ac1580d155a24ce13671b677dbfdf184489`; no product behavior or Cloudflare production configuration changed.

PR-022 adds revision-aware publish/unpublish controls in the editor using existing commands and the editor's same-snapshot draft/live pointer read. The UI shows draft and live revisions, the public URL, publication status, stale-pointer conflicts, exact-request retry, and explicit unpublish confirmation. Conflict and post-success refreshes update publication state only, preserving unsaved edits. Exact-head review found that a successful idempotent receipt can return an obsolete publication pointer after another session changes it. The follow-up refreshes the current pointer before enabling another publication action; the browser regression commits and loses a publish response, performs a competing unpublish, retries the exact same key/body, confirms the current state, then saves and republishes the preserved draft. A hosted run exposed that the older PR-010 browser selector no longer matched the accessible reorder name; the selector now tracks `Move up CTA`, retaining the visible-text-in-name Axe requirement. Both local browser suites, `cargo xtask check`, and `cargo xtask ci` pass; Axe reports no serious or critical findings on dashboard home, funnel list, editor, or public funnel. No command, schema migration, dependency, or remote Cloudflare operation was added. Fingerprinted evidence is in `plan/verification/PR-022/`. Updated-head Sol and @codex reviews were clean, all review threads were resolved, hosted CI run `38828457913` and release matrix run `38828457823` passed on final head `a0bb07fba251f8eb63c5c9bd75cb7321697c0a97`, and PR #25 merged at `e3088d370110f60a424a8c311ed2a7b148a8ae23`.

PR-023 specifies the separately versioned V2 document/manifest and bounded staging protocol without changing C-01 or C-01_config. The version-2 reader is not implemented. Source research and architecture challenges confirmed V1 `format_version: 2` is already used for theme overrides, a missing C-01 `schema_version` must stay invalid, and the 100-funnel × 100-revision × 1 MiB snapshot maximum exceeds the published 10 GB paid D1 ceiling before metadata. The 100-pair Node comparison shows a typed-patch candidate passes local fixture gates, but does not select a runtime wire; D22 selects complete-snapshot transfer for initial V2 use until Worker/D1/R2 evidence supports a later decision. Every revision remains a complete immutable snapshot. V2 stage/save commands remain excluded from MCP discovery and invocation pending a bounded transfer design and a separately named, versioned D21 addendum; frozen D21 remains unchanged. `plan/verification/PR-023/research.md` cites first-party standards and provider limits.

PR-010's keyboard announcement and RustSec lockfile fixes were reviewed and merged in PR #11 at
`883ee7b98f0203d38654f15d2633010da9e420ee`. PR-011 implements the closed `settings_schema`
inspector for text, color, select, toggle, and image controls, including preview and V2 persistence
for validated theme overrides. It preserves V1 decoding and order-only requests, strict duplicate
key rejection, C-01/C-04 idempotency/CAS behavior, and the existing 16 KiB API request cap; no
frozen contract, manifest dependency, or migration changed. The duplicate-key HTTP parser and
preview-only renderer fixes address both Sol findings. `cargo xtask check`, `cargo xtask ci`, focused
API/command/render suites, migration preflight, and the five-control browser suite pass. Fresh
interrogate and no-comments audits are clean. PR #12 merged at
`d783e3f3a9e43ff7c74cb245250852cf74073600`. Fingerprinted evidence is in
`plan/verification/PR-011/`. No remote migration, Cloudflare deployment, or merge operation was
performed as part of the PR-011 work.

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
`/`, `/funnels`, and validated `/funnels/:id` paths. The shared route tree and SSR renderer live in
the L1 `eez-dashboard-presentation` crate; the `eez-dashboard` WASM browser entry stays in L4, and
the L4 API depends on the L1 presentation crate. A layer regression rejects a direct API-to-dashboard
edge. SSR emits an outer `#main` hydration container with the Dioxus app tree as its child, matching
`dioxus-web`'s `rootname("main")`; page content keeps its semantic `main` landmark. D12 records this
boundary, the hydration payload format, pinned CLI, Rust 1.85/1.88 build split, and the mount-root
relationship. Unknown API, health, MCP, probe, and unmatched funnel paths remain with the existing
Worker router. `ASSETS` serves the generated browser bundle through Worker-first routing after the
host allowlist; the shell contains honest empty states until funnel APIs arrive. Six presentation
regressions cover routes, invalid and unowned paths, hydration payload/bootstrap globals, and the
mount-container parent relation.

`cargo +1.85.0 test -p eez-dashboard-presentation --locked` passes 6 tests; `cargo xtask check`
passes with API 11, commands 2, presentation 6, domain 13, render 33, storage 5, xtask 26, and CLI
tests 2 and 5. A new empty-target `cargo xtask ci` passes workspace checks/tests, API/browser WASM
builds, asset bundle generation, and audit (345 locked dependencies, 1,310 advisories, zero
findings). The local Wrangler smoke after the final feature split verifies SSR routes, hydration
payload, assets, fallback behavior, and host denial. No headless browser verified hydration
completion, so browser hydration E2E remains unverified.

Codex's first review on pushed head `c5d440c` identified that the app root and hydration mount
container shared `id="main"`. The regression failed before the fix and passes with the outer wrapper
in implementation commit `9e0086371c0b8b15963b47784fb28a6f71b445f7`. A later architecture review on
head `1755fd8` found that classifying the dashboard app in L3 conflicted with the frozen L4 app
boundary. A new layer test failed before correction by accepting an API-to-dashboard edge; commit
`cf05024e0130185648cb83fadf9fdabd70754d63` moved the shared tree/SSR/assets into L1 and restored the
browser app to L4. `AGENTS.md` and `plan/production.md` remain unchanged. Fingerprinted evidence is
in `plan/verification/PR-008/execution.txt` and `results.json`. PR #9 merged the dashboard shell at
`13472d2daa8047b166e1a9635434cc48e5b3b62e`; no remote deployment was run.

PR-009 adds the funnel list, create flow for the five closed blueprints, and idempotent delete. The
Worker exposes `GET/POST /api/funnels` and `DELETE /api/funnels/:id`; command handling stays in
`Session::execute`. D13 records the versioned snapshot format and the initial immutable revision
created atomically with each funnel. Create and delete receipts enforce C-03's 24-hour same-key,
same-payload replay and reject key reuse with different command parameters. The dashboard locks
create fields during pending requests and uses native modal dialogs that cannot be dismissed with
Escape while a mutation is pending. Before create/delete dispatch, it stores only a UUID idempotency
key in `sessionStorage`, in a method/route slot keyed by a SHA-256 digest of the canonical request
tuple. The slot value is only the UUID; user-entered form fields are absent. Ambiguous failures
retain the key for replay; successful responses clear the slot. D13 records this lifecycle. The create
dialog uses the same 120-byte UTF-8 name limit as commands and blocks oversized multibyte names.
Create and delete D1 batches remove expired receipts before claiming a fresh key, so expired records
are reclaimed during ordinary writes. The list uses request generations so a delayed initial load
cannot replace the result of a newer post-create refresh.

D14 sets a 100-funnel workspace cap to bound the all-at-once list response and dashboard rendering.
`funnel.list` applies `LIMIT 100`; create claims its idempotency receipt only below the cap within
the same atomic batch as the funnel and initial revision. At capacity the API returns the decision-scoped
`conflict/funnel_limit_reached` (HTTP 409) without storing a receipt or funnel. A D1 trigger rejects
other over-cap insert paths. Local smoke verifies list truncation, idempotent replay for the 100th
funnel, direct insert rejection, and that deleting a funnel frees a slot. C-01 and C-04 are restored
byte-for-byte to their frozen base content; the cap and error semantics live only in D14 and implementation.

Hosted run `38509655594` failed on evidence head `0a8b265` at the Wrangler process-group check: it
read the background launch process before `setsid` established the server's own group. The startup
poll now tracks the live child whose PID, PGID, and SID match. A second review found shutdown only
watched the launcher after SIGINT; if that launcher exited while a group member remained, cleanup
could miss it. Shutdown polls the verified PGID after SIGINT and SIGTERM. In fallback mode it snapshots owned
PID+start-time identities and uses those retained identities during escalation, so a child reparented
after launcher exit remains within cleanup scope. The red/green regression records a PPID change at
TERM, keeps the child alive after TERM, and confirms KILL removes it. Cleanup fails if port 8787 stays open.

Source commit `10b77f909dcc9074b49086eff7681cf1d181f419` adds the process-group shutdown fix and passes `cargo +1.85.0 xtask check`
with API 11, commands 7, dashboard presentation 13, domain 13, render 33, storage 5, xtask 26, and
CLI tests 2 and 6. A fresh `cargo +1.85.0 xtask ci` invocation on the existing isolated target passes
workspace checks/tests, API and dashboard browser WASM, asset generation, the process-group regression,
local CRUD smoke, and audit of 345 locked dependencies against 1,310 advisories with zero findings;
the target was not empty at invocation start. The separate local Wrangler smoke passes all five
blueprints, create/replay, payload-mismatch conflict, expired-receipt cleanup after create and delete,
delete/replay, validation, unsupported method, not-found, and host-denial checks. The integrated and
standalone runs leave no live Wrangler or workerd processes and close port 8787. Fingerprinted
regression logs and validation evidence are in `plan/verification/PR-009/execution.txt` and
`results.json`.

PR-012 adds bounded JSON-diff undo/redo to the local editor draft, with Ctrl+Z/Ctrl+Y shortcuts.
History applies only to section order and sparse typed settings changes; preview requests, save
captures, idempotency, editor-head CAS, and frozen contracts are unchanged. The latest 100 accepted
edits are retained; successful saves and route loads establish a new history boundary. Decision D17
records these semantics. Source commit `1c7b40b41779a3e57756182818c7de31fcacec50` fixes repeated
identical history announcements by alternating text between two persistent spans in the atomic polite
region. The browser regression first failed on the second same-field undo (expected two live-region
mutations, observed one), then passed for consecutive undo and redo transitions with restored values.
It verifies DOM mutations and text; assistive-technology speech delivery is not verified. The follow-up
history-boundary fix consumes recognized Ctrl+Z/Ctrl+Y in the editor before attempting an app transition,
including empty undo/redo directions; its hydrated regression verifies the input value and input-event
count stay unchanged at both boundaries while ordinary history remains usable. `cargo xtask check`,
`cargo xtask ci`, the local migration harness, presentation tests, and hydrated Worker/browser suite pass,
including the 100 edits / undo 50 / redo 50 regression; the browser suite reports zero critical axe findings.
Hosted run `38598159776` initially failed only at the PR-009 process-cleanup fixture; the supported
failed-job rerun completed successfully on old head `0ac87e5267c4bb6390717100b247be1429ddcd54` in
job `115971746548`. The new boundary-fix head requires fresh hosted CI after push. PR #13 is open at
https://github.com/lianihossam-eng/EezFunnels/pull/13, targeting `main`; Sol review remains pending.
Fresh two-reviewer interrogate found zero actionable findings, and no-comments found no comments,
actionable TODO/FIXME markers, suppressions, or workaround prose. Exact source-fingerprint evidence
is recorded in `plan/verification/PR-012/`.

PR-012 is complete. The final reviewed head `9a7944bf81ed66e183ed42369fd0ede7a06cba27` passed
hosted CI run `38611264446`, including `cargo xtask ci`, migration, and browser coverage. PR #13
merged on main at `a71b1520d86358e38ed84a1ad6cc0b0ea20cd4d8`. The browser regression verifies DOM
mutations and input events; actual screen-reader speech delivery was not tested. No remote D1
operation, Cloudflare deployment, or environment publication was performed.

PR-013's implementation is verified at source commit `d2db41f7dcd94635b89fc183f3313d5e2ec4c7f4`.
The private viewport switcher sets the preview iframe to 375, 768, or 1280 CSS pixels and keeps the
selection outside editor/history/API state. Hydrated browser snapshots measure the actual iframe
and funnel widths, feature-grid columns/card widths, and section order; the narrow dashboard keeps
horizontal overflow inside the preview canvas. `cargo xtask check`, `cargo xtask ci` (including
local smoke and audit), the migration harness, and responsive browser suite pass; axe reports zero
critical findings. Interrogate's redundant duplicate snapshot assertion was removed and the browser
suite rerun. No renderer, API, contract, migration, dependency, or lockfile surface changed. Exact
source and log fingerprints are recorded in `plan/verification/PR-013/`. The browser verifies
responsive geometry and DOM accessibility state; it does not test assistive-technology speech.

The hosted-runner `rg` dependency was replaced with standard `grep` in `dbca0f012e2d2305c4d4219124e6901c3a567773`.
Source commit `350b399` restores C-01 and C-04 to their frozen base content and keeps the 100-funnel
cap/error semantics in D14. Follow-up `6d1c21dd213de3c8efedfd83f487f4c9b19fa5c6` strengthens the
fallback cleanup regression: it records the child PPID at TERM, rejects a marker written before
reparenting, and proves KILL ends the child that remains alive after TERM.

Codex's review on exact head `62ef95b90a29596a4eb2e120106d372b6e02e8da` found that the HTTP
adapter still returned 400 for oversized and invalid-UTF-8 create bodies before receipt lookup.
Source commit `7ba830a7abb52ceee4c4190da41551a3eba93101` sends both unreadable-body paths through
active-receipt classification; smoke asserts 409 for active keys and 400 for fresh or expired keys.

Sol's final-gate review found that an older successful create continuation could clear the pending state of a newer create after the first list refresh finished. Source commit `5c6c939ddf8f35b52e90087c8e5f6159bfe8780b` moves the full response continuation into `finish_create_response`: it awaits and classifies the API result, completes the UI state, clears pending before the independent refresh, and performs no pending-state writes after refresh. Non-201 and transport errors clear pending without refreshing. The regression supplies an HTTP 201 response future, starts a second create while the first refresh is unresolved, and verifies that completing the old refresh preserves the second pending state. Applied to a parent-`592d050` source archive with the old post-refresh reset retained inside the extracted helper, the same test failed at the final assertion (exit 101); it passes on the final source. A second focused test covers non-201 and transport errors.

Exact-source `cargo xtask check`, full `cargo xtask ci`, and independent local Wrangler smoke all pass on `5c6c939ddf8f35b52e90087c8e5f6159bfe8780b` using `/workspace/.eezfunnel-pr009-name-limit-ci`. The API suite has 12 tests and dashboard presentation has 15. CI includes API/dashboard WASM, asset generation, both process-cleanup regressions, CRUD/idempotency/capacity smoke, and audit of 345 dependencies against 1,310 advisories with zero findings. The direct smoke passes five blueprints, active-receipt precedence, fresh/expired validation, delete replay, capacity 100, bounded list, and host denial. It leaves port 8787 closed with no live Wrangler/workerd process. The refreshed source fingerprint is `0d1910b11351978f5122399beb37eae56bef46f9c1419459000c124ca79e46b3` across 110 source files. Fingerprinted logs and WASM/bundle hashes are in `plan/verification/PR-009/execution.txt` and `results.json`. Browser hydration and native Escape behavior remain unverified in a headless browser; no remote migration or deployment ran. Hosted CI run `38534210556` passed on previous exact head `654cd3f1574da914b590ef4a954d25f89e9b723e`; CI for the new source/evidence head is pending. No merge occurred.

Codex follow-up comment `4268403530` on head `654cd3f` found that a list GET begun before successful DELETE could restore the removed funnel. Source commit `ca4ceada2dd090a423bb85ac12b4f13abfc0d118` adds a delete reducer transition that advances the list generation and removes the card, then starts an authoritative GET. The regression `a_list_response_started_before_delete_cannot_restore_the_deleted_funnel` failed before the reducer fix when the stale response restored the item (exit 101) and passes afterward. On the exact source, `cargo xtask check`, full `cargo xtask ci`, and standalone local Wrangler smoke all pass; the dashboard-presentation suite has 16 tests, CI audits 345 dependencies against 1,310 advisories with zero findings, and smoke leaves port 8787 closed with no live Wrangler/workerd process. Fingerprint and logs are in `plan/verification/PR-009/execution.txt` and `results.json`. No merge or remote deployment occurred.

Closure update (2026-10-14; supersedes the older pending-CI and no-merge notes above): final reviewed head `61cbfd519b9a5efb55709ae4cf816f5e9a00a042` passed hosted CI run `38535569606`; Sol and @codex reviews were clean and all 12 review threads were resolved. PR #10 merged at `cdd6c894fa7e3f6171b7d86bc518ce7bf14aaabc`. No remote migration or deployment was run.

PR-010 adds the visual editor, strict stored-snapshot decoding, inert server-rendered previews, and receipt-backed immutable revision saves guarded by an independent editor head. `cargo xtask check` and `cargo xtask ci` pass locally; the dashboard presentation suite has 22 tests. The migration harness proves fail-closed backfill for zero or multiple revisions. The hydrated browser suite verifies pointer and keyboard reorder, preview isolation, save replay, stale-save conflict, a shared-idempotency-key race across funnels, and route changes during a delayed save; axe reports zero critical findings. Review fixes ensure receipt ownership gates every save write and keyboard cancellation cannot invalidate a locked save. Local implementation and integration proof is recorded in `plan/verification/PR-010/execution.txt` and `results.json`. Hosted CI and PR review remain pending. No remote migration or deployment ran.

PR-009's local CRUD smoke validates API behavior and does not execute browser JavaScript. Its
separate browser hydration and native create-dialog Escape checks remained unverified at closure.
PR-010's browser suite now verifies editor hydration and keyboard interaction. No remote binding,
migration, or deployment was used for these local checks.

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
- Claude Desktop interoperability has not been verified.
- The DeepSeek share was not accessible through the available browser path; zero prompts were sent.
  The user explicitly chose local adversarial review, which is the active process and is not blocked
  on DeepSeek.
- Rust Worker/D1 queue runtime, provider behavior, account topology, restore-safe deletion/effect
  evidence, Cloudflare production configuration, and real workload pricing remain unverified or
  `NEEDS_PROTOTYPE`; the roadmap does not claim these results.
- C-01's `validation/schema_version_missing` is now included in C-04 and mapped by `ApiError` to
  validation/400. Other domain-local diagnostics remain outside the C-04 registry until a later API
  integration defines their transport mapping.

## Next action

Await fresh exact-head Sol and `@codex` reviews plus green hosted checks for PR-023. Resolve all review threads before merge. Keep real Cloudflare validation separate until a roadmap task needs configured credentials or resource IDs.
