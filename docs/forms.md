# Public forms

The typed dashboard editor can add a form to a page section, configure its purpose, and edit its fields. Supported fields are text, email, phone, checkbox, and dropdown, with editable submission keys, visible labels, optional help, required status, text-length limits where supported, and dropdown options. Fields can be reordered and removed; every form keeps at least one field. The editor also lets an owner change required request-consent wording and separately enable and word optional marketing consent. Changing required consent wording creates a new copy version for later evidence.

Published V2 documents render these definitions as native HTML forms and accept submissions on the same Worker. The renderer uses native input types and associated labels so browser validation and keyboard interaction work without client-side JavaScript. The typed editor can add a page from the currently selected page by supplying its title and a unique public route slug. It creates an editable Hero section and adds an internal navigation button to the source page in the same edit. The slug determines the public route, such as `/f/:funnel_slug/thanks`. Button actions can point to an HTTPS URL or to another page by its stable page ID; the document validator rejects edits that create a route loop or leave a page unreachable.

The home page cannot be deleted. Deleting another page removes navigation buttons that point to it, resets any form result that points to it to the standard inline confirmation, removes forms placed only on that page, and then validates the remaining page graph. If the deletion would make another page unreachable, the whole edit is rejected and the draft stays unchanged. Choosing a success page in the form editor remains a follow-up to this general page-routing work.

The owner API and dashboard provide a read-only lead inbox and form-completion report. Treat the D1 database and its backups as sensitive because they contain submitted field values and consent evidence. These owner routes have no application login or per-user authorization, so keep them behind the external Cloudflare Access or equivalent perimeter described in [Security](security.md).

## Submission behavior

The browser posts URL-encoded values to the same-origin route:

```text
POST /api/public/funnels/:slug/pages/:page_id/forms/:form_id/submissions?key=:uuid
```

The Worker validates the current published revision, verifies the form is placed on the requested page, and checks each submitted value against the field definition. It rejects unknown or repeated fields, invalid select options, missing required values, invalid consent values, and oversized bodies. A successful post redirects with HTTP 303 to the configured result. Reusing the page's submission key is idempotent, so browser retries do not create another lead.

Request acknowledgement is required for every submission. Marketing opt-in is a separate optional checkbox that starts unchecked. The database stores the submitted values, the exact consent copy and version, the acknowledgement and marketing choice, and their timestamps. Submission values are not returned in the redirect or written to application logs.

Each submission also keeps a compact snapshot of its original page title, form purpose, and ordered field names, labels, and kinds. This preserves the meaning of saved answers if a revision is later removed. It does not copy field help or dropdown options, and it does not change the stored answers or consent evidence. Deleting a source revision does not delete its submissions or historical context; they remain readable through the owner lead API until their normal expiry. Context is removed after no submissions remain for that funnel, revision, page, and form; deleting the whole funnel still removes its submissions and context.

Submissions are retained for 90 days by default and removed by the scheduled cleanup. Set `PUBLIC_FORM_RETENTION_DAYS` to a value from 1 to 3650 to change this period. The Worker allows up to 100 submissions per funnel in a rolling 60-second window by default; set `PUBLIC_FORM_SUBMISSIONS_PER_MINUTE` from 1 to 100 to lower the limit for that funnel.

## Owner lead API

The read-only owner routes are:

```text
GET /api/funnels/:funnel_id/leads?cursor=:opaque_cursor
GET /api/funnels/:funnel_id/leads/:submission_id
```

The list returns at most 50 newest unexpired leads, ordered by submission time and submission UUID. Its projection contains page/form context and the marketing choice, but no submitted answers, consent wording, source revision IDs, or idempotency keys. Follow `next_cursor` until it is `null`. Detail returns answers in their original form order and the exact request and marketing consent evidence. A configured marketing consent copy is retained in detail even when the visitor left its checkbox unchecked. Missing, expired, or cross-funnel details share the same not-found response. Responses use `Cache-Control: no-store` and do not include an ETag.

These are owner routes without application authentication. Keep `/api/funnels/:funnel_id/leads*` behind Cloudflare Access or another external perimeter; `ALLOWED_HOSTS` only checks the request host. The API does not add user accounts, roles, dashboard state, exports, or write operations. Protect dashboard and editor routes as described in [Security](security.md).

## Form-completion analytics

The owner report is read-only and accepts only these bounded UTC calendar windows:

```text
GET /api/funnels/:funnel_id/analytics/forms?days=7
GET /api/funnels/:funnel_id/analytics/forms?days=30
GET /api/funnels/:funnel_id/analytics/forms?days=90
```

Each report counts successful submissions that have not expired at one server-side snapshot time. It returns zero-filled daily counts, the total, up to 20 page/form-purpose label pairs, and a combined count for the remaining labels. Labels come from the published context saved with submissions; long labels are shortened for display. The result contains no answers, consent evidence, visitor values, submission IDs, or revision IDs. Every outcome uses `Cache-Control: no-store` and omits ETags.

The report measures stored submissions only. It has no page-visit or form-start denominator, so its counts are not conversion rates. Expired submissions are excluded and retention can make earlier dates incomplete. If a report exceeds the synchronous processing limit, the API returns an unavailable-report error instead of partial counts; a shorter period may fit, but is not guaranteed to.

These analytics routes are owner routes without application authentication. Keep `/api/funnels/:funnel_id/analytics*` behind Cloudflare Access or another external perimeter; `ALLOWED_HOSTS` only checks the request host. Aggregate counts still belong to the protected owner surface.

The dashboard exposes a per-funnel, read-only **Leads** inbox and **Form completions** report from the funnel list and lead inbox. The inbox shows recent submissions, historical field labels and values, and the stored consent evidence. The report shows retained submission totals and UTC day buckets. Neither surface adds notes, statuses, assignments, exports, or automation. Deleting a funnel also deletes its saved submissions and consent evidence; the confirmation names this data loss.

Run `bash scripts/smoke-pr033-public-forms.sh` to exercise the published-page form flow against local Worker and D1 bindings.
