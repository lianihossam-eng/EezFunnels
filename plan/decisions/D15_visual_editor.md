# D15 - Visual editor revision ownership and preview

Status: accepted for PR-010; `dioxus-dnd` 3.1.0 passed the Dioxus 0.7.10/Rust 1.85 SSR and WASM compatibility slice. Local migration preflight, browser behavior, and accessibility checks passed. Hosted CI and review remain PR gates; no remote migration or deployment ran.

## Problem

PR-010 needs a read path, a rendered preview, and a way to reorder sections into a new immutable revision. PR-009 stores a private v1 snapshot and manifest but has no reverse decoder or editing head. Publication has its own pointer and compare-and-swap behavior; it cannot identify the current draft. C-01 requires every edit to have a new revision ID and concurrent edits from the same base to conflict. C-01, C-03, and C-04 remain frozen.

## Usage (caller's view)

Opening a funnel card enters `/funnels/:id`. The editor loads the current section order and a server-rendered preview. Dragging a section or using the keyboard changes only the order of its existing IDs. Save creates a fresh immutable revision if the loaded revision is still the editing head. A stale save keeps the local order and offers an explicit reload; it never rebases silently.

The browser uses three same-origin API calls:

```text
GET  /api/funnels/:id
POST /api/funnels/:id/preview
PUT  /api/funnels/:id/revisions
```

Read returns the head revision ID, section outline, and preview HTML. Preview accepts `{base_revision_id, section_ids}` and returns rendered HTML for that exact base and order. Save accepts the same order-only payload and requires an `Idempotency-Key`; it returns the new revision ID. Neither preview nor save accepts client-supplied content, theme tokens, a manifest, HTML, or a revision ID to persist.

## Shape

Commands own the private v1 snapshot decoder and encoder. A validated snapshot contains the domain revision, blueprint, theme tokens, and typed content map. Decode checks the stored byte limit, format/schema versions, row and manifest identity, valid section types and unique IDs/orders, exact section/content coverage, theme and content hashes, and renderer compatibility. Domain constructors and a renderer preflight validate the reconstructed values before read, preview, or save. The existing v1 stored shape remains unchanged.

The browser owns a permutation of the validated section IDs and interaction state only. It never imports command, storage, or renderer types. The outline is a semantic list with stable-ID controls. Pointer drops use `dioxus-dnd`'s `SortableList`; explicit Space/arrow/Escape keyboard actions and accessible reorder buttons call the same reorder transition. Tab follows document order; focus stays on the moved section; a polite live region announces the action and position. Do not implement an ARIA tree for this flat list.

The renderer gains a preview policy that emits link-looking spans instead of navigable anchors and places a restrictive CSP in the preview document. Current PR-010 preview blocks external images and other network access while permitting the trusted inline styles required by the renderer. The dashboard places returned HTML only in an iframe `srcdoc` with an empty `sandbox` attribute and a useful title. No scripts, same-origin, forms, popups, top navigation, message bridge, parent-DOM insertion, or preview DOM rewriting are allowed. Browser tests must verify these properties; string tests alone are insufficient.

Presentation calls the browser API transport and commands through the existing HTTP boundary. The Worker parses the exact editor routes before its generic funnel-item route and dispatches each user-visible action through `Session::execute`. `funnel.editor.read` reads the head and its revision in one joined query. `funnel.editor.preview` validates an exact permutation against an owned immutable base and is read-only, uses `POST` to avoid URL-length limits, and returns `Cache-Control: no-store`. `funnel.revision.save` is a receipt-backed mutation.

Storage adds `funnel_editor_heads`, separate from `published_revision_id`. A guarded migration backfills only funnels with exactly one existing revision. It must fail atomically with a useful diagnosis if any funnel has zero or multiple revisions; no timestamp, ID ordering, rowid, or publication pointer is treated as revision ancestry. Run a read-only local preflight before migration and keep the repair mapping explicit for ambiguous data. Funnel creation writes its initial editor head atomically with the initial revision. Only the editor-save capability advances the head; publication and generic revision append do not.

For a save, classify an active idempotency receipt before stale-head, missing-funnel, capacity, or payload validation, including unreadable request bodies under the established PR-009 policy. A single D1 batch inserts a revision only when the expected head matches and capacity remains, advances the head only to that inserted revision, updates the funnel timestamp, and writes the successful receipt only when the new head is present. Guards prevent partial success; SQL failures roll back the batch. After an unsuccessful batch, classify a concurrent active receipt first, then not-found, stale head, and revision capacity. Never infer stale edits from database exception text. Same-key/same-payload retries return the stored result; same-key/different-payload returns the existing C-04 mismatch; competing distinct-key saves from one base yield one winner and one stale conflict, with no receipt or revision for the loser.

The implementation-scoped errors are `conflict/edit_stale_revision` and `conflict/revision_limit_reached` (HTTP 409), documented here without changing C-04. Corrupt stored snapshots are `internal/unexpected`; malformed client permutations use existing validation codes. The 100-revision cap remains; PR-010 does not prune history. A no-op API save still creates a new revision if submitted, while the UI disables Save when clean.

Preview responses carry a local generation and apply only to the matching route, base revision, and order. Old responses are ignored. Preview failure preserves the last rendered frame with a visible stale/error state. Save captures its exact payload and key, freezes reorder controls while pending, and retains the same capture for an uncertain retry. Success adopts the returned revision and saved order as the clean base. A stale conflict preserves unsaved local order until the user explicitly reloads.

The interface is deep: commands hide snapshot integrity, domain reconstruction, rendering, receipt precedence, revision limits, and CAS; storage hides the D1 transaction; presentation state hides request generations and retry identity. Callers provide a funnel ID, immutable base ID, and exact section order only. This follows boundary discipline and keeps one owner for each invariant.

## Synthesis decision

Candidate B is the base. Its fail-closed migration avoids promoting an insertion detail into draft lineage, and its renderer-owned preview output removes navigation targets at their source. Candidate A's receipt-first classification details and body-based `POST` preview are grafted in. Candidate A's `rowid` migration choice and transparent iframe interaction shield are rejected because they do not establish product lineage or reliably remove navigation behavior. The independent cross-judge is recorded at `/tmp/arena-pr010-editor/cross-judge.md`.

## Tradeoffs accepted

- We accept a separate editing-head table and a guarded legacy migration in exchange for atomic stale-edit conflicts independent of publication.
- We accept stopping migration on zero- or multi-revision legacy funnels in exchange for never guessing which revision is editable.
- We accept a server round trip for each settled preview order in exchange for one renderer and one validation policy.
- We accept an inert preview with blocked images in exchange for preventing preview content from navigating or making network requests.
- We accept freezing reorder controls during a save in exchange for a small state machine with no implicit rebase or lost edits.
- We accept a pinned DnD dependency and real browser test tooling only after compatibility, license, and runtime checks are recorded. `dioxus-nox-dnd` is not usable with this workspace: release 0.13.2 pins `dioxus = "=0.7.4"`, while its upstream main manifest pins `=0.7.9`; both conflict with the workspace's exact `=0.7.10`. `dioxus-dnd` 3.1.0 is MIT licensed, declares Rust 1.85, and accepts Dioxus 0.7; an isolated `SortableList` slice compiled with the exact Dioxus 0.7.10 and Rust 1.85 toolchain for host SSR and `wasm32-unknown-unknown`. Pointer sorting and the project-owned keyboard controls remain separately browser-tested.

## Alternatives considered

**Choose the newest legacy row by `rowid`, timestamp, or ID.** This keeps migration moving but turns physical insertion or sort order into unproven revision lineage. It loses to an explicit migration preflight and fail-closed policy.

**Send full document state to the browser and render locally.** This could make preview immediate, but exposes snapshot and renderer policy to presentation and requires a new shared crate or a duplicate renderer. PR-010 only reorders sections, so this surface is broader than its caller needs.

**Move renderer DOM nodes in the parent and use the result as `srcdoc`.** This makes renderer markup an undocumented editing protocol and creates a second owner of ordering. A preview-specific renderer policy keeps output generation and navigation policy together.

## Verification result

- The local migration harness proves atomic failure for funnels with zero or multiple revisions and successful head backfill when exactly one revision exists.
- The actual hydrated Worker/browser path passes pointer and keyboard reorder, preview refresh, save/replay, stale-save conflict, and delayed-save route-change checks. The section list remains semantic and axe reports zero critical findings.
- Browser checks confirm the preview iframe has an empty sandbox, its CSP denies network access, links are rendered without navigation targets, and a local network tripwire receives no request.
- A review regression found that a concurrent request losing an idempotency-key race could still advance its funnel head. The D1 batch now claims the receipt before mutation and gates each write on matching receipt ownership. A browser integration test checks that only the winning request advances its head.
- Save remains unavailable while a keyboard reorder is active, and Escape cannot mutate a locked save state. State tests cover both transitions.
- `cargo xtask check`, `cargo xtask ci`, `npm run test:pr010:migration`, and `npm run test:pr010:browser` passed on the final local source.

## Remaining gate

Hosted CI and PR review must pass before merge. The preview intentionally omits external images and link destinations for this phase. No remote D1 migration or Cloudflare deployment ran; this decision does not claim production readiness.
