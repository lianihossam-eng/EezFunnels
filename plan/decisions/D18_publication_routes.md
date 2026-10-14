# D18 - Receipt-backed publication and public rendering

Status: accepted for PR-014.

PR-014 adds the planned `POST /api/funnels/:id/publish`,
`POST /api/funnels/:id/unpublish`, and `GET /f/:slug` routes without changing frozen
C-01, C-03, C-04, or the database schema. The existing `funnels.published_revision_id`
pointer, same-funnel ownership triggers, immutable revisions, and `render_manifest`
already represent the publication state. No migration or remote database operation is
needed.

Publish accepts a closed JSON object containing `revision_id` and
`expected_published_revision_id` (nullable to represent an unpublished baseline).
Unpublish accepts the required, non-null `expected_published_revision_id`. The funnel ID
comes only from the route. Both commands require `Idempotency-Key`. Commands validate
the target revision's funnel ownership, supported schema, stored snapshot, and manifest
before mutation. A publish or unpublish compares the caller's expected published pointer
and returns the existing `conflict/publish_stale_revision` code when the state changed.
Publishing the current target is treated as a stale/no-op conflict, matching the existing
repository publication invariant.

The receipt claim and pointer change are one D1 batch. The receipt is inserted only when
the funnel exists, the target is owned and supported, and the expected pointer still
matches; the pointer update is gated by the exact receipt payload and repeats the CAS.
The pointer update also requires that this batch newly claimed one receipt row. A matching
active receipt already stored by an earlier batch can replay its result but cannot authorize
another pointer mutation, including after the pointer changes away and later returns to the
request's expected baseline.
Unpublish uses the same receipt-first replay behavior and CAS, setting the pointer to
NULL. A loser creates no receipt and no partial mutation. An active same-key/same-payload
receipt replays the stored response before current state is checked; key or payload reuse
returns `conflict/idempotency_payload_mismatch`. This keeps uncertain retries safe while
preventing a stale request from recording success.

The HTTP adapter rejects duplicate JSON object keys before normalizing the body. For an
ambiguous body it classifies the key without a payload: an active receipt returns the
existing idempotency mismatch, and a fresh key returns `validation/invalid_json`.
The adapter injects the route ID into the command before typed parsing, so command execution
can classify an active key before returning a route error. An invalid route ID cannot mutate
state: an active key mismatches against the different injected route ID, while a fresh request
returns `not_found/funnel`.

Public lookup performs one joined read from slug to its currently published immutable
revision. Missing and unpublished slugs both return `not_found/slug`. The command decoder
validates the stored snapshot and `render_manifest`, and `Renderer` produces the public
HTML; client content, HTML, theme tokens, and renderer metadata never enter the public
request. The response uses the revision ID as a strong quoted ETag and
`Cache-Control: public, max-age=0, must-revalidate`. Conditional requests are evaluated
only after confirming the slug still has a published revision, so unpublishing cannot
turn a stale ETag into a 304. A newly published revision naturally receives a different
ETag. This does not promise CDN purge or immediate edge eviction.
If a persisted published revision has an unsupported schema version, the public route
returns `validation/schema_version_unsupported` with HTTP 400.

Verification covers receipt replay and mismatch, successful and stale CAS, owned-revision
isolation, atomic publish/unpublish state, rendering from the stored manifest, ETag and
304 behavior, ETag changes after republishing, and 404 after unpublish. Worker/D1 checks
are local only; no production deploy or remote database operation is part of this PR.
