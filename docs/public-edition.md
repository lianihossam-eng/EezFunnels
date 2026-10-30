# Public edition contract

EezFunnels is an open-source funnel builder for one operator's self-hosted Cloudflare account. The operator controls the Worker, D1 database, R2 bucket, KV namespace, public destinations, backups, and access perimeter. The public edition has no built-in login or multi-user roles; protect owner pages, APIs, reports, and the inbox with Cloudflare Access or another network perimeter.

This series covers the complete public, single-owner edition. It excludes hosted SaaS, multiple accounts or roles, a full CRM, email/SMS campaigns, a native calendar, payment processing, subscriptions, member areas, AI generation, and A/B tests. It adds no Cloudflare service.

The product's terms are defined in the [glossary](../GLOSSARY.md). The five application layers and their allowed dependencies are defined in [Architecture](architecture.md).

## Capability snapshot

This table records the capabilities on the series baseline, commit 65feaeee, and the boundaries maintained by the public improvement series. It is a baseline snapshot; issue 82 is the live source for delivery progress.

| Area | Available on the baseline | Boundary maintained by the series |
| --- | --- | --- |
| Visitor pages | Typed multi-page funnels, immutable revisions, preview, publication, SEO metadata, and mapped custom hosts. | A page represents content and navigation; it does not establish a commercial outcome. |
| Public forms | Native text, email, phone, checkbox, and select fields; required request acknowledgement; optional marketing consent; bounded and idempotent submission. | Inbox and owner APIs remain private. Only fields explicitly selected for an activated operator-configured webhook may be sent to its destination. Measurement and marketing consent remain separate from request acknowledgement. |
| Inbox | Private owner access to retained submissions and the statuses new, contacted, qualified, and closed. | Status records follow-up. Closed or completed never implies payment. |
| Delivery | No built-in external delivery integration on the baseline. | The planned integration is an operator-configured, disabled-by-default, signed webhook with bounded retries and no visitor-supplied destination. |
| Reports | Retained form-completion counts and optional, consent-based page/form interaction aggregates. | No visitor or session identity, person-level attribution, visitor-to-submission rate, or commercial attribution is inferred. |
| Portability | No funnel JSON import/export on the baseline. | The planned template format excludes private briefs, submissions, and secrets; import creates a new unpublished funnel. |
| Runtime | Cloudflare Workers with D1, R2, and KV bindings. | The series adds no Cloudflare service and does not verify a remote production deployment. |

## Outcome limits

The public journey may record that a request arrived, that an owner contacted or qualified it, and that the owner documented an outcome. It does not provide a native calendar, reservation system, payment processing, subscription handling, customer account, or full CRM. Do not describe a submitted form as a booking, a qualified request as a sale, or an HTTP webhook acknowledgement as proof of a completed downstream action.

The service journey is a working product hypothesis, not a validated result from user research. Test fixtures, sample funnels, and reference receivers use fictional scenarios; demos are not evidence of customer outcomes or conversion uplift.

The system has three independent choices:

- Request acknowledgement is required to process a submission.
- Marketing consent is optional and starts unchecked.
- Measurement consent is optional and controls supported aggregate interaction collection.

## Compatibility and data handling

The versioned rendering, erasure, webhook, and measurement decisions are recorded in [ADR 0001](adr/0001-versioned-rendering-compatibility.md), [ADR 0002](adr/0002-submission-erasure-and-recovery.md), [ADR 0003](adr/0003-webhook-delivery.md), and [ADR 0004](adr/0004-measurement-boundaries.md).

## Delivery tracking

[Issue 82](https://github.com/lianihossam-eng/EezFunnels/issues/82) owns the live checklist for the 21-PR series. The [milestone guide](public-series.md) records the evidence required at each delivery gate; it does not duplicate the issue's per-PR status.
