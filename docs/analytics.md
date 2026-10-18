# Interaction measurement contract

This document records the bounded contract for a possible future interaction report. It does not mean that the product currently collects, stores, or reports interaction events. Today, **Form completions** counts accepted form submissions only; page views, form views, starts, visitors, and sessions are not tracked.

## Consent and event shape

An interaction event may be created only while the visitor has explicitly granted a separate interaction-measurement choice. Missing or refused consent produces no event. This choice is independent of the acknowledgement required to process a form and the optional marketing opt-in. Marketing consent does not enable analytics, and submitting a form does not imply analytics consent.

The Rust domain contract accepts schema version `1` and only these observed actions: `page_viewed`, `form_viewed`, and `form_started`. A page event has a funnel, published revision, and page reference. Form events also have a form reference. Stable references are limited to 64 ASCII letters, digits, `_`, or `-`. Event IDs are canonical random UUIDv4 values. The JSON payload is limited to 2,048 bytes and rejects unknown fields.

```json
{
  "schema_version": 1,
  "event_id": "018f47a2-8b3c-4d61-8e2a-7f4c9b12d6a0",
  "kind": "page_viewed",
  "observed_at_unix_seconds": 1781740800,
  "funnel_id": "funnel-public",
  "revision_id": "revision-2",
  "page_id": "page-home",
  "campaign_id": "spring-launch"
}
```

`observed_at_unix_seconds` comes from the client and is untrusted. The receiving server supplies `received_at`; only that server timestamp determines report dates and expiration. A delayed event is assigned to its receipt date and is never backdated from the client clock.

The contract has no visitor, session, device, IP address, user agent, full URL, arbitrary query string, or referrer field. A collector may read only the named `campaign_id` parameter. The value is retained only when it exactly matches an owner-configured ID; otherwise attribution is unknown. Other campaign parameters and referrer data are discarded. A campaign ID is a label, not proof of source or identity.

## Replay, retention, and deletion

An exact repeat of an event ID and payload is idempotent: it does not add another count, change the stored attribution, or extend the original expiration time. Reuse of an event ID with different event data is rejected. To compare retries without storing unregistered campaign input, the receiver computes an HMAC-SHA-256 fingerprint of the validated fields before attribution filtering, using a random installation-specific server key of at least 32 bytes. Only that fingerprint is retained for replay detection; it is not used for reports and expires with the event. The key must remain available for at least the maximum retention period, including after rotation. The database implementation is not part of this slice; any future writer must enforce these rules atomically.

The contract defaults to 30 days of event retention. An owner may choose 1 through 90 days, and no event may be retained longer than 90 days. Expiration is calculated from trusted `received_at`; an event is expired when `expires_at <= now`. Removing a funnel must also remove its event records. Because events carry no visitor or session identifier, an individual visitor's older events cannot be located for targeted deletion. Withdrawing interaction consent stops future events but does not identify earlier events for immediate deletion; those expire on schedule. The current product has no interaction-event store to delete.

## Reports and limits

An eventual owner report is limited to fixed 7, 30, or 90 UTC-day windows and aggregated counts by event kind and configured campaign ID. It does not expose raw event rows or claim unique visitors or sessions. These counts cannot be divided into form completions to claim a conversion rate: event capture can be refused, delayed, or absent, and accepted form submissions remain the only source of completion counts. The report must use the same private access boundary and `Cache-Control: private, no-store` behavior as the submissions and completion APIs.

The domain contract tests enforce consent gating, schema and payload limits, identifier boundaries, exact campaign matching, replay behavior, receipt-time reporting, and expiration boundaries. These local tests do not establish legal compliance. No interaction collection, event persistence, dashboard report, or deployed tracking is included in this change.
