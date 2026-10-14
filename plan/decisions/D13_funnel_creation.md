# D13 - Funnel creation snapshots and commands

Status: accepted for PR-009.

PR-009 exposes the closed command IDs `funnel.list`, `funnel.create`, and
`funnel.delete` through `Session::execute`. The Worker remains the HTTP adapter, and the
commands layer owns parameter validation, blueprint construction, versioned snapshot encoding,
and render preflight. The five stable blueprint IDs are `tripwire`, `vsl`, `webinar`, `launch`,
and `lead_gen`; their labels and descriptions come from the domain catalog so the selector and
command parser share one closed list.

Every create operation stores a first immutable revision in the same D1 transaction as its funnel
row. Tripwire starts with Hero, Features, and CTA sections in that order. VSL starts with Hero,
Testimonials, and CTA; webinar with Hero, Features, Testimonials, and CTA; launch with Hero,
Features, Testimonials, CTA, and Footer; lead generation with Hero, Features, CTA, and Footer.
Testimonials contain explicit replacement text rather than invented customer claims. New funnels
start unpublished and use the `minimal` preset.

Revision `data_json` uses a private `FunnelSnapshotV1` envelope with `format_version: 1`,
`schema_version: 1`, the blueprint ID, theme ID, ordered section IDs/types/orders, and an ID-keyed
typed `SectionContent` map. Renderer content uses a closed tagged Serde representation. The separate
`RenderManifestV1` carries the exact revision ID, theme token digest, `renderer-v2`, the section
content digest, and an empty-assets digest. The empty asset manifest is the UTF-8 byte sequence
`eez-assets-manifest-v1`, one NUL byte, then `[]`; its SHA-256 is
`8b3972abe1696375524a8237889ff77fa13664876dd51ede33db93795b8d5cca`. Commands construct domain
objects and render the complete revision before storage accepts the snapshot. C-01's open-ended
serialization format stays outside the domain model.

Create and delete require an `Idempotency-Key`. A new `command_receipts` table stores the stable
command ID, canonical typed parameter JSON, result JSON, and a database-clock expiry 24 hours from
the mutation. Receipt claim and the funnel/revision write or delete share one D1 batch. A matching
replay returns the stored result; a key used by another command or parameter set returns
`conflict/idempotency_payload_mismatch`. The API adds the current request correlation ID after the
stored result is read, so retries retain their own request correlation without repeating effects.

The dashboard's first SSR and hydration render both show the same loading state. The funnel list
loads after hydration through same-origin API calls. Creation success reports the persisted initial
section summary and reloads the list; deletion removes an item only after the API succeeds. This PR
does not add a draft pointer: PR-009's create response identifies the initial revision, while the
revision editing/read path is part of the editor work that follows.

This accepts one receipt migration and one versioned snapshot envelope in exchange for atomic
creation, retry-safe mutations, and a reproducible first render. The snapshot format can evolve by
adding a new format version; existing revision rows remain immutable.
