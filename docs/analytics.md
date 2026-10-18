# Interaction measurement contract

This document describes first-party interaction measurement and the boundaries of the owner report. When the Worker has a valid `INTERACTION_REPLAY_KEY`, published pages offer a separate choice and can record page and form interaction events. Without that secret, the measurement interface is not added and no interaction events are accepted. **Form completions** continues to count accepted form submissions only; these events do not change that report.

## Consent and event shape

The first-party interaction script presents **Allow measurement** and **No thanks** controls. It sends no interaction event until the visitor chooses to allow measurement. The visitor can reopen **Privacy choices** and withdraw consent; that stops future events. A grant or refusal is saved as one value in this browser's `localStorage`, without a visitor identifier. If browser storage is unavailable, the choice applies to the current page only. Loading the same-origin script does not send an event. The event request is same-origin, omits credentials and referrer data, and contains no form values.

This choice is independent of the acknowledgement required to process a form and the optional marketing opt-in. Marketing consent does not enable analytics, and submitting a form does not imply analytics consent. The collector also requires an exact same-origin `Origin` and an explicit grant marker; requests with missing or refused consent return without reading or storing their body.

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

`observed_at_unix_seconds` comes from the client and is untrusted. D1 supplies `received_at`; only that server-side timestamp determines report dates and expiration. A delayed event is assigned to its receipt date and is never backdated from the client clock. The current V1 renderer uses the fixed page reference `legacy-entry`; V2 pages and forms use their published stable IDs.

The contract has no visitor, session, device, IP address, user agent, full URL, arbitrary query string, or referrer field. The script reads only the named `campaign_id` parameter after consent. The value is retained only when it exactly matches an owner-configured ID; otherwise attribution is unknown. No campaign IDs are configured by the current product, so incoming campaign IDs are discarded. Other campaign parameters and referrer data are discarded. A campaign ID is a label, not proof of source or identity.

## Replay, retention, and deletion

An exact repeat of an event ID and payload is idempotent: it does not add another count, change the stored attribution, or extend the original expiration time. Reuse of an event ID with different event data is rejected. To compare retries without storing unregistered campaign input, the receiver computes an HMAC-SHA-256 fingerprint of the validated fields before attribution filtering, using the random installation-specific `INTERACTION_REPLAY_KEY` secret of at least 32 bytes. Only that fingerprint is retained for replay detection; it is not used for reports and expires with the event. Keep the secret unchanged while any rows created with it may remain (up to 90 days); key rotation is not automated.

The contract defaults to 30 days of event retention. Set `PUBLIC_INTERACTION_RETENTION_DAYS` to a value from 1 through 90; the Worker rejects invalid values. The default per-funnel rate limit is 100 events per minute and can be changed with `PUBLIC_INTERACTION_EVENTS_PER_MINUTE`. D1 calculates expiry from trusted `received_at`; an event expires when `expires_at <= now`. The scheduled cleanup runs every 15 minutes. Removing a funnel cascades to its event records. Because events carry no visitor or session identifier, an individual visitor's older events cannot be located for targeted deletion. Withdrawing interaction consent stops future events but does not identify earlier events for immediate deletion; those expire on schedule.

## Reports and limits

The owner report is available from each funnel's dashboard editor and `GET /api/funnels/:funnel_id/interaction-report?period=7|30|90`. It accepts only fixed 7, 30, or 90 UTC calendar dates, includes the current date as a partial day, and aggregates retained events by server receipt date and event kind. It also groups events by an exact campaign ID only when the collector retained that registered ID. No campaign IDs are currently configured, so visitor-supplied values are discarded and events remain unattributed.

The endpoint and dashboard view are limited to the owner boundary and use `Cache-Control: private, no-store`. The response contains aggregate counts only; it does not expose event IDs, replay fingerprints, raw rows, form IDs, or form values. It does not claim unique visitors, sessions, visitor journeys, or conversion rates. Event capture can be refused, delayed, or absent, and accepted form submissions remain the only source of completion counts. This measurement design does not establish legal compliance.

The domain and Worker tests enforce consent gating, schema and payload limits, current published resource references, exact campaign matching, replay behavior, receipt-time reporting, and expiration boundaries. These local tests do not establish legal compliance.
