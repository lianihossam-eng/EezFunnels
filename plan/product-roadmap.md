# Product roadmap and review packet

Status: proposed plan. Three local adversarial passes are complete; pass 3 returned corrections required, which are integrated into this revised candidate for targeted exact-candidate confirmation. The user chose local reviewers after DeepSeek could not be reached through the available browser path. Review provenance, findings, dispositions, and candidate digests are recorded in [the PR-021 review ledger](verification/PR-021/review-ledger.md).

Source discussion requested for review: https://chat.deepseek.com/share/t9ahagrwdr38ctp6rr

## Product boundary

Eez Funnel has one public business core whose features must work in the self hosted product. A future private distribution pins and uses that same public core without forking business behavior. Its private account/authentication and platform-admin overlay lives outside the public repository and integrates through stable trusted interfaces; private-only code, environment flags, and secrets do not enter public PRs or artifacts. The owner UI, business commands, domain rules, data model, and user-facing features remain common.

Only two layers are reserved for the final phase: platform user accounts and authentication, and a private platform administration dashboard. The funnel builder that a product owner uses remains common product UI. CRM contacts, form leads, buyers, and email recipients are business records. They are not platform user accounts.

The current v0.1.0 product has one implicit owner. Cloudflare Access may be an optional perimeter for a self hosted installation. It is not a chosen identity design for a future hosted service. Do not add login, user tables, admin routes, or private business features before Phase 8.

Design for later isolation while the common product grows. Keep ownership checks at the command and storage boundaries, preserve explicit resource scope in interfaces, and keep public visitor actions distinct from owner actions. These are design constraints for future work. They do not authorize early account or tenant implementation. Phase 8 will choose the actual identity model, persistence changes, and migration after the common product is stable.

Every user visible business action must use a stable command ID and JSON parameters through Session::execute. The UI, CLI, MCP, and future AI actions must share that command path. Add a command only after its actor, inputs, outputs, idempotency behavior, and errors are defined. Do not advertise placeholders.

The frozen contracts in plan/contracts remain authoritative and must not be edited. If a feature needs new semantics, define a separately named, versioned contract document or addendum plus an ADR, migration, and compatibility tests; preserve the old contract files and legacy behavior. A review of a diff does not authorize changing a frozen contract. Existing immutable revisions, compare and swap behavior, and idempotency rules remain in force.

Keep security and accessibility checks in every phase and every PR. Phase 7 adds broad release hardening, recovery, and measured operations. It does not defer basic security, privacy, accessibility, input limits, or regression tests.

Do not commit secrets. Do not run a remote Cloudflare operation until the required account, resource IDs, credentials, and operation scope are available. The GitHub v0.1.0 release does not establish Cloudflare production readiness. Cloudflare production bindings and deployment remain unverified. Claude Desktop interoperability also remains unverified.

Measure Cloudflare use against named workloads and actual account pricing. Record the workload, duration, request count, stored data, and observed cost before making a cost claim. The plan promises no absolute monthly ceiling.

## Verified product baseline

GitHub PR #21 merged on 2026-10-15 at 3d46ecaf4fc6dd0411ae047841b1001f1ab1f8e6. The v0.1.0 GitHub Release is published with Linux x86_64, macOS aarch64, and Windows x86_64 CLI binaries plus SHA256SUMS. The release run and hosted checks passed. This proves that the release artifacts were published. It does not prove that a Cloudflare account is configured or that the application is production ready.

The current product has funnel create, list, delete, a single page editor, section content and inspector setting edits, theme overrides, section reorder, preview, immutable revision save, publish, unpublish, CLI, and an MCP endpoint. The shared command registry currently contains nine executable command IDs. Structural section operations are limited to reordering existing sections; the editor cannot insert, delete, or duplicate them.

The dashboard has no publish or unpublish control. The CLI can publish. MCP exposes the common command tools. R2 is bound, but the product has no media upload flow. Five preset themes exist. A funnel has one page, and the product has no form submission, lead store, CRM, analytics, attribution, checkout, order, email automation, or AI feature.

The current dashboard is an implicit single owner interface. It has no user login, user roles, or platform admin dashboard. Its trust boundary is documented in plan/architecture.md.

The API request limit and funnel domain limit are separate constraints. The editor API caps a request at 16 KiB. C-01_config caps a serialized funnel at 1 MiB. The domain limit does not raise the API limit. A media upload and a larger editor payload need a reviewed transport and storage design; they must not bypass either current limit.

## Eight phases

The phase order is the proposed delivery order. Review interactions before implementation. Keep each implementation PR small enough to test and revert independently. The `PR-0XX` values are project Task-IDs, not GitHub PR numbers. The consolidated ledger is the canonical number map; a material scope change must update all references and dependencies together.

### Phase 1: close v0.1.0 and settle the shared product boundary

Outcome: release facts, current behavior, exclusions, public and private boundary, and the common backlog have one reviewable source.

Work:
- Correct README and plan/STATUS.md to record PR #21, tag v0.1.0, and the four published release assets.
- Record the shared product rule, the two final private layers, the self hosted single owner model, and Cloudflare Access as an optional self hosted perimeter only.
- Record the actual product gaps and an ordered roadmap with review prompts and acceptance gates.
- Keep GitHub release proof, application integration proof, and Cloudflare production proof distinct.

This documentation packet is Task-ID PR-021. Its proposed branch, Conventional Commit subjects, source dependencies, evidence, and gates are in the canonical [PR ledger](roadmap-pr-ledger.md). It ships no product feature.

Prototype:
- README presents the released CLI assets, current dashboard capability, and a link to the roadmap.
- The roadmap shows current behavior, planned behavior, dependency order, data ownership, and review status.

Gate:
- All release claims match the remote PR, tag, workflow, and asset list.
- No product feature implementation begins until local reviewers have covered every phase, all 28 pairs, and the combined journeys, the findings are integrated or explicitly left as gates, and the exact integrated plan passes the final independent review.
- The DeepSeek history remains explicit: 0 prompts were sent, and no external findings exist.

### Phase 2: complete the shared builder

Outcome: the owner can manage publication, page sections, media, themes, and accessible editing from the dashboard.

Proposed PR order is detailed in [the PR ledger](roadmap-pr-ledger.md). It starts with dashboard publish and unpublish controls, then a versioned document and transport decision, typed structural editing, immutable R2 media, and theme, responsive, history, and accessibility work. Current section-content editing, inspector settings, and theme overrides remain intact while structural operations are expanded.

Prototype:
- Funnel editor: page title and saved revision; Preview; Save; Publish or Unpublish; a section outline; Add section; Duplicate; Delete; an inspector; a media picker; five preset theme choices.
- Publish flow: inspect the revision, compare it with the published revision, confirm Publish, and show the resulting public URL. On a stale conflict, reload the current state and ask the owner to retry.
- Unpublish flow: state that the public URL will stop serving the funnel, ask for confirmation, then show the unpublished state.

Data and versions:
- Section changes create a new revision and never mutate a stored revision.
- The published pointer continues to reference one immutable revision.
- Media keys must not make an old published revision render different bytes after a later edit. Store media identity and validated metadata in the revision or its manifest, and retain referenced objects for supported revisions.
- Theme and renderer versions stay recorded in the render manifest.
- Define upload and render budgets separately. Do not encode image bytes in a funnel JSON field.

Acceptance:
- Dashboard publication uses the existing publish and unpublish command semantics.
- Section operations preserve revision history, idempotency, and compare and swap behavior.
- Every media reference resolves to an authorized object with validated type, size, and safe rendering.
- Keyboard operation, visible focus, labels, status announcements, mobile layout, and contrast are checked in each PR.

### Phase 3: add pages, forms, lead capture, domains, and SEO

Outcome: an owner can create a multi page funnel, capture and manage prospects from forms, configure a verified custom domain, and set search metadata. The owner-facing “Leads (soumissions)” inbox presents each accepted form submission as a prospect record. A row is one submission, not a deduplicated person, Contact, or CRM opportunity; cross-submission identity and pipeline actions belong to Phase 4 CRM. This is the local plan's explicit P3 product-scope choice, not a validated product decision.

Proposed PR order is detailed in the [PR ledger](roadmap-pr-ledger.md): versioned page routes; typed form definitions; an owner Leads inbox with export, deletion, retention, and redaction controls; public form capture only after those controls and restore-surviving deletion evidence pass; and verified domain plus SEO output.

Prototype:
- Editor navigation shows pages and their order. Selecting a page loads its sections and page settings.
- A public form shows labels, required fields, field errors, submit progress, and a clear success or failure state.
- Domain settings show verification, active host, and failure state. SEO settings preview title, description, canonical URL, and share card.

Data and versions:
- Page structure, form schema, and render inputs belong to an immutable funnel revision.
- A public submission stores the exact form schema version, page ID, form ID, published revision ID, submit time, and applicable consent evidence. A later form edit does not reinterpret old submissions.
- The Leads inbox exposes captured prospects backed by `FormSubmission` records. One row equals one accepted submission. It does not merge by email, create a Contact, or create a CRM Lead entity. Phase 4 adds explicit person/opportunity creation and links. All these records are business data, never platform identities.
- Minimize stored personal data and define consent, retention, export, deletion, redaction, and restore behavior before enabling public collection. PR-029 must prove a deletion overlay/journal survives a restore from before deletion and is applied before export, projection rebuild, or dispatch resumes. If that proof is unavailable, PR-030's collector remains disabled; Phase 7 repeats the restore exercise but is not the first privacy gate.
- A custom domain maps to an owner resource through an exact verified binding and fail-closed public-only route resolver. Never infer ownership from a Host header alone; transfer retires the old binding before a new owner can serve content.
- Keep old public routes and supported revision rendering explicit during schema migration.

Acceptance:
- Old single page funnels continue to render.
- Forms validate on both client and server and reject forged, oversized, or stale definitions.
- Public submissions cannot read or alter owner data, choose an owner or schema version, or access CRM identity records. Retry and stale-form behavior is explicit and idempotent.
- Domain ownership, TLS status, and SEO canonical behavior are visible and testable without silently deploying remote configuration.

### Phase 4: add CRM, analytics, and attribution

Outcome: the owner can understand funnel performance and act on leads using transparent, traceable data.

Proposed PR order is detailed in the [PR ledger](roadmap-pr-ledger.md): define event and privacy contracts; add bounded analytics collection and reports; add explicit Contact and CRM opportunity records with consent-aware links, status, tags, and attribution; then add CRM activities, follow-ups, search, saved filters, and deep links. The Phase 3 Leads inbox remains the source submission record.

Prototype:
- Overview shows visits, form completion, and conversion by funnel, page, and selected time range.
- A contact detail shows source, consent, form submissions, timeline, and owner notes.
- Attribution shows the evidence used for each source, the window, and any unknown or conflicting data.

Data and versions:
- The D1 `FormSubmission` row is the sole durable source of truth for a committed form submission. `form.submission.accepted` is a versioned, non-PII outbox signal carrying only an opaque submission ID; `page_view` and `form_start` are separate observation signals. Each projection deduplicates by submission ID and rule version, checks the durable deletion overlay, and never counts a success-page beacon.
- Events refer to funnel revision, page, and form IDs so results can be traced to published content.
- Separate event identity, submission identity, Contact identity, and opportunity identity. Link only with an explicit versioned rule and consent basis. Email equality is a duplicate candidate, never proof of identity.
- Store event time and receive time. Define duplicate handling and correction rules. Do not claim exactly once delivery.
- Store attribution inputs and rule version so a later attribution change can be explained or recomputed. Accept only the five named UTM keys and exact owner-configured campaign IDs; discard arbitrary query/referrer values and mark attribution `unknown` when no safe registered ID survives. Never carry a visitor identity, token, or session across domains.
- Keep personal data out of aggregate event rows where possible.

Acceptance:
- Counts reconcile against deterministic local fixtures.
- Duplicate and delayed events produce documented results.
- Owners can inspect where attribution came from.
- Consent, retention, deletion, and export work across both CRM detail and analytics aggregates.

### Phase 5: add payments inside funnels

Outcome: an owner can sell products through funnel checkout pages, inspect orders, add upsells, and run related emails and automations.

This phase covers customer payments for the owner's products. It does not cover a subscription for access to a private SaaS service. Checkout, orders, payment providers, email, and automations remain shared public business features. Payment and email providers have separate decision and credential policies.

Proposed PRs PR-036 through PR-043 are detailed in the [PR ledger](roadmap-pr-ledger.md). They make separate payment-provider and email-provider decisions, decide secret handling and durable runtime first, then add immutable offer versions, signed provider-event intake, checkout intents and orders, reconciliation/refunds, upsells, consented email, and bounded automations.

Prototype:
- Checkout shows the offer, price, currency, tax or shipping state where applicable, payment method, and a clear pending, paid, or failed result.
- The next funnel step shows an optional upsell and its price before consent.
- The owner view lists orders with provider state, last event, reconciliation state, and an action history.

Data and versions:
- An order stores immutable product and price snapshots, currency, the funnel revision, checkout page, and provider reference.
- Provider event IDs and command idempotency keys make observed retries safe where the provider supports them. Reconcile ambiguous outcomes. Do not promise universal exactly-once effects.
- Payment events, emails, and automation jobs are external effects. Persist intent before sending. Record retries and outcomes. A provider may accept an effect while the caller loses the response; represent this as `unknown` until reconciled. Never treat it as `paid`, `failed`, or safe to retry without evidence.
- Provider-event and action IDs outlive C-03's 24-hour command-receipt window. A duplicate callback must not silently create a second order.
- Email-provider acceptance is not proof of delivery or sender health. Real delivery stays gated on a selected provider, verified sender-domain state, bounded rates, visible bounce/complaint health and pause/suppression controls, a viable Rust Worker durable-intent prototype, and an authorized provider test-account check.
- Never put payment credentials in funnel revisions, client responses, or logs.
- Keep customer orders distinct from platform billing. Platform billing is outside this roadmap.

Acceptance:
- Every payment transition has a tested idempotent path where the provider supports it, plus a recovery or reconciliation path.
- The owner can see pending, confirmed, failed, and unknown states without treating an uncertain response as success.
- Failed email or automation delivery does not corrupt the order.
- Tests cover duplicate callbacks, timeout after provider acceptance, retry, refund, and changed offers.

### Phase 6: add AI through the shared command path

Outcome: AI can help draft funnels while the owner keeps control over every saved or published change.

Proposed PRs PR-044 through PR-047 are detailed in the [PR ledger](roadmap-pr-ledger.md): provider/privacy/usage boundary; typed funnel proposals; read-only, evidence-linked business summaries; and explicit approval, audit, retry, and compensating actions.

Prototype:
- The owner describes a goal and selects a funnel or page.
- The product shows a proposal beside the current revision.
- The owner can inspect each change, preview it, accept selected changes, or reject the proposal.
- Saving creates an ordinary immutable revision. Publishing remains a separate owner action.

Data and versions:
- AI output is untrusted input. Parse it into typed command parameters, validate it through existing domain rules, and use the same dispatcher as UI, CLI, and MCP.
- An AI action cannot write storage directly or publish without the normal command and owner decision.
- Record provider, model, command, input references, accepted result, and actor according to a reviewed privacy policy. Do not store hidden reasoning.
- Keep API keys in secure environment configuration. Never commit them or include them in prompts.
- A prompt and output do not become authoritative product state until a validated command creates a revision.

Acceptance:
- Invalid and oversized output fails closed.
- A proposal can be previewed, partly accepted, rejected, or undone.
- The same command works through every supported adapter with identical validation and errors.
- The owner can inspect AI changes before publication.

### Phase 7: stabilize the common product

Outcome: the shared product has measured reliability, recovery, accessibility, and operational behavior before private user and admin layers begin.

This phase is a broad release gate, not the first time the project tests security. Every earlier PR already adds focused tests, security and privacy checks, accessibility checks, migration coverage, and relevant local integration tests.

Work:
- Run the full CI suite on each PR and release candidate.
- Add migration rehearsal and rollback guidance, backup and restore checks, and a local recovery exercise.
- Measure representative Worker, D1, R2, KV, email, and AI workloads. Record actual usage and pricing inputs.
- Review retention, data export and deletion, logs, errors, rate limits, abuse handling, resource ceilings, and accessibility.
- Recheck command parity across the dashboard, CLI, MCP, and AI adapters.
- Record unverified hosted provider behavior instead of inferring it from local mocks.
- PR-048 records release-candidate evidence and the phase exit decision. Land any product fixes as separate PRs with the usual tests.

Acceptance:
- Every release gate has an owner, reproducible command, recorded result, and known limitations.
- Restore reproduces a tested state or lists exactly which external effects cannot be restored.
- Performance and cost claims include a workload and measurement date.
- No result upgrades implementation proof to production proof without a live, authorized check.

### Phase 8: add private platform users and administration

Outcome: platform operators can manage user access and platform operations without changing the common business product.

This is the final product phase. It does not retrofit CRM contacts, leads, or buyers into platform accounts. It does not introduce a private version of funnel business features. Platform identity and admin stay in a private overlay outside the public core; the overlay pins and integrates with a released public-core version through stable trusted interfaces.

Proposed PRs PR-049 through PR-052 are detailed in the [PR ledger](roadmap-pr-ledger.md): hosted identity/topology decision; platform accounts and authorization migration; private platform administration and audit; and self-hosted/private distribution parity. These are the only private product layers.

Prototype:
- Hosted sign in and recovery belong to the platform identity boundary.
- The owner dashboard retains the common funnel, CRM, analytics, payment, automation, and AI features.
- The private admin dashboard manages platform accounts and service operations. It does not edit customer funnel content by bypassing commands.

Data and versions:
- Choose tenant keys, ownership backfill, session lifetime, access revocation, and migration behavior before adding account columns.
- Every scoped query and command gets negative cross owner tests.
- Define how self hosted single owner installations work without requiring a platform account.
- Keep Cloudflare Access as an optional self hosted edge protection choice unless a reviewed decision changes that boundary.
- Keep the public core free of private implementation code, private-only environment flags, and secrets. Public PRs define only stable integration seams; P8 changes the private overlay and its compatibility tests, while self-hosted deployments keep the same public core without login.
- Platform subscription billing remains a separate product decision.

Acceptance:
- Cross owner access fails closed at API, command, and storage boundaries.
- Existing single owner installations can upgrade without data loss or forced hosted login.
- Platform admin actions are authorized and audited.
- Business behavior and command IDs remain common to every distribution.

## Low fidelity interaction prototypes

### Builder publication

~~~mermaid
flowchart TD
  A[Open funnel] --> B[Edit draft revision]
  B --> C[Preview current revision]
  C --> D[Save immutable revision]
  D --> E{Publish this revision}
  E -->|Confirm| F[CAS publish command]
  E -->|Cancel| D
  F -->|Current pointer matches| G[Show public URL and revision]
  F -->|Stale pointer| H[Show conflict and reload action]
  G --> I{Unpublish}
  I -->|Confirm| J[CAS unpublish command]
~~~

### Submission to order lifecycle

~~~mermaid
flowchart LR
  A[Published page revision] --> B[Versioned form]
  B --> C[FormSubmission and consent]
  C --> D[Leads inbox row]
  D --> E[Explicit Phase 4 Contact or opportunity link]
  C --> F[Separate page-view and form-start observations]
  E --> G[Checkout only after a published offer exists]
  G --> H[Verified provider evidence]
  H --> I[Order state and outbox]
  I --> J[Email or automation delivery]
~~~

Each arrow is a durable relationship to verify, not a promise of exactly once delivery.

### AI edit review

~~~mermaid
flowchart TD
  A[Owner request] --> B[AI proposal]
  B --> C[Typed command validation]
  C --> D[Preview and diff]
  D --> E{Owner decision}
  E -->|Accept selected changes| F[Normal revision save]
  E -->|Reject| G[Discard proposal]
  F --> H[Separate publish action]
~~~

### Private platform boundary

~~~mermaid
flowchart LR
  A[Platform account] --> B[Private authentication]
  B --> C[Scoped owner session]
  C --> D[Common dashboard]
  C --> E[Common command dispatcher]
  E --> F[Common business storage]
  G[Private platform admin] --> H[Platform operations]
~~~

The private platform admin cannot bypass the common command path to change funnel business data.

## Pairwise review matrix

Review all 28 pairs separately after the eight individual phase reviews. For each pair, challenge the stored data, version references, migration order, user-facing transition, failure state, security boundary, and measurement plan.

| Pair | Shared data or boundary | UX and order risk | Reviewer challenge |
|:--|:--|:--|:--|
| P1 + P2 | Release facts and the current builder baseline | A reader may mistake planned controls for shipped controls | Which claims need a current code or release artifact link? |
| P1 + P3 | Backlog scope and the current single page model | Roadmap copy may promise custom domains or forms that do not exist | What must README say to separate release scope from future scope? |
| P1 + P4 | Public product boundary and future customer data | Analytics planning may blur aggregate events and personal data | Which privacy decisions must be made before collection starts? |
| P1 + P5 | Common product rule and customer payments | Payment checkout may be confused with SaaS subscription billing | Does this plan keep product sales separate from platform billing at every layer? |
| P1 + P6 | Shared command rule and AI proposal scope | A generated change may appear to bypass owner review | Which boundary prevents an AI result from becoming published state? |
| P1 + P7 | Proof statuses and release gates | A GitHub release can be mistaken for production readiness | What evidence is needed for each proof status? |
| P1 + P8 | Public product and final private layers | Auth or admin work may appear as a prerequisite for common features | Does the plan keep user accounts and admin last while designing extension seams now? |
| P2 + P3 | Section, page, form, and media references | Adding pages may invalidate current editor routes or section IDs | Which objects belong in one immutable revision, and which need independent IDs? |
| P2 + P4 | Published revision, page events, and assets | Analytics may count a draft preview as a public visit | How are preview traffic, published traffic, and revision IDs separated? |
| P2 + P5 | Builder pages, checkout steps, offers, and media | A theme or section edit may change a live checkout without a new publication | How does a purchase retain the exact published offer and price? |
| P2 + P6 | Builder commands and generated edits | AI may save directly or publish an unreviewed result | How do proposal, preview, selected acceptance, save, and publish stay separate? |
| P2 + P7 | Builder revisions, migrations, and accessibility | A late editor change may lack keyboard or recovery tests | Which builder checks run in every PR rather than only at stabilization? |
| P2 + P8 | Common owner dashboard and future platform session | Auth design may fork the builder into public and private editions | Which session seam scopes ownership without changing common command behavior? |
| P3 + P4 | Versioned forms, submissions, leads, and events | A contact merge may rewrite historical funnel attribution | Which immutable IDs preserve the form and page a visitor saw? |
| P3 + P5 | Leads, checkout form, order, and consent | Form completion and paid conversion may be confused | How do orders retain form consent and checkout revision without duplicating identity? |
| P3 + P6 | AI page and form proposals | Generated fields may collect unnecessary personal data | What data policy and human review apply before a generated form is published? |
| P3 + P7 | Multi page routes, forms, domains, and migrations | New route support may break old slugs or migration rollback | Which old snapshots and public routes must stay renderable in each PR? |
| P3 + P8 | CRM leads and future platform accounts | A lead email may be treated as a login identity | How do business contact identity and platform account identity stay separate? |
| P4 + P5 | Contacts, attributed campaigns, purchases, and email | A delayed payment event may rewrite attribution or send duplicate email | What event identity, receive time, and reconciliation state must be stored? |
| P4 + P6 | Analytics and AI summaries | AI may expose personal event data or present inference as fact | Which aggregates can AI read, and how does the UI show uncertainty and source evidence? |
| P4 + P7 | Analytics retention, query cost, and correctness | Stabilization may optimize a metric that loses raw evidence | Which retention, sampling, cost, and reconciliation checks run continuously? |
| P4 + P8 | CRM records and private account administration | Admin access may expose customer contact data without reason | Which role and audit boundaries protect business records after Phase 8? |
| P5 + P6 | Offers, prices, payment commands, and provider secrets | AI may change a price or initiate a charge without consent | Which generated actions are proposal only, and which always require owner confirmation? |
| P5 + P7 | Orders, webhooks, emails, and recovery | Retries may duplicate external effects after an ambiguous timeout | What reconciliation evidence is needed without claiming universal exactly once behavior? |
| P5 + P8 | Customer purchases and platform accounts | A buyer may be mistaken for a platform user or billed for SaaS access | Are customer payment records independent from platform authentication and billing? |
| P6 + P7 | AI providers, audit data, and repeatable tests | A provider change may alter output or leak prompt data | What deterministic command tests and provider privacy checks run in each PR? |
| P6 + P8 | AI actor context and future account permissions | AI may inherit broader platform admin rights than the owner intended | How does a later auth context constrain the same AI commands without an early auth implementation? |
| P7 + P8 | Common product release proof and platform identity | Auth work may conceal unresolved common product failures | Which common product readiness criteria must pass before account migration begins? |

## Multi phase journey reviews

After the pair matrix, review these combined journeys as separate scenarios. They expose interactions that no single pair can cover.

1. A visitor opens a published page, submits a versioned form, appears as one “Leads (soumissions)” inbox row, and may later be explicitly linked to a Contact/opportunity in Phase 4. Trace the published revision, form schema, consent record, duplicate submission behavior, separate page-view/form-start observations, retention, and deletion.
2. A visitor checks out, pays, accepts or declines an upsell, receives an order result, and enters an email automation. Trace the price snapshot, verified provider event, distinct `upsell.decision.recorded` event, uncertain timeout, retry, duplicate callback, outbox, and reconciliation. Trigger paid-order effects only from verified provider confirmation; confirm that no step means SaaS subscription billing.
3. A campaign link leads to a page, form submission, CRM contact, and later purchase. Trace exact owner-configured campaign IDs from the five UTM keys, dropped arbitrary query/referrer values, event time, receive time, contact linkage, correction, and a changed attribution rule. Show `unknown` when safe registered evidence is absent; never carry identity across domains.
4. An owner asks AI to add a page and form, previews a proposal, accepts part of it, saves a revision, publishes it, then receives a lead. Confirm that typed commands, form privacy, version references, audit data, and owner approval remain intact.
5. A future platform account signs in to the common owner dashboard and views customer records. Test an owner with the wrong workspace, a revoked session, a self hosted single owner installation, platform admin access, and buyer records that have no platform account.
6. A release migration updates pages, forms, assets, analytics, payment records, and a platform account boundary. Confirm that old public revisions still render and that external provider effects cannot be rolled back as database writes.

## Adversarial review protocol

Review each phase first, then each pair in the matrix, then the six combined journeys. Keep the review independent of the plan author. The user chose this local review process after the available browser path could not reach the authenticated DeepSeek tab. The review ledger records four independent reviews, four cross-reviews, the exact-candidate pass-3 findings, their dispositions, and the targeted confirmation status for the revised candidate.

For every prompt, send the relevant phase text, pair row, current baseline, and cited contracts. Ask for concrete counterexamples in data, UX, migration, permissions, cost measurement, and recovery. Request severity, affected decision, failure path, smallest fix, and unresolved question. Ask the reviewer to distinguish facts supported by the repository from assumptions. Do not ask for hidden reasoning.

Use this common review instruction:

~~~text
Review this Eez Funnel plan adversarially against the supplied repository facts and the stated product boundary. Do not invent shipped behavior. Find concrete user journeys that break the data model, version compatibility, command parity, access boundary, privacy, accessibility, or recovery behavior. For each finding, give the affected phase, a reproducible scenario, impact, smallest plan change, and any fact that remains unknown. Challenge the proposed order and identify circular dependencies. Do not assume external effects are exactly once or infer production readiness from a GitHub release. Do not propose private account or admin implementation before the final phase.
~~~

Individual phase focus:
- P1: challenge whether the release state, product boundary, and next-action gate are backed by evidence.
- P2: challenge save, publish, unpublish, section edits, media, theme, and keyboard flows against revision and payload limits.
- P3: challenge old route compatibility, form privacy, consent, lead deletion, domain ownership, and SEO.
- P4: challenge event identity, attribution evidence, duplicate delivery, retention, deletion, and query cost.
- P5: challenge order transitions, provider retries, webhook duplicates, upsell consent, secret handling, and email recovery.
- P6: challenge untrusted output, command validation, selective acceptance, data sent to providers, and publication control.
- P7: challenge release gates, recovery evidence, performance workloads, cost inputs, and the boundary between local proof and production proof.
- P8: challenge account recovery, cross owner isolation, self hosted migration, platform admin scope, and separation from CRM and customer payment identities.

Pair prompts use the common instruction followed by the exact matrix row. Combined journey prompts use the common instruction followed by the journey description and the linked phase sections.

Record each actual response in a dated review log with its work request, source report hash, findings, accepted or rejected plan changes, and rationale. See [the PR-021 review ledger](verification/PR-021/review-ledger.md) for report digests, dispositions, current pass status, and P3 arbitration. The DeepSeek share remains a historical reference only. No prompt was sent, and no DeepSeek response or approval is implied by this packet.

## Implementation gates

- Do not begin the next business feature until local reviewers have covered all eight phases, all 28 pairs, and the combined journeys, pass-3 findings have been addressed and confirmed against the revised manifest, and the exact final PR head passes review.
- Every implementation PR adds behavior tests for its changed user journey, schema and migration tests, command parity checks, security and privacy checks, accessibility checks, and local integration checks where relevant.
- Run cargo xtask check and cargo xtask ci for implementation PRs. Keep Cloudflare production checks separate and run them only when the required remote prerequisites are available.
- Keep each PR small and ordered. Land a stable command contract and data owner before adding adapters.
- Record new decisions in plan/decisions before implementation. Do not edit frozen C-01, C-03, C-04, D19, D21, or configuration contracts; define a new versioned document/addendum and compatibility tests alongside any new semantics.
- At each release, report implementation, integration, and production proof separately.

## Sources and limits

Repository facts come from README.md, plan/STATUS.md, plan/architecture.md, plan/contracts/C-01_funnel.md, plan/contracts/C-01_config.md, plan/contracts/C-03_security.md, plan/contracts/C-04_errors.md, plan/decisions/D10_render_section_content.md, plan/decisions/D15_visual_editor.md, plan/decisions/D18_publication_routes.md, plan/decisions/D19_command_registry.md, plan/decisions/D21_mcp_worker_transport.md, apps/api/src/lib.rs, the current command registry, dashboard, storage migrations, and wrangler.toml. Review provenance and local prototype limits are in [the PR-021 review ledger](verification/PR-021/review-ledger.md).

The DeepSeek share remains a historical reference: https://chat.deepseek.com/share/t9ahagrwdr38ctp6rr. The available browser path could not interact with the authenticated DeepSeek tab. Prior access to the share page reached a sign-in redirect and sent zero prompts. The user then chose local adversarial review as the plan review method. No external findings are claimed, and external review is not a gate.

This plan remains proposed pending targeted confirmation of the integrated corrections and final exact-head PR review. Phase acceptance criteria are planning gates, not evidence that the features exist or have passed.
