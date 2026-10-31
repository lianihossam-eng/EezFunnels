# Public edition contract

EezFunnels is an open-source funnel builder for one operator's self-hosted Cloudflare account. The operator controls the Worker, D1 database, R2 bucket, KV namespace, public destinations, backups, and access perimeter. The public edition has no built-in login or multi-user roles; protect owner pages, APIs, reports, and the inbox with Cloudflare Access or another network perimeter.

This series covers the complete public, single-owner edition. It excludes hosted SaaS, multiple accounts or roles, a full CRM, email/SMS campaigns, a native calendar, payment processing, subscriptions, member areas, AI generation, and A/B tests. It adds no Cloudflare service.

The product's terms are defined in the [glossary](../GLOSSARY.md). The five application layers and their allowed dependencies are defined in [Architecture](architecture.md).

## Structured service offers

An owner can add an offer with its scope, exact decimal amount, currency, price unit, and conditions. Amounts use a non-negative decimal string with up to 12 whole digits and 3 decimal places; currency values use three uppercase ASCII letters. One-time and recurring prices are distinct; recurring offers can state an initial amount and an owner-entered announced total, which the system never calculates. A comparison references the same offers shown in their cards, so scope, prices, and conditions stay consistent. Incomplete offers may remain in a draft but block publication. These content blocks describe terms and do not accept orders, calculate discounts, or imply availability.

## Contextual evidence

An owner can add a testimonial or case study as an evidence item. Each item has a title, content, a visible limitation, and either a demonstration status or a real status. A real item also requires owner-entered attribution, a date or period, and scope. The system displays these details beside the claim; it does not independently verify a real-status claim. Demonstrations are visibly labeled and are never rendered as client evidence. Incomplete items may be saved as drafts but block publication.

Evidence images reuse assets already in the funnel's media library and require alternative text. The document stores an asset reference rather than a copy; missing media blocks rendering. Remote image URLs, video, and third-party media embeds are not supported by this block. Existing testimonial sections and old V2 documents remain readable. Evidence is an additive V2 block rendered by renderer-v12; older renderer profiles reject it instead of dropping its status or context.

## Service recipes

V2 funnel creation offers contact, estimate-request, and diagnostic-request recipes. Each recipe creates a fresh V2 document from the existing `lead_gen` blueprint with purpose-specific copy and form fields; the selected recipe is baked into that document and does not add a new blueprint ID or stored schema field. Existing documents and the five established blueprint IDs remain unchanged. Updating a recipe affects only funnels created afterward.

Contact asks for a message, estimate requests collect scope and an optional timeline, and diagnostic requests collect the current situation and a goal to clarify. Request acknowledgement remains required and marketing consent remains off. An estimate request does not promise a price or response date. A diagnostic request does not book a session, score the applicant, or provide a diagnosis.

## Portable templates

The dashboard exports the saved V2 revision as a versioned JSON template with a manifest for every referenced image. Export does not include an unsaved editor draft. Import validates the package and target media library, remaps internal funnel, page, section, block, form, field, and consent-copy identifiers, then atomically creates a new unpublished V2 funnel. Images already present in the target library keep their references; an unavailable image requires an explicit replacement from that library. The importer does not fetch external URLs. V1 revisions, private briefs, submissions, delivery configuration, and secrets are excluded.

## Current capability snapshot

This table describes the public edition after the improvement series. Issue 82 remains the live source for delivery status.

| Area | Available now | Boundary maintained by the series |
| --- | --- | --- |
| Visitor pages and editor | Typed multi-page V2 documents, immutable revisions, preview, publication checks, SEO metadata, mapped custom hosts, and renderer compatibility for older revisions. | Pages and actions present information or navigate; they do not establish a booking, sale, or payment. |
| Service composition | Contact, estimate-request, and diagnostic-request recipes; FAQ, operating steps, exact-price offers, comparisons, and contextual evidence. | Recipes seed editable documents. Demonstrations are labeled; owner-entered real evidence is not independently verified. |
| Public forms | French and English copy, text/email/phone/checkbox/select fields, multi-line text, field-level HTML recovery, bounded idempotency, and required request acknowledgement. | Marketing and measurement consent are separate. Public errors do not echo untrusted transport bodies or include values in URLs. |
| Submissions and inbox | Private retained responses, export, targeted erasure, legacy statuses, versioned follow-up history, next action, and close reason. | Recovery requires restored D1 with the original R2 journals intact and reconciliation before reads or dispatch resume. `closed` is not payment. |
| Delivery | Operator-owned HTTPS destination, disabled by default, selected form fields, immutable task configuration, signed requests, bounded retry, and owner-visible controls. | No visitor-provided destination. Delivery may result in zero, one, or several receiver attempts; an acknowledgement does not prove downstream completion. |
| Reporting | Retained completion and follow-up outcome cohorts, explicit coverage and retention, and consented current-page interaction aggregates using exact registered campaign IDs. | No visitor/session identity, cross-page attribution, campaign-to-person join, visitor-to-request rate, or inferred sale/payment. |
| Portability | Versioned saved-V2 template export with an exact media manifest and import as a new unpublished funnel. | Import validates media in the target library; it does not fetch external URLs, overwrite, publish, or include private briefs, submissions, delivery settings, or secrets. |
| Runtime and recovery | Cloudflare Workers with D1, R2, and KV; the series adds no Cloudflare service. | CI and local recovery tests do not verify an account-specific deployment or execute an actual Cloudflare D1 Time Travel restore. |

## Outcome limits

The public journey may record that a request arrived, that an owner contacted or qualified it, and that the owner documented an outcome. It does not provide a native calendar, reservation system, payment processing, subscription handling, customer account, or full CRM. Do not describe a submitted form as a booking, a qualified request as a sale, or an HTTP webhook acknowledgement as proof of a completed downstream action.

## Submission follow-up history

The inbox keeps a current status and optional next action, due date, and close reason. `POST /api/submissions/:submission_id/status` retains its strict one-field body and four existing status values. A change through that endpoint also appends a `legacy_status` history event; setting the same status remains a successful no-op.

`GET /api/submissions/:submission_id/follow-up` reads the retained current state and its history. `PUT` accepts the complete disposition with `expected_version` and an `Idempotency-Key`. A stale version or an incompatible reuse of a key, including across submissions, returns a conflict. The state and history event are written atomically; a no-op does not create an event. Event snapshots carry server-recorded time, provenance, and the version they supersede. Events and idempotency receipts expire with, and are erased alongside, their source submission.

An owner can explicitly correct one earlier ordinary event by sending its version as `corrects_version`. A correction is a new event recorded at the time it is made; it does not rewrite or backdate the original event. A correction can target only an earlier ordinary event, and that event can be corrected once. A correction cannot itself be corrected; invalid, already-used, or unavailable targets return a conflict. The current disposition values describe the result after the correction, while the linked event identifies what was corrected.

The dashboard keeps follow-up drafts and retry operations in route memory. Drafts remain available when filters or pages remove their submission from the visible list, and a stale save must be explicitly reviewed before another save. An uncertain retry reuses its original request and idempotency key. Discarding a draft or deleting a submission clears its local follow-up state; draft text is not stored in browser storage.

Submissions that predate this journal start at version zero with no history event. Their existing status is preserved, and the system does not infer when contact or qualification occurred. `closed` records an owner follow-up outcome; it never means a sale or payment.

Outcome reports include every still-retained submission in the selected date range in current-status stock. Transition proportions and first-contact delays use one cohort: retained submissions received strictly after the immutable follow-up history boundary. Older records remain visible in stock, while transitions before that boundary are unknown and excluded. A correction replaces its target in report counts at the correction's recorded time; reasons are summarized as counts and are not included in the report response.

The service journey is a working product hypothesis, not a validated result from user research. Test fixtures, sample funnels, and reference receivers use fictional scenarios; demos are not evidence of customer outcomes or conversion uplift.

## Private service brief

The owner may keep an incomplete service brief with the funnel to clarify its audience, need, offer, scope, terms, intended action, promised next step, and available evidence. The owner can choose contact, quote request, or diagnostic candidacy; these intentions do not add booking, payment, scoring, or automated follow-up.

The brief is stored apart from the public document and has its own modification version. Saving against a stale version cannot silently overwrite newer notes; a retry succeeds when its requested contents are still current, and otherwise returns a conflict for the owner to resolve. Only the current brief is retained, not a private edit history. The owner API follows the configured access perimeter and marks every response private and non-cacheable. Brief values are excluded from public HTML, previews, reports, and public revisions. See [ADR 0005](adr/0005-private-service-brief.md).

The system has three independent choices:

- Request acknowledgement is required to process a submission.
- Marketing consent is optional and starts unchecked.
- Measurement consent is optional and controls supported aggregate interaction collection.

## Campaign links and measurement

The owner can register up to 100 campaign IDs per funnel from the dashboard's Campaign links page. IDs use 1–64 ASCII letters, digits, hyphens, or underscores, start with a letter or digit, and match case-sensitively. Generated entry-page links use `?campaign_id=<registered-id>`; a configured custom funnel host takes precedence over `PUBLIC_BASE_URL`. Without a trusted absolute origin, the dashboard displays the relative path and does not invent a hostname.

The interaction collector records a campaign only when measurement consent is granted and the exact ID is registered for that funnel when the event is accepted. Unknown values remain unattributed. This applies to interactions on the current page only; the campaign does not follow the visitor to another page, domain, form submission, or person. `utm_medium` continues to set the separate submission channel and `utm_campaign` does not create campaign attribution.

Adding or removing an ID affects future events. Retained event rows are not backfilled or rewritten when the registry changes, so a replay after registration remains a duplicate with its original attribution. Removing an ID does not erase campaign groups already recorded in retained events. Campaigns are not stored on form submissions, and reports do not join campaign events to people or requests.

## Compatibility and data handling

The versioned rendering, erasure, webhook, and measurement decisions are recorded in [ADR 0001](adr/0001-versioned-rendering-compatibility.md), [ADR 0002](adr/0002-submission-erasure-and-recovery.md), [ADR 0003](adr/0003-webhook-delivery.md), and [ADR 0004](adr/0004-measurement-boundaries.md).

Owners can export and target-delete one retained submission. D1 restore remains blocked by an R2-authoritative gate until its retained erasure journals have been reconciled; see the [submission erasure runbook](runbooks/submission-erasure.md) before restoring D1.

## Delivery tracking

[Issue 82](https://github.com/lianihossam-eng/EezFunnels/issues/82) owns the live checklist for the 21-PR series. The [milestone guide](public-series.md) records the evidence required at each delivery gate; it does not duplicate the issue's per-PR status.
