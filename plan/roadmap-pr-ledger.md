# Implementation PR ledger

Status: PR-021 roadmap merged as GitHub PR #22; PR-022–PR-052 remain proposed product work; PR-053 is the current cross-cutting skills/workflow task. These stable IDs are Task-IDs, not GitHub PR numbers. The merged GitHub PR #21 is the v0.1.0 release. A GitHub number for any future task is assigned only after a branch is pushed and a PR is opened. The ledger is the single current source for branch names, commit subjects, dependencies, interfaces, migrations, UX outcomes, and acceptance checks. Older reviewer reports use provisional IDs and are not normative.

## Delivery workflow and shared rules

For each task, branch from the latest merged `main` in the target repository using the exact branch shown. Keep implementation in one reviewable PR; do not stack dependent unmerged branches unless a documented integration need requires it. Use the listed Conventional Commit subjects in the listed order. Add `Task-ID: PR-0XX` to the body of every commit. Open the PR with title `[PR-0XX] <scope>`, complete the local evidence first, and request exact-head review. Merge only after required hosted checks and review pass. Future GitHub PR numbers are unknown and are not predicted by these Task-IDs.

## Repository ownership

Task-IDs PR-021 through PR-049 and PR-053 target the public EezFunnels repository. PR-049 is a public architecture/topology decision and a synthetic-scope prototype only; it adds no private auth/admin code. PR-050, PR-051, and PR-052 target a separate private overlay repository selected during PR-049. Its exact repository URL is intentionally unknown until that decision. Keep private branches, source, secrets, and build configuration out of the public checkout and its PRs/artifacts. The private PRs pin a released public-core version and test their trusted interfaces against that exact version; PR-052 verifies distribution parity from the private overlay side. Public PRs continue to test and release the common business core without an auth/admin feature flag.

Every implementation PR adds focused behavior tests, input-bound tests, negative actor/scope/privacy tests, migration and compatibility tests for affected data, keyboard/mobile/accessibility checks for affected UI, and local integration checks where the boundary crosses Worker/storage. Public Rust-core PRs run `cargo xtask check` and `cargo xtask ci`; private-overlay PRs run that repository's CI plus interface/compatibility tests against the exact pinned public-core release. Add the relevant browser, API, MCP, CLI, migration, or Worker smoke. A local mock, GitHub release, or SQLite sketch is not Cloudflare production proof. Run no remote Cloudflare operation until the account, resource IDs, credentials, and scope are available; commit no secrets.

Use `Session::execute` and the closed command registry for owner, public, provider, job, and AI-triggered business actions. The actor and resource scope come from a trusted runtime or adapter, never request JSON. Add a command only with its typed parameters/result, actor, errors, idempotency, data effect, and adapter exposure policy. Generic owner MCP must not expose visitor, provider, email-recipient, worker-job, or platform-admin commands. UI, CLI, MCP, and allowed AI use the same owner command semantics. A transport may move bounded bytes, but only a validated command may commit business state. If a large-document or media path does not work in a real MCP client, state the precise limitation instead of claiming full parity.

Current transport ceilings remain independent: editor API JSON request 16 KiB; C-01 serialized funnel 1 MiB; MCP full request 32 KiB, command parameters 16 KiB, and response 256 KiB. Existing editor parsing and settings also have narrower internal caps. Do not raise any limit by implication. Define separate reviewed byte, item, dimension, rate, and retention budgets for large documents, media, forms, events, provider callbacks, email, and AI.

## Trusted actor and adapter boundary

Self hosted v0.1.0 composes one implicit installation-owner `ResourceScope`. Introduce that as a typed trusted argument at command/repository seams as common data features arrive. Never read it from command JSON, a URL field, or a browser-supplied owner ID. Public visitors derive narrow scope from the served published route; provider scope derives only after signature verification; worker jobs derive from a trusted dispatcher; AI receives only the owner scope and capabilities of the ordinary command it is proposing. These are capability distinctions, not platform accounts.

Generic owner MCP exposes only the allowlisted owner surface. It must not expose public visitor intake, provider callbacks, recipient unsubscribe, worker-job execution, or private platform-admin commands just because their IDs exist in the command registry. UI, CLI, MCP, and allowed AI use the same owner command semantics, with per-adapter discovery and pagination limits. Large document/media byte transport may be adapter-specific; the final validated state change still goes through `Session::execute`. Any unsupported surface must be named and bounded, never represented as full parity.

No login, platform-user table, owner session, admin route, or private business feature is added before Phase 8. Whether D1 uses a stored scope column/composite keys, isolated bindings, or a hybrid remains an evidence-led PR-049 topology decision. A scope column alone is not authorization or isolation.

## Adapter, schema, and migration map

This is the intended user-visible surface of every task. “Owner command” means a stable registered command routed through `Session::execute`; read/list operations must be bounded and paginated. Public visitor/provider/worker inputs use dedicated trusted actors and do not become generic owner MCP tools. Schema and migration details are completed under each task and in the version register below.

| Task | Dashboard/UI | HTTP/API and actor | CLI | MCP | Schema, compatibility, or migration boundary |
|:--|:--|:--|:--|:--|:--|
| PR-021 | No product UI change; document shared owner UI boundary | No runtime endpoint change | No CLI change | No MCP change | No migration; docs record current C-01/C-03/C-04, release scope, and proposed future versions without editing frozen contracts. |
| PR-022 | Publish/unpublish, live revision, conflict, URL | Existing owner publication commands | Existing publish behavior | Existing allowed owner commands | No migration; preserve D18 CAS and replay behavior. |
| PR-023 | Document transfer contract, no shipped editor UI | Owner-minted temporary stage plus validated save command | File/stream transport decision | Chunked transfer and bounded reads remain `NEEDS_PROTOTYPE` | New separately versioned V2 document/manifest specification and ADR; preserve frozen C-01 and V1 reader; retain 1 MiB legacy domain cap and separate 16 KiB request cap. |
| PR-024 | Typed section/block compose, history, recovery | Versioned owner preview/save commands | Same owner edits and local draft recovery | Same owner commands with paginated projections; upload/read size limits explicit | V1 remains readable; first V2 edit creates new immutable revision; additive migration only if prototype requires one. |
| PR-025 | Accessible media picker and alt/decorative choice | Owner stage/finalize plus public immutable media fetch; stage bytes are non-authoritative | Same owner media metadata commands; streaming adapter decision | Owner metadata commands; binary upload parity is `NEEDS_PROTOTYPE` | R2 object IDs/digests pin in revisions; additive metadata migration; `/media/*` is distinct from static `/assets/*`. |
| PR-026 | Themes, responsive tokens, compare/restore, a11y | Owner theme/history commands | Same owner edits and revision operations | Same owner commands, bounded history | Preserve D9/D11/D17; no silent change to C-01 revision-retention ceiling. |
| PR-027 | Multi-page navigation, owner route map/breadcrumbs, and route preview | Owner page commands; public read resolves immutable page route | Same owner page commands | Same owner commands; no preview HTML in large responses | C-01 V2 pages/IDs; `/f/:slug` stays V1 root route; additive migration if selected. |
| PR-028 | Form designer and no-collection preview; optional marketing opt-in starts unchecked | Owner form commands; public collection remains disabled | Same owner schema edits | Same owner schema commands | Form schema/consent copy version is inside immutable V2; no submission/PII rows yet. |
| PR-029 | “Leads (soumissions)” inbox, detail, export, delete/redact | Owner-scoped submission reads and privacy commands; capture is still off | Bounded export/delete owner commands | Paginated owner inbox/export/delete only | New business tables are additive; PII lifecycle and restore behavior gate capture; no Contact/CRM Lead. |
| PR-030 | Public form submits to accepted state; owner inbox remains separate | Public HTTP route is the only adapter for registered `form.submission.accept` with `Public` actor; it derives revision/form/scope server-side | No visitor command exposed in owner CLI; owner inbox commands remain separately available | Public visitor command is excluded from owner discovery; owner inbox commands remain separately available | Immutable `FormSubmission` plus replay identity/receipt/outbox signal; exact V2 source revision; event `form.submission.accepted`; collector remains off until PR-029 restore-surviving deletion proof passes. |
| PR-031 | Domain verification and SEO preview/state | Owner config plus public DNS/TLS-observation/render routes | Same owner config/status commands | Same owner config/status commands | Additive domain state; SEO metadata derives from immutable revision and verified host; no remote DNS/TLS promise. |
| PR-032 | Definitions and privacy controls, no collector UI | Contract only | Contract only | Contract only | Versioned event vocabulary, consent absent/accepted/refused behavior, identity, retention/deletion; no runtime schema until implementation decision. |
| PR-033 | Bounded analytics reports and definitions, only after consent policy gate | Public bounded observations; owner report API | Paginated owner reports | Paginated owner reports; no visitor event injection via owner token | Versioned events/projections with revision/page refs; deletion rebuild behavior explicit. |
| PR-034 | Contact/opportunity, explicit conversion, tags/status/attribution | Owner CRM commands and bounded reads | Same owner CRM commands | Same allowed owner commands, bounded | Additive Contact/opportunity/link records; source submissions remain immutable; no platform account identity. |
| PR-035 | Activities, follow-ups, search, saved views, deep links | Owner CRM commands and bounded query endpoints | Same owner actions and paginated search | Same owner actions and bounded search | Additive activities/filter records; closed filter AST and query budgets. |
| PR-036 | Separate payment and email provider/runtime/secret decisions; no effects | Decision/prototype only | Decision/prototype only | Decision/prototype only | No provider feature or schema migration; new versioned contract artifacts/ADRs before dependents; preserve C-01/C-03/C-04/D19/D21. |
| PR-037 | Catalog, immutable offer editor and checkout preview | Owner catalog commands | Same owner catalog commands | Same owner catalog commands | New separately versioned V3 commerce document/manifest specification; preserve frozen C-01 and V1/V2 readers; additive catalog migration. |
| PR-038 | Provider connection, test/live status, safe secret rotation, and signed-event history | Owner connection commands; separate signature-verified callback actor | Owner setup/status only | Owner setup/status only; callback never appears in generic command discovery | Additive connection/inbox state and stable provider event ID beyond 24-hour C-03 receipt. |
| PR-039 | Buyer offer/handoff/return states and owner order view | Public checkout visitor actor; separate owner order reads | Owner order status only | Owner order status only; no visitor checkout capability | Additive order/attempt/receipt/outbox; immutable offer/terms/revision snapshot; provider I/O outside D1 transaction. |
| PR-040 | Order/refund/dispute/reconcile view with exact confirmation | Verified provider callback plus owner reconciliation/refund commands | Same owner reconciliation/refund commands | Same allowed owner commands with exact target confirmation | Additive append-only observations/state projection; unknown state retained; provider event order not assumed. |
| PR-041 | Upsell preview and owner configuration | Public buyer decision actor on exact offer/revision; owner setup commands | Owner setup/status | Owner setup/status; buyer decision not generic owner MCP | Additive acceptance/order item/attempt; stale price and duplicate acceptance rules. |
| PR-042 | Templates, audience preview, campaign review/confirmation, consent/suppression, delivery state | Owner actions; recipient unsubscribe uses separate narrow token actor | Same owner template/delivery actions | Same owner commands; recipient token never exposed as owner tool | Additive template/delivery/consent records; attribution campaign IDs and email campaign IDs are distinct. |
| PR-043 | Rule preview/activate/suspend, job inspect/pause/resume/reconcile | Trusted job dispatcher; bounded owner job commands | Same owner job controls | Same owner controls; worker execution is not generic owner command | Additive versioned rule/job/action/outbox schema; job captures rule version and stable effect ID. |
| PR-044 | AI provider/usage/privacy settings | Bounded provider adapter and owner proposal commands | Same owner proposal/status commands where supported | Same allowed owner commands and caps | Additive bounded provenance/usage; no hidden reasoning; policy/secret/version decision before provider use. |
| PR-045 | Typed proposal diff/select/reject/save; publish remains separate | Owner proposal API; all accepted edits use common commands | Same proposal/acceptance commands | Same owner commands and bounded diff | Proposals pin base revision; accepted edit creates normal immutable revision; no direct DB write. |
| PR-046 | Evidence-linked business summaries and uncertainty | Read-only scoped summary API | Same bounded summary reads | Same bounded summary reads | No new authority; source IDs/event versions and deletion/redaction references preserved. |
| PR-047 | Fresh confirmation, audit, undo/compensate | Normal owner command plus target-bound confirmation | Same command and confirmation contract | Same allowed owner command; platform admin denied | Append-only bounded action provenance; compensation is a new revision/business command, not rollback. |
| PR-048 | Readiness dashboard/report only if useful | Test/recovery endpoints remain non-public or gated | Reproducible operational checks | Verify only allowlisted owner parity | No feature schema; restore and external-effect journal proof before P8. |
| PR-049 | Identity/topology decision prototype; no private code | No auth route; trusted synthetic scope only | Credential migration design only | Credential/MCP policy decision only | No account schema; compare scope storage/isolation using real D1/Worker evidence; specify an external private overlay pinned to a public-core release. |
| PR-050 | Common owner UI remains in public core; private overlay adds hosted session | Private overlay authenticates then injects trusted scope into stable public-core interfaces | Hosted credential flow in private overlay; self-host mode remains public core | Scoped hosted credential in private overlay with owner-only allowlist | Reviewed overlay migration and public-core interface compatibility; no private auth code or env-flag path in public artifacts. |
| PR-051 | Private platform admin in external overlay only | Private admin routes/commands in overlay; no business-data bypass | Admin tooling in overlay only | Generic owner MCP cannot call admin | Private operational/audit records stay out of public core and public PRs; separate from CRM, buyer, order, and funnel schemas. |
| PR-052 | Same public business journeys; only external auth/admin overlay differs | Both distributions pin the same public-core version; private identity endpoints live in overlay | Same common command behavior | Same common owner tools; credentials differ only by hosted scope | Upgrade from v0.1.0; overlay compatibility, migration/rollback and self-host implicit-owner checks; no business-feature fork. |
| PR-053 | No product UI change | No runtime endpoint change | No CLI change | No MCP change | Pinned local skill sources and routing documentation only; no global settings, hooks, runtime dependencies, schema, or product behavior. |

## Data and version register

These are proposed compatibility rules. They do not describe shipped schema support.

| Version axis | Current source fact | Proposed ownership and evolution |
|---|---|---|
| C-01 `schema_version` | Frozen C-01 supports schema version 1; its serialized funnel ceiling is 1 MiB. | Define new separately versioned V2 and V3 document specifications and ADRs without editing C-01. Proposed V2 defines the closed typed page/document shape used later for pages, forms, blocks, theme, SEO, and media references. Proposed V3 adds checkout/offer references. Each reader keeps V1 and prior versions supported until a separate retirement gate. |
| Snapshot `format_version` | Internal snapshot formats V1 and V2 exist; V2 includes theme-override data and is not the multipage/page-block schema. | Keep representation format separate from C-01 semantic schema. Readers must not infer pages or checkout support from `format_version` alone. A first explicit edit creates a new immutable revision and never rewrites the old row. |
| Render manifest | D10 pins renderer and section-content integrity; the current manifest has no explicit `manifest_version` field. | Define `manifest_version` in a new versioned manifest specification and ADR, preserving frozen C-01 and the V1 decoder/default. The V2 manifest pins schema, renderer, page/route data, theme digest, ordered media IDs/digests, and later offer/version digest. |
| Renderer and theme | `RENDERER_VERSION`, D9 theme-token hashes, and D11 preset themes have their own behavior. | Renderer version, theme token digest, schema version, and manifest version remain separate. Old supported revisions render with their pinned or compatible renderer, never silently with changed tokens/assets. |
| Media | R2 binding and an asset metadata table exist; upload, picker, and immutable customer asset flow do not. | Each R2 object uses immutable digest identity. Revision and manifest pin asset ID+digest and contextual alt/decorative choice. Keep bytes for supported/published/restore-referenced revisions. Stage objects are temporary, nonpublic, and collected only after TTL plus a reference recheck. |
| Forms/submissions | No form or submission data exists. | Form definition, fields, consent purpose/text version, and success route belong to the immutable funnel revision. `FormSubmission` rows live separately and pin revision, page, form, form schema, received time, replay identity, and applicable consent evidence. |
| Host bindings and path aliases | No custom-host binding or multi-page route alias exists. | A verified hostname binding has a stable ID and monotonic mapping epoch separate from DNS/TLS proof; cache identity includes this epoch. A renamed published path keeps a versioned 307 alias to its current canonical route, and unpublish removes public routing. |
| Events | No analytics/event collector exists. | Each event has a stable event ID, schema version, source/actor class, `occurred_at`, `received_at`, and immutable source IDs. D1 `FormSubmission` is the sole durable capture authority; `form.submission.accepted` is a versioned non-PII outbox signal carrying an opaque submission ID. `page_view` and `form_start` are separate observations. Contact linkage and attribution rule versions are explicit. Projections dedupe by source ID plus rule version and consult deletion state before dispatch, rebuild, or restore. |
| Commerce | No products, offers, checkout, order, provider event, email, or job state exists. | Offer versions, checkout snapshots, order items, provider event mappings, job definitions, email templates, and AI proposals each have explicit version and source IDs. V3 funnel content pins the exact offer version and terms. External effect identity outlives the 24-hour C-03 mutation receipt. |
| Database migrations | Migrations 0001 through 0006 are current. | Additive migrations only. Assign actual next migration numbers when each implementation branch is prepared. Never edit migration history or invent final migration numbers in this roadmap. Provide v0.1.0 fixtures, migration rerun/rollback/recovery notes, and compatibility checks in each affected PR. |

Frozen files C-01, C-03, C-04, D19, D21, and current configuration are immutable under the repository instructions. New semantics use separately named, versioned contract documents/addenda and ADRs with explicit compatibility tests; old file contents and legacy behavior remain unchanged. C-01 governs the existing document contract, C-03 the existing security/receipt contract, and C-04 the existing error contract. Keep C-03's 24-hour command receipt behavior. Durable business dedupe for submissions, provider events, jobs, or effects is a separate identity and retention policy, not an extension of that receipt.

## Phase 1: close v0.1.0 and settle the shared product boundary

### PR-021: shared product boundary and consolidated roadmap

- Branch: `docs/pr-021-shared-roadmap`.
- Commits: `docs(roadmap): record shared product boundary and implementation plan`; `test(docs): validate roadmap ledger and review evidence`.
- Depends on merged Task-ID PR-020 release v0.1.0; no feature implementation.
- Deliver the current README/status/architecture baseline, historical production-plan correction, 8-phase roadmap, 28-pair matrix, six combined journeys, version and adapter registers, canonical PR-021–052 ledger, review provenance, exact local prototype results, and limits. The P3 submission-only inbox choice is explicitly a local planning arbitration; the global reviewer's Lead 1:1 alternative remains recorded as dissent.
- UI/API/CLI/MCP: documentation only; no runtime surface or command registry change. Keep shared owner behavior and Phase 8-only user/auth/admin boundary explicit.
- Migration/compatibility: none. Describe current C-01/C-03/C-04 and migrations 0001–0006 without editing frozen contracts or claiming new schema support.
- Checks: `git diff --check`; `python3 plan/verification/PR-021/validate-roadmap.py`; run both bounded SQLite prototypes and record exact output/limits; `source .project-tools/env.sh && cargo xtask check`; `source .project-tools/env.sh && cargo xtask ci`. No remote Cloudflare operation.

## Phase 2: complete the shared builder

### PR-022: dashboard publication controls

- Branch: `feat/pr-022-dashboard-publish`.
- Commits: `feat(dashboard): add revision-aware publish controls`; `test(dashboard): cover publication conflict and unpublish`.
- Depends on current PR-014 publication commands and the merged editor.
- UI invokes existing `funnel.publish` and `funnel.unpublish` semantics. Show saved draft revision, live revision, stale pointer conflict, live URL, retry-safe status, and an explicit unpublish confirmation. Preserve unsaved edits on conflict. No new owner command or schema migration.
- Test create → edit → save → publish → public read → unpublish → 404; two stale publish requests; same-key replay and payload mismatch; visible keyboard focus and status announcements. Do not claim immediate CDN purge; D18's authoritative published pointer and revalidation remain the source of truth.

### PR-023: V2 document and bounded transfer decision

- Branch: `docs/pr-023-document-v2`.
- Commits: `docs(schema): specify immutable funnel document v2`; `docs(transport): define bounded document staging`; `test(transport): compare snapshot and patch fixtures`.
- Depends on PR-021 review completion; blocks structural editing.
- Define a separately named, versioned V2 document and manifest specification plus ADR; do not edit frozen C-01 or C-01_config. Define stable page/section/block/form IDs, closed node types, layout/edge rules, supported limits, V1 readers, V2 manifest version and default V1 decoding. V2 may describe pages/forms before their Phase 3 UI ships. Keep the existing 1 MiB legacy storage ceiling unless a future independently governed version explicitly says otherwise. Decide by prototype between a full snapshot and a bounded typed patch. Keep API/MCP command parameters at 16 KiB.
- Candidate large-document path: trusted owner command mints an expiring purpose-bound stage token; binary transport writes temporary bytes only; `funnel.revision.save.v2` validates scope, digest, length, schema, IDs, base revision, and limits before writing an immutable D1 revision under the existing CAS/receipt behavior. Stage alone cannot save or publish. UI/CLI can use file or streaming transports. MCP chunking and bounded read pagination remain `NEEDS_PROTOTYPE`; never truncate.
- Prototype V1/V2 1, 10, 25, and 50-page fixtures at 16 KiB, 128 KiB, 512 KiB, and near 1 MiB. Compare full snapshot vs typed patch with at least 30 local samples per fixture: bytes, round trips, latency p50/p95/max, memory when observable, D1/R2 operations, conflicts, and dated pricing inputs. Test unknown version, duplicate IDs, path collisions, invalid edges, malformed JSON, oversize/depth, wrong digest, and interrupted transfer. This PR establishes a decision; it does not claim runtime or billing readiness.

### PR-024: typed section and block editor

- Branch: `feat/pr-024-typed-editor`.
- Commits: `feat(commands): add revision-safe structural edits`; `feat(editor): compose typed sections and blocks`; `test(editor): cover revision recovery and adapter parity`.
- Depends on the approved PR-023 schema and transfer contract.
- Add real add, duplicate, delete, reorder, and typed block operations. Keep current inspector content/settings and theme edits. Every duplicate gets new IDs. Local edits remain drafts until preview/save. Add predictable focus/selection after delete, undo/redo, compare, and restore-as-new-revision. Register versioned read/preview/save IDs only after their parameters, result, actor, idempotency, errors, and D19/D21 exposure have been reviewed. Final writes still use `Session::execute`.
- Preserve V1 reads and saves. V1's first explicit V2 save creates one new immutable revision; stale base retains a recoverable local draft. UI/CLI/MCP owner actions use the same typed command; MCP returns bounded page projections without preview HTML. Test duplicate/delete graph integrity, invalid node, stale CAS, retry, lost response, reload recovery, wrong-funnel draft, 1 MiB hostile input, adapter parity, keyboard and mobile.

### PR-025: immutable R2 media library

- Branch: `feat/pr-025-media-library`.
- Commits: `feat(storage): add immutable staged media objects`; `feat(commands): register media stage and finalize`; `feat(dashboard): add accessible media picker`; `docs(architecture): separate static and customer media routes`.
- Depends on an image-bearing V2 node from PR-024 and an accepted per-file/aggregate quota decision.
- Keep dashboard build assets at the existing static `ASSETS` path. Reserve a distinct validated public media route such as `/media/<asset-id>/<digest>`; do not reuse `/assets/*` for R2. Separate cache, MIME, CSP, `nosniff`, and access rules. Raw bytes go through a bounded binary staging route; a small registered command validates and finalizes metadata. Keys are generated/content-addressed. Revision stores immutable ID+digest and contextual alt text or decorative choice.
- Reject unsupported signatures/MIME, SVG/script, oversized bytes, excessive dimensions/pixels, decompression bombs, expired or wrong-scope token. Retry staged transfer safely. Garbage collect only expired, unreferenced stages after a fresh D1 reference scan; a concurrent save/publish protects live bytes. Keep media needed by all supported revisions and restore horizon.
- Local Worker/R2-binding prototype: 100 max-sized uploads, save/publish racing GC, invalid type/size/digest, timeout and retry; record bytes, R2 ops, duration, memory if observable, p50/p95/max, and dated pricing inputs. No remote account proof implied.

### PR-026: themes, responsive editing, history, and accessibility

- Branch: `feat/pr-026-theme-accessibility`.
- Commits: `feat(theme): add responsive design token controls`; `feat(editor): add revision history and recovery`; `fix(a11y): correct editor contrast and keyboard states`.
- Depends on PR-024 editor surfaces and PR-025 media picker where integrated.
- Extend the closed preset/token system: palette, type scale, safe font list, spacing/corners/content width/button tokens, and explicit responsive inheritance. No arbitrary CSS, user HTML, or script. Preserve D9 digest integrity and pin renderer/theme versions. Add history, compare, restore as new draft, and explicit retention behavior; never silently prune the C-01 100-revision ceiling or a published/head revision.
- Correct the known serious Axe contrast finding already recorded for four dashboard nodes. Test keyboard-only and pointer flows, visible focus, 320/390/768/1280/1440 CSS px, 200% zoom, reduced motion, empty/error/conflict/recovery states, AA text/non-text pairs, and relevant screen-reader structure. Axe is not evidence of spoken screen-reader output. Run these focused checks in every earlier PR too.
- Acceptance includes zero `serious` or `critical` Axe violations on the targeted dashboard/editor routes and responsive/theme states, including the four already identified contrast nodes. Keep keyboard review and manual screen-reader checks; an Impeccable critique is not a pass signal.

## Phase 3: pages, forms, lead capture, domains, and SEO

### PR-027: multi-page routes and navigation

- Branch: `feat/pr-027-pages`.
- Commits: `feat(pages): add versioned page routes`; `feat(editor): add page navigation and route preview`; `feat(dashboard): add owner route map and navigation breadcrumbs`; `test(pages): preserve V1 public routes`.
- Depends on PR-022 through PR-026; Phase 3 starts only after all Phase 2 tasks merge so page routing and editing build on the completed shared builder.
- Implement V2 pages, stable opaque `page_id`, explicit stable path independent of title, start page, bounded typed navigation edges, page outline, accessible table/diagram, and public route `/f/:funnel_slug/:page_path`. Add an owner route map, route-aware navigation and breadcrumbs in the shared shell; extend it as Submissions, Analytics, Contacts, Orders, and Jobs arrive. Keep private platform-admin routes out of the public shell. Legacy `/f/:slug` remains root/start page and keeps V1 fixtures readable. Page path changes become public only with a saved revision and publish CAS. A published path rename keeps the old path as a versioned same-host `307` alias to the current canonical path, marked `Cache-Control: no-store`; update existing aliases directly on later renames, prohibit loops/chains/collisions, and reserve old aliases until explicitly removed by the owner. Unpublishing removes the page and its aliases from public routing (404). Reserve dashboard, API, MCP, static asset, and media routes. Internal links use page IDs rather than arbitrary URLs.
- Test 1/10/25/50-page routes, collisions/reserved prefixes, missing edges, cycles, unreachable warnings, old→new rename before/after publish, repeated rename without route-registry chains, alias reuse/removal, unpublish, migration/restore, canonical/sitemap output, stale publish, old route/render bytes, page ETag, preview `no-store`, and keyboard/mobile navigation. Record size/latency and no Cloudflare production claim.

### PR-028: typed form definition and preview

- Branch: `feat/pr-028-forms`.
- Commits: `feat(forms): define versioned form schemas`; `feat(editor): preview form states`; `test(forms): cover old schema rendering`.
- Depends on PR-027. Public collection remains disabled.
- Store closed typed form definitions in the V2 immutable revision: form/field IDs, order, type, label/help, bounded validation, required status, purpose and consent copy version, and inline or success-page result. Keep submissions/PII outside the snapshot. No scripts or arbitrary regex. Form consent for handling a request is distinct from purchase authorization and optional marketing consent.
- Test old snapshot decode, field rename/removal, broken page reference, field bounds, no-JS rendering, keyboard/mobile layout, visible validation, status announcements, and draft/live distinction. If optional marketing opt-in is offered, it must start unchecked and remain separate from request-processing consent. Do not collect real data in this PR.

### PR-029: owner submission inbox and privacy controls

- Branch: `feat/pr-029-submission-inbox`.
- Commits: `docs(privacy): define form submission retention`; `feat(storage): add bounded submission records and redaction`; `feat(dashboard): add Leads submission inbox`; `test(submissions): verify export deletion and recovery`.
- Depends on PR-028 and must merge before public capture.
- Provide owner UI “Leads (soumissions)” with list/detail/search, paginated export, explicit delete/redact, retention setting/default, consent evidence, and safe logging. The persistent data entity is `FormSubmission`; there is no Contact profile, email deduplication, CRM opportunity, or platform identity yet. A prospective lead is shown with exact source revision/page/form. Define replay key and payload digest without PII. Before retaining personal data, define and prototype a deletion journal/overlay outside the D1 backup restore path that survives a restore from before deletion, is consulted by every read/export/projection/dispatch, and has retention at least as long as supported backup and replay horizons. Write/acknowledge the durable tombstone before physically deleting D1 PII; readers hide the source as soon as the tombstone is acknowledged, and the UI reports pending until the source deletion is complete. If the overlay cannot acknowledge, fail closed without claiming deletion success. A tombstone contained only in the restored D1 snapshot is insufficient.
- No public endpoint is active until list/export/delete/rate/abuse controls work and the restore-surviving deletion overlay passes a restore-before-delete proof. If no mechanism passes, keep the collector disabled. Test synthetic data absence from queries/exports after deletion; pending/claimed outbox cancellation; stale worker suppression; raw and aggregate views; projection rebuild; restore from before deletion; unknown/expired schema; PII log redaction; export bound; migration rerun; consent purpose; and stale-vs-current form references. Phase 7 repeats the common restore/recovery gate but does not unblock first collection.

### PR-030: idempotent public form capture

- Branch: `feat/pr-030-form-capture`.
- Commits: `feat(forms): accept bounded public submissions`; `feat(api): add scoped form endpoint`; `feat(dashboard): show accepted submission state`; `test(forms): cover retries and stale publication`.
- Depends on PR-027 through PR-029.
- Public request resolves the served published revision and form once, then validates against that exact version. The collector stays inactive unless PR-029's deletion overlay has passed its restore-before-delete proof. Register stable command ID `form.submission.accept` with actor `Public`; the public HTTP capture route is its only allowed adapter. The route derives visitor capability and resource scope; callers cannot choose owner ID, revision, form schema, or actor. It invokes `Session::execute`, which commits D1 `FormSubmission` as the sole durable source of truth plus a durable submission replay identity, existing command receipt/result, and a versioned non-PII `form.submission.accepted` outbox signal containing only opaque source/version IDs in one supported transaction boundary. Receipt/result, logs, and replay identity contain no submitted field values; the receipt stores only the existing permitted digest and the result returns an opaque submission ID. The signal is never a second conversion source. Owner CLI/MCP discovery and invocation must not expose this public visitor command. P3 does not create a Contact or CRM opportunity. The Leads inbox surfaces the accepted submission. No email is sent in P3.
- Preserve C-03's 24-hour generic receipt window and do not edit C-03. Define separate stable submission identity/dedupe retention for retries beyond it in a new versioned event/data policy. Before dispatch, export, and projection rebuild, consult the durable deletion overlay; deletion cancels pending signals, marks claimed work suppressed, and prevents replay from resurrecting data. If the overlay is unavailable or cannot survive restore, keep capture disabled. Define legal/policy retention before enabling real deployments. Test exact size cap/+1, same key and payload, changed payload, timeout before/after commit, stale form, publish/unpublish race, invalid/duplicate JSON, forged field, CSRF/Origin, spam burst, DB error, public read/delete denial, C-04 fields, no-JS accessible confirmation, delete-vs-pending/claim/dispatch/replay, restore-before-delete, and unique projection from submission ID.

### PR-031: verified domains and SEO output

- Branch: `feat/pr-031-domains-seo`.
- Commits: `feat(domains): model verified hostname lifecycle`; `feat(seo): add versioned page metadata and discovery`; `feat(dashboard): configure domain and preview SEO`; `test(domains): cover host transfer and canonical routes`.
- Depends on PR-027. Keep the work as one bounded user flow because canonical and discovery output depend on active verified host state; keep ownership activation and SEO rendering in separate commits with separate fixtures.
- Page title, description, social metadata, index policy, and canonical output derive from the immutable page revision and verified route. Keep current static owner-host routing intact; do not add customer hosts to the global `ALLOWED_HOSTS`. On other hosts, an exact normalized active verified binding resolver serves only approved public funnel/SEO/media route classes; unknown, pending, expired, revoked, or colliding hosts fail closed. Dashboard, `/`, `/api/*`, `/mcp`, admin, and reserved routes never pass through that resolver. Domain mapping, DNS proof, and provider-observed TLS readiness are separate operational state. Require a fresh proof for each exact hostname and transfer. No wildcard without a separate prototype. Never infer ownership from `Host`.
- A transfer first disables the old owner's binding and advances its mapping epoch. The host serves a safe 404/503 state until the new owner supplies fresh exact-host DNS proof and provider-observed TLS is ready; only then can the new binding activate. Cache keys must include host binding ID/epoch, resource scope/funnel, route, published revision, and render manifest. Invalidate/revalidate server-side caches and respect the provider's documented TTL before activation. This cannot revoke bytes already downloaded by a visitor or control provider-managed certificate caches; do not claim instant edge purge or immediate client revocation.
- Before activation, prototype and choose the custom-host root-path experience alongside `/f/:slug`; a root route may only serve a verified public funnel and must never expose the owner dashboard. Test bad/replayed DNS challenge, competing owner claim, exact FQDN/subdomain, delayed/expired TLS, transfer A→B with no old content served after disable, revocation, stale mapping/cache epoch, collision, duplicate/conflicting Host headers, Host spoof, route-class isolation for `/`, `/api/*`, `/mcp`, reserved paths, publish/detach race, cache key/version, canonical alias, noindex preview/draft, sitemap only published/indexable pages, robots privacy, XML escaping, and legacy `/f/:slug`. Do not claim remote DNS/TLS or instant edge purge.

## Phase 4: CRM, analytics, and attribution

### PR-032: event, analytics, and privacy contract

- Branch: `docs/pr-032-event-contract`.
- Commits: `docs(analytics): define event and privacy versions`; `test(analytics): add event and deletion fixtures`.
- Depends on PR-030 submission identity and PR-031 page/domain IDs.
- Specify event schema versions, stable IDs, actor/source, `occurred_at` and `received_at`, bot filtering, consent purposes, idempotency/delay/correction semantics, retention/deletion, per-request and per-owner limits, query budgets, and cost inputs. For `page_view` and `form_start`, define exact behavior for consent absent, accepted, and refused; PR-033 collection stays disabled until this policy is decided and tested. Keep analytics consent distinct from handling a form request and from marketing opt-in. D1 `FormSubmission` alone is authoritative for accepted form conversion; `form.submission.accepted` is a versioned non-PII signal with opaque `submission_id` and source version, never a second count source. `page_view` and `form_start` are separate observation events. Define an honest view/session/conversion vocabulary. Without an explicit tested signal, report observed page renders rather than unique visitors or sessions. No PII in aggregate rows, event payloads, raw query strings, or referrers.
- Allow only `utm_source`, `utm_medium`, `utm_campaign`, `utm_content`, and `utm_term` as input keys, normalize key casing, cap values at 64 UTF-8 bytes, and store a value only when it exactly matches an owner-configured, versioned campaign ID matching `^[a-z0-9][a-z0-9._-]{0,63}$`. PR-034 creates that registry; before then, and for unregistered/invalid values, attribution is `unknown`. Drop all other query keys and referrers before persistence/logging; do not carry visitor IDs, tokens, or sessions across hosts/domains.
- Define projection keys as `(submission_id, projection_rule_version)` and rebuilds that consult the durable deletion overlay before every read, dispatch, or replay. A deleted source cannot reappear through a pending/claimed signal, late delivery, restore, or projection rebuild. Retain dedupe/tombstone evidence for the supported restore and replay horizons. Test delete-vs-pending/claimed dispatch, duplicate/reordered delivery, restore-before-delete, rebuild after deletion, UTM email/token/referrer inputs, consent denied, host change, and unknown attribution. Do not enable analytics collection before policy and limits are decided. No legal compliance claim from a local test.

### PR-033: bounded analytics collection and reports

- Branch: `feat/pr-033-analytics`.
- Commits: `feat(analytics): collect bounded first-party events`; `feat(dashboard): report views and conversions with definitions`; `test(analytics): verify delayed and duplicate events`.
- Depends on PR-032; contact linkage is not required for aggregate metrics.
- Add public bounded event intake and owner dashboard reports from immutable page/revision/form references only after PR-032 decides and tests absent/accepted/refused analytics consent. Draft and preview traffic are separately marked/excluded. A page view is not a session. Form conversions derive from accepted submissions; payment conversions appear only after provider confirmation in P5. Follow the decided consent behavior exactly; do not leave beacon opt-in as an unresolved implementation choice. No cross-domain visitor ID or cookie-free unique-user inference.
- Test JS/cookie disabled and consent denied, bot/robot fixtures, duplicate/delayed/reordered events, ETag/revalidation, timezone/DST/range boundaries, missing denominator, page/revision mismatch, deletion plus projection rebuild, query pagination/budget, PII/token redaction, and report reproducibility. Record local workload measures; account pricing remains separate.

### PR-034: CRM identity, opportunities, and attribution

- Branch: `feat/pr-034-crm-core`.
- Commits: `feat(crm): add contact and opportunity records`; `feat(commands): add explicit submission conversion`; `feat(dashboard): link contacts and opportunities`; `test(crm): cover identity and consent edges`.
- Depends on PR-029/030 and PR-032/033.
- Add opaque `Contact`, contact identifiers, and CRM `Opportunity`/pipeline records distinct from `FormSubmission` and platform accounts. One Contact may have multiple submissions and opportunities; a submission may remain unresolved. An owner command explicitly links or converts. Email is a candidate duplicate signal only. Merge is previewed, idempotent, audited, and reversible. Add a versioned owner-configured campaign-ID registry; allow attribution only for exact registered IDs under PR-032's allowlist, otherwise show `unknown`. Keep status/tags, activity timeline, consent-purpose history, source revision, campaign evidence, and versioned attribution rules. Do not infer CRM identity from a buyer email or form submission.
- Test shared family email, missing email, case/Unicode normalization, duplicate/merge/undo, owner-only access, explicit conversion replay, invalid/unregistered campaign IDs including email/token samples, conflicting/unknown attribution, multi-submission contact, consent withdrawal, export/delete, event-source preservation, and PII removal from analytics. No contact record is a platform account.

### PR-035: CRM activities, follow-ups, search, and saved views

- Branch: `feat/pr-035-crm-operations`.
- Commits: `feat(crm): add activities and owner follow-ups`; `feat(crm): add bounded saved filters`; `feat(dashboard): preserve CRM navigation context`; `test(crm): cover segments and deep links`.
- Depends on PR-034 and PR-033 for linked analytics.
- Add owner notes/activities, due follow-ups, bounded search, stable pagination, and a closed typed saved-filter AST. A configurable multi-pipeline editor is a later decision; the initial pipeline remains bounded. Deep links connect funnel, submission, event, contact, opportunity, and later order while preserving date, filter, sort, focus, and return context. This is CRM UI/workflow, not the durable Phase 5 automation engine.
- Test deleted/stale tags or stages, filter injection and query bounds, pagination under mutation, timezone/due dates, deep link reload/back, keyboard/mobile/focus, empty/error/conflict states, export permissions, and CRM deletion without broken analytics.

## Phase 5: payments inside funnels, orders, email, and automations

### PR-036: payment and email provider, secret, and durable runtime decisions

- Branch: `docs/pr-036-commerce-decisions`.
- Commits: `docs(commerce): compare payment and email providers`; `docs(secrets): choose credential setup and rotation`; `docs(runtime): compare durable work options`; `test(commerce): record bounded SQLite prototype`.
- Depends on PR-032 through PR-035 and their accepted data/privacy/identity boundaries. This decision PR ships no checkout/provider feature.
- Make separate payment-provider and email-provider decisions here. PR-036 is the sole task that selects both providers. Payment criteria cover jurisdiction, fees, taxes, signed callbacks, idempotency window, lookup/reconciliation, refunds/disputes and test tooling; email criteria cover sender-domain verification/authentication/reputation, acceptance/delivery/bounce/complaint events, suppression, provider lookup/idempotency, rate limits, pricing, retention and test tooling. PR-042 configures sender identity and proves readiness for the selected email provider; it does not select a second provider. Decide how owners configure/rotate both secret classes without echo or plaintext in D1. Compare D1 outbox plus Cloudflare Queue, Workflows, and a scheduled Rust pump against pinned `worker-rs 0.8.3`; test current Wrangler/local fidelity and quotas before choosing. Define any new semantics in separately versioned contract documents/addenda and ADRs; preserve C-01/C-03/C-04/D19/D21 and existing configuration unchanged. No new dependency before decision.
- SQLite model outputs in `plan/verification/PR-021/prototypes/` are semantics illustrations only. PR-036 chooses the payment provider; PR-037 opens with a spec-only V3 commerce document/manifest and ADR commit. Review and accept that spec before adding any PR-037 domain, storage, command, or dashboard implementation commit. Gate all PR-037 implementation on both the accepted V3 specification and payment-provider choice. Gate PR-039/043 on a viable Rust Worker job prototype. Gate PR-042 real delivery on the separately recorded email-provider/sender policy, verified sender-domain state, visible provider/domain health, bounded send rates with pause/suppression behavior, a viable Rust Worker durable-intent prototype, and an authorized test-account check. Local email tests do not send. If a provider/runtime is unclear, record `NEEDS_PROTOTYPE` and stop the dependent implementation.

### PR-037: product catalog and immutable offer versions

- Branch: `feat/pr-037-offers`.
- Commits: `docs(schema): define versioned checkout V3 specification`; `feat(domain): define immutable offer versions`; `feat(storage): add catalog migration`; `feat(commands): register offer operations`; `feat(dashboard): edit and preview offers`.
- Depends on PR-027 page/document model and PR-036 reviewed decisions.
- Open with the spec-only V3 checkout document/manifest and ADR commit. Review and accept that commit before adding any implementation commit to PR-037. Then create product and immutable `offer_version_id` records; validate currency and integer minor-unit prices; define tax/shipping/terms state. The accepted separately versioned V3 spec defines proposed checkout references and manifest digest; do not edit frozen C-01. V1/V2 readers remain available. A funnel page pins an offer version and displays the captured terms. Catalog edits never alter a published page. No card data or provider secrets in revisions. After implementation, rerun Sol and `@codex` review and hosted checks against the final exact PR head; the intermediate spec acceptance is not a substitute.
- Test old V1/V2 render, old page price after catalog edit, new revision preview/publish, invalid currency/overflow amount, missing/stale offer version, migration from v0.1.0 and immutable snapshot hash. A hosted provider price must be reconciled with exact displayed totals before checkout can proceed.

### PR-038: provider connection and signed event inbox

- Branch: `feat/pr-038-provider-inbox`.
- Commits: `feat(commerce): add scoped provider connection`; `feat(dashboard): configure provider and rotate credentials`; `feat(api): verify signed provider events`; `feat(storage): dedupe provider event inbox`; `test(commerce): cover signature and replay`.
- Depends on provider/secret choice in PR-036 and offer model in PR-037.
- Store nonsecret provider connection metadata and secret reference/status. Give the owner explicit test/live connection status, test-connection feedback, and secret rotation that never re-reads or echoes plaintext. Separate connection readiness from callback signature verification. Verify bounded raw-body signature before parsing. Runtime derives a narrow verified-provider capability scoped to its connection. Dedupe `(connection_id, provider_event_id)` durably beyond generic 24-hour receipts. Keep event and receive time; use provider object versions only when documented. Callback records evidence and does not directly mark an order paid.
- Test valid/altered/expired signature, secret rotation, replay, same event ID on another connection, oversized/unknown/malformed event, unknown connection, safe error/logging, no generic MCP exposure, and provider fixture mapping. Real provider behavior remains unverified until an authorized test account is used.

### PR-039: checkout intent, order, payment attempt, and outbox

- Branch: `feat/pr-039-checkout`.
- Commits: `feat(domain): model checkout and payment attempts`; `feat(storage): commit order and intent atomically`; `feat(api): add scoped public checkout`; `feat(checkout): render buyer handoff and return states`; `feat(dashboard): inspect orders and pending state`; `test(checkout): inject timeout and replay failures`.
- Depends on PR-036 runtime proof and PR-037/038 contracts.
- Public checkout route derives a visitor capability from one exact published revision, page, offer version, amount/currency, and consent. Buyer UI shows the exact offer and terms, provider handoff, pending return, and safe failure/retry states. A browser return never proves payment. Commit `Order`, immutable `OrderItem` snapshot, `PaymentAttempt`, receipt, and durable outbox intent before any provider call. Network call occurs outside D1 transaction. Record stable merchant/effect IDs. Unknown provider result stays `unknown` until verified callback or lookup. No public order reads; no platform SaaS billing.
- Test same-key and changed-payload retries, stale/unpublished offer, concurrent checkout, crash before/after D1 commit, provider accepted/response lost, callback duplication, request body/rate cap, no secret/PII logs, no card data in funnel JSON, and no second charge. Run Rust Worker/D1 integration only after PR-036 runtime choice.

### PR-040: order lifecycle, refund, dispute, and reconciliation

- Branch: `feat/pr-040-order-reconciliation`.
- Commits: `feat(commerce): model payment order refund and dispute states`; `feat(storage): append provider observations`; `feat(dashboard): inspect and reconcile orders`; `test(commerce): cover out of order provider history`.
- Depends on PR-038/039.
- Keep order, payment, refund, and dispute state distinct. Record append-only verified provider observations and last reconciled local projection. Owner `refund.request` includes exact amount/currency/target and a confirmation tied to payload. Ambiguous outcome remains `unknown`; reconciliation uses provider-specific lookup or manual review.
- Test duplicate and out-of-order callbacks, no-version providers, partial/full refund, dispute after refund, lost response, retry after receipt expiry, cross-order ID, unverified callbacks, reconcile mismatch, retention/export/deletion policy, and provider result vs local intent.

### PR-041: upsells and order bumps

- Branch: `feat/pr-041-upsells`.
- Commits: `feat(domain): define consented upsell snapshots`; `feat(commands): accept scoped upsell`; `feat(dashboard): preview order bumps`; `test(upsell): cover stale offer and retries`.
- Depends on PR-027 and PR-037 through PR-040.
- Model the displayed upsell offer version and affirmative buyer decision. Record separate order item or linked order, source revision, consent evidence, and payment attempt. Do not reuse a payment token/authorization unless the chosen provider contract explicitly supports it.
- Emit one versioned `upsell.decision.recorded` event with opaque order/offer IDs and `accepted` or `declined`; this records the buyer decision, not a paid state. `order.paid` is emitted only from verified provider confirmation. PR-042/043 may consume these distinct events under their own allowlisted rules.
- Test decline creates no new attempt; duplicate accept creates one child purchase and one `upsell.decision.recorded` source event; an accepted decision is not represented as `order.paid`; only verified provider confirmation emits paid state; price change invalidates stale view; timeout/reconcile remains explicit; mobile price/consent is visible; no implicit extra charge.

### PR-042: consented transactional email and campaigns

- Branch: `feat/pr-042-email`.
- Commits: `docs(email): record sender and deliverability setup`; `feat(email): add templates suppression and owner health view`; `feat(dashboard): review and confirm email campaigns`; `feat(storage): persist delivery intents and attempts`; `test(email): verify consent deliverability and replay`.
- Depends on PR-034 CRM consent/campaign registry, PR-036 separate email provider and secret decision, PR-039/040 order events, PR-041 upsell outcomes, and the chosen durable intent path.
- Configure sender identity, authentication, health, event mapping, retention, consent purpose, bounded rates, suppression, and pause thresholds for the email provider selected in PR-036; do not repeat provider selection. Keep transactional and marketing purposes separate. Distinguish the PR-034 attribution `campaign_id` registry from email campaign IDs. Give owners a campaign flow to select an eligible audience, preview recipients/content, inspect current consent/suppression, explicitly confirm, and review dispatch results. Maintain unsubscribe, bounce, complaint, provider acceptance/delivery/unknown states, and durable send/action IDs. Never infer marketing permission from form processing or purchase. Before every dispatch/retry, check current consent and PR-029 deletion overlay; delete/redact cancels pending sends and prevents an old outbox signal or restore from reintroducing a recipient. Show provider/domain health and stop or suppress dispatch on failed sender verification, sustained throttling, or configured bounce/complaint thresholds. No live send until PR-036's provider decision, verified sender setup, viable Rust Worker durable-intent prototype, and an authorized provider test-account check all pass. Local tests never deliver real messages.
- Test campaign audience preview vs dispatch, revocation between preview and send, duplicate intent, provider accepts then response lost, resend only with provider-safe evidence, sender authentication/reputation failure, rate throttling, pause and suppression, bounce/complaint, template edit after queue, PII redaction, restore-before-delete, and that `upsell.decision.recorded` cannot trigger a paid-order email without `order.paid` verification. Test accessible preference state and retention/export/delete rules. Extend deletion/dedupe retention before enabling a provider whose replay horizon exceeds the proven journal horizon.

### PR-043: bounded automations and durable jobs

- Branch: `feat/pr-043-automations`.
- Commits: `feat(automation): define versioned allowlisted rules`; `feat(storage): add durable jobs and action attempts`; `feat(commands): dispatch bounded job actions`; `feat(dashboard): preview and activate rules`; `feat(dashboard): inspect and resume jobs`; `test(automation): cover restart and dead letter`.
- Depends on PR-036 runtime prototype, PR-032 event contract, PR-034/035 CRM, PR-040/041 order and upsell events, and PR-042 email states.
- D1 intent and job/action IDs are authoritative. Queue/Workflow/pump is execution machinery. Rules are typed/versioned trigger, conditions, and allowlisted actions. The owner can configure, preview without effects, explicitly activate, suspend, and version rules. Define historical-event replay as off by default; replay requires a bounded explicit owner action. Publish a closed event × action × actor × scope × replay matrix. Initial triggers are accepted `form.submission.accepted` and provider-confirmed `order.paid`; `upsell.decision.recorded` may at most create an owner follow-up and can never mean paid. V1 actions are limited to the existing CRM follow-up command and approved email templates with current purpose/consent checks; marketing campaigns remain explicit owner actions, not automatic fan-out. No scripts, arbitrary URLs, SQL, charge/refund, publish, delete, offer changes, or AI-generated email content. Exclude `page_view`, `form_start`, pending payments, and browser success signals from commercial effect triggers. Bound bytes, concurrency, duration, retries, storage, and per-owner usage. Every job captures the rule version. Before every dispatch, retry, or rebuild, check the durable deletion overlay and current source state; a deleted source is suppressed. UI shows queued/running/retry scheduled/unknown/reconciliation/suppressed/complete/dead-letter and next action. Job detail links the source event, rule version, affected funnel/submission/contact/order/email record, and each action attempt; it shows a safe error/correlation ID and one explicit next action: `retry safe`, `reconcile first`, or `owner decision`. The detail preserves deep-link return context and omits raw PII and secrets.
- Inject crash before/after state commits, lost acknowledgement, duplicate/out-of-order event, expired lease, provider outage, poison action, quota exhaustion, rule edit while running, worker restart, and restore. Test the event/action/actor/scope/replay matrix with negative cases for owner MCP, visitor, provider callback, and worker. Verify separate decision and payment events, and that an `order.paid` action requires verified provider confirmation. Unknown external effects do not retry unless provider proves safety. Test deep-link reload/back, keyboard/mobile, deleted source, attempt history, safe error detail, and the three retry/reconcile/owner-action states. No arbitrary script, SQL, or unbounded retry.

## Phase 6: AI through shared commands

### PR-044: AI provider, privacy, secret, and usage boundary

- Branch: `feat/pr-044-ai-boundary`.
- Commits: `docs(ai): choose provider and data boundary`; `feat(ai): add bounded proposal client`; `feat(storage): record usage and provenance`; `test(ai): cover scope and output limits`.
- Depends on PR-022 through PR-043 command and privacy contracts.
- Decide provider, BYOK/default behavior, explicit opt-in, data egress, retention/deletion, secret setup, model/version provenance, input/output size, timeout/retry/token and spend caps. Store bounded proposal metadata and accepted source IDs, never hidden reasoning. No direct DB or new provider dependency without an ADR.
- Test malformed/oversized/slow/replayed provider response, injected text in pages/contact notes, PII/secret filtering, owner scope, cost cap, safe error/logs, and provider timeout. No hosted provider claim from a mock.

### PR-045: funnel and page copilot proposals

- Branch: `feat/pr-045-funnel-copilot`.
- Commits: `feat(ai): generate typed funnel proposals`; `feat(dashboard): review selected field diffs`; `test(ai): cover stale revisions and partial acceptance`.
- Depends on PR-023/024/027/028/037 and PR-044.
- AI returns a typed proposal against a named funnel/revision. Owner previews and selects changes. Accepted edits use the normal revision save command; publication remains a separate owner action. Price, checkout, email, or payment changes are proposals only and require ordinary owner confirmation.
- Test invalid output, prompt injection, stale base CAS, partial apply, reject, repeat request, no direct DB write, one accepted revision, no publish/charge/send/refund by AI, and same validation/error behavior as UI/CLI/MCP.

### PR-046: evidence-linked business summaries and workflow suggestions

- Branch: `feat/pr-046-business-copilot`.
- Commits: `feat(ai): add scoped business summaries`; `feat(dashboard): show evidence and uncertainty`; `test(ai): verify deletion and source scope`.
- Depends on PR-032 through PR-043 and PR-044.
- Read only, minimized context for approved aggregates. Show event/contact/order/job source IDs, rule version, time window, unknown/conflicting data, and consent constraints. Suggested automation edits remain typed drafts and do not run.
- Test fixture reconciliation, delayed/duplicate events, deletion/redaction, prompt injection, changed evidence, model timeout, and no permission expansion or external action.

### PR-047: approvals, audit, and compensating actions

- Branch: `feat/pr-047-ai-approval-audit`.
- Commits: `feat(ai): apply reviewed command proposals`; `feat(ai): record bounded action provenance`; `feat(dashboard): add confirmation and history`; `test(ai): verify compensating behavior`.
- Depends on PR-045/046 and final target command contracts.
- Preserve proposal, accepted diff, command IDs, actor/scope, provider/model/version, safe input references, usage, and result. Undo creates a compensating revision or explicit inverse business command; it does not erase history. Charge, refund, send, delete, and publish require the normal command and a fresh confirmation tied to exact target/amount/content. No admin capability.
- Test stale approval, repeated apply, changed proposal, irreversible action confirmation, external action unknown, undo after send/charge, audit retention, and owner-only adapter exposure. Do not store chain-of-thought.

## Phase 7: stabilization and release evidence

### PR-048: common-product reliability, recovery, and cost evidence

- Branch: `test/pr-048-common-product-readiness`.
- Commits: `test(recovery): rehearse migration backup and restore`; `docs(operations): record workload and cost evidence`; `test(readiness): verify common product release gates`.
- Depends on PR-022 through PR-047 feature completion. Product fixes are separate reviewed PRs with their normal checks.
- Rehearse migration from v0.1.0 and supported prior schemas; back up/restore D1 plus immutable R2 references; invalidate/rebuild only caches that the selected implementation actually uses; apply the PR-029 deletion overlay/journal before export, projection rebuild, or outbox dispatch; pause dispatch; reconcile remote payment/email/AI effects; resume only known-safe work. List external effects that cannot be rolled back. P8 is blocked until shared product gates pass.
- Add an integrated checkout recovery journey: provider accepts then the caller loses the response; buyer retry; accepted/declined upsell; verified callback; refund; email already queued; deletion/consent overlay and restore; no duplicate charge/send and no blind retry of `unknown`. Add an AI journey with a stale proposal, rejection then owner acceptance, separate publication, and no charge/send from proposal acceptance. These complement, not replace, each task's focused tests.
- Run four named hypothesis fixtures, recording duration, concurrency, rows/bytes, CPU/storage, p50/p95/max, errors, retries and retention: (1) `small-hour`: 1,000 public renders, 20 forms, 10 checkouts at a fixture-only average USD 50 each (USD 500 gross attempted checkout value), 1 USD 50 refund, 12 provider callbacks including 2 replays, 60 transactional plus 20 consented campaign sends, 5 automation runs/15 steps, and 10 AI proposals at 10k input/1k output tokens each; 10 concurrent requests. (2) `active-day`: 25,000 renders, 1,000 forms, 500 checkouts at a fixture-only average USD 50 each (USD 25,000 gross attempted checkout value), 20 USD 50 refunds, 600 callbacks including 30 replays, 1,500 transactional plus 500 consented campaign sends, 200 runs/1,000 steps, and 100 proposals at 10k/1k tokens each; 50 concurrent requests. (3) `peak-hour`: 250,000 renders, 10,000 forms, 5,000 checkouts at a fixture-only average USD 50 each (USD 250,000 gross attempted checkout value), 250 USD 50 refunds, 6,000 callbacks plus 10x duplicate/reorder injection, 15,000 transactional plus 2,000 consented campaign sends, 2,000 runs/10,000 steps, and 500 proposals at 20k/2k tokens each; 250 concurrent requests. (4) `restore-replay`: restore the active-day dataset from a D1 backup taken before 50 deletions/redactions, 20 accepted submissions, 10 orders, 2 refunds, 12 ambiguous external effects, and 200 pending jobs; resume only after overlay reconciliation, with no automatic replay of unknown payment/email effects.
- Report separate dated formulas and measured inputs for Cloudflare Worker requests/CPU, D1 rows/queries/storage/restore, R2 bytes/operations, KV reads/writes only if used, and Queue/Workflow operations/retries only if selected; payment processor transaction/refund/dispute fees and volume; email plan/billable sends/retries and provider fees; AI/model input/output tokens and retries. Include cost per accepted form, confirmed order, delivered email, and accepted AI proposal. Fixture counts are test hypotheses, not quotas, capacity claims, price promises, or a fixed monthly ceiling.
- Test backup from before deletion, duplicate/out-of-order effects, pending unknown charge/email, cache rebuild, old revision/media rendering, schema rollback/forward, consent deletion, rate/abuse handling, accessibility, command exposure, both integrated journeys above, full CI, SBOM/license/provenance, and cost formulas. If deletion evidence or effect IDs do not survive a restore point, mark the capability unproven, block affected data collection or automatic retry, and do not proceed to P8.

## Phase 8: platform users and administration, last

### PR-049: hosted identity, authorization, scope topology decision

- Branch: `docs/pr-049-hosted-identity-decision`.
- Commits: `docs(auth): compare hosted identity and recovery models`; `docs(architecture): choose resource isolation topology`; `test(scope): prototype scoped migration alternatives`.
- Depends on PR-048 exit gate. This is the first phase that can choose account/session implementation.
- Compare per-owner isolated Worker/D1/R2/KV, shared scoped storage with mandatory predicates/composite keys, and hybrid routing using actual Cloudflare constraints, restore operations, migration burden, and measured costs. Define identity, sessions, recovery, revocation, account-to-resource mapping, MCP/CLI credential handling, platform roles, operator support, audit, and self-host upgrade. Specify an external private overlay that pins the released public core and integrates only through stable trusted interfaces; do not plan private auth/admin implementation in public core or under a public environment flag. Optional Cloudflare Access remains a self-host perimeter unless a reviewed product decision changes it.
- Prototype trusted `ResourceScope` plus synthetic owners without platform accounts. SQLite work only motivates a real D1/Worker test. Compare a persistent scope column versus singleton composition based on D1 migration and query evidence; the user boundary does not itself forbid scope columns. A scope field alone is not authorization or tenant isolation.
- Gate PR-050 on a reviewed topology, threat model, compatibility/backfill/rollback, new versioned auth/error addenda if needed, and public-core interface tests. Preserve frozen C-03/C-04 and do not implement users/auth/admin earlier.

### PR-050: platform accounts and authorization migration

- Branch: `feat/pr-050-platform-accounts`.
- Commits: `feat(auth): add platform account and session boundary`; `feat(storage): add reviewed account/resource migration`; `feat(commands): enforce scoped owner authorization`; `test(auth): verify cross-owner isolation and self-host upgrade`.
- Depends on PR-049 and PR-048.
- Introduce platform user/auth only now, in the private overlay outside the public repository. Pin an exact released public-core version and integrate through its stable trusted scope/command interfaces. Add expand/backfill/validate/switch/contract migrations only in the private deployment according to PR-049 topology. Trusted scope is injected before API/command/storage; it is never read from business JSON. Preserve all business command IDs, errors, UI, and self-hosted implicit-owner mode without forced login. No private code, secrets, or auth/admin environment-flag branch enters a public PR or artifact.
- Test wrong owner at API/command/repository/receipt/cache/R2/KV/domain/export boundaries; duplicate IDs across synthetic owners; missing scope; revoked session; recovery; migration rollback/forward; legacy self-host open without auth; adapter parity. A database column is not sufficient without scope-aware queries and negative tests.

### PR-051: private platform administration

- Branch: `feat/pr-051-platform-admin`.
- Commits: `feat(admin): add private platform operations`; `feat(admin): audit scoped operator actions`; `test(admin): deny business-data bypass`.
- Depends on PR-050.
- Add only platform administration screens and commands for platform accounts and service operations in the private overlay. Keep the owner funnel/CRM/analytics/commerce/AI dashboard in the public core and version-pin it unchanged. Support access to business data is deny-by-default, time-limited, reasoned, narrowly scoped, logged, and separately approved. Never mutate business state with direct SQL or privileged UI bypass. Keep overlay source, credentials, and private build configuration outside public PRs/artifacts.
- Test unauthenticated/wrong-role access, least privilege, audit immutability, expired support grant, data export, and every attempted business mutation through common commands. MCP owner tokens cannot call platform admin.

### PR-052: self-hosted and private distribution parity

- Branch: `test/pr-052-distribution-parity`.
- Commits: `test(distribution): verify common owner behavior in both builds`; `docs(operations): document self-host upgrade and account boundary`.
- Depends on PR-050/051.
- Verify the same pinned public-core release, business commands, errors, validation, UI journeys, and data model in self-hosted implicit-owner and private authenticated deployments. The private overlay adds only platform user/auth and admin surfaces. Preserve optional self-hosted Cloudflare Access and no forced SaaS subscription; no public feature may depend on overlay code or a private environment flag.
- Test upgrade from v0.1.0, migration/rollback, no-login self-hosted flow, common builder/CRM/payment/email/AI command parity, cross-owner denial in hosted mode, private admin separation, and distribution build composition. Report implementation, integration, and Cloudflare production proof separately.

## Cross-cutting skill workflow

### PR-053: install and route project-local skills

- Branch: `chore/pr-053-skill-workflow`.
- Commits: `chore(skills): add pinned project-local installer`; `docs(workflow): map skills to roadmap phases`.
- Task-ID: `PR-053`; tracking issue: [#23](https://github.com/lianihossam-eng/EezFunnels/issues/23).
- Depends on merged PR-021 (GitHub PR #22).
- It must complete before PR-022 starts. It does not create a ninth product phase, reorder PR-022–PR-052, change the public/private boundary, or alter product behavior.
- Pin and verify the already selected Matt Pocock and Cursor pstack revisions plus Impeccable 4.6.0. Add only missing selected local skill links; preserve existing links, ignored checkouts, and `.project-skills/README.md`. Do not install the eight Snyk skills, change global settings, add hooks, invoke Cursor automation, add app dependencies, or execute Impeccable's separately downloaded engine.
- Add a task-by-task map for PR-022–PR-052, explain each skill's role and limits, and route evidence to the owning tests. Impeccable may guide a future rendered-Dioxus UI pilot only after its engine has an independently verified digest; Playwright/axe, manual keyboard/screen-reader checks, and product-specific tests remain acceptance evidence.
- Integrate the adversarial plan clarifications: owner route map in PR-027; explicit absent/accepted/refused analytics consent gate in PR-032/033 and unchecked marketing opt-in in PR-028; unique email-provider choice in PR-036; provider setup/rotation UI in PR-038; buyer handoff/return UI in PR-039; campaign review and distinct campaign identities in PR-042; preview/activate and closed effect matrix in PR-043; integrated recovery journeys in PR-048; zero serious/critical contrast violations for PR-026's target states.
- Checks: installer syntax, pinned source verification, selected-link integrity, idempotent second install, and preservation of the preexisting 90 links and `.project-skills/README.md`; `git diff --check`; the roadmap structural validator; `cargo xtask check`; `cargo xtask ci`. No remote Cloudflare operation or product runtime change.
- GitHub workflow: link issue #23, open `[PR-053] Add project-local skill workflow`, tag `@codex`, request Sol's exact-head review, resolve all threads, require green hosted checks, then Sol merges. Update `plan/STATUS.md` before opening the PR.

## Decision and evidence gates that remain open

- V2 snapshot-vs-patch transfer; full editor/MCP document read/write within 16/16/256 KiB limits; stage token semantics; R2 object-size and owner quotas: `NEEDS_PROTOTYPE` before affected implementation.
- Domain/TLS/DNS, custom-host routing, cache invalidation, Queue/Workflow bindings, D1 transaction/recovery, and provider signature/idempotency behavior: `NEEDS_PROTOTYPE` or external account evidence. Do not infer from local mocks.
- Public consent wording, retention periods, data export/deletion law, attribution windows, email marketing policy, provider/jurisdiction fees, AI data retention, and secrets model require explicit policy/provider decisions in their owning PR. Do not invent legal claims.
- Physical scope columns/topology remain a Phase 8 evidence-led choice. A typed trusted scope seam may be designed earlier. The seam creates no user account, authentication, session UI, tenant promise, or private feature.
- Cost and capacity claims require actual dated pricing and measured workloads. `plan/STATUS.md`'s PR-019 local p95 is evidence for that local fixture only.
