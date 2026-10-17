# Public forms

Published V2 documents can render native HTML forms and accept submissions on the same Worker. A form has a closed field schema, a required request acknowledgement, optional separate marketing opt-in, and either an inline confirmation or a success-page result. The renderer uses native input types and associated labels so browser validation and keyboard interaction work without client-side JavaScript.

The dashboard does not yet create or edit form definitions, and there is no dashboard or public API for reading submitted leads. Configure a form in a typed V2 document before publishing it. Treat the D1 database and its backups as sensitive because they contain submitted field values and consent evidence.

## Submission behavior

The browser posts URL-encoded values to the same-origin route:

```text
POST /api/public/funnels/:slug/pages/:page_id/forms/:form_id/submissions?key=:uuid
```

If a request includes an `Origin` header, the Worker requires one valid HTTP(S) origin that matches the request scheme and host. It checks this header before reading the form body or accessing D1. Requests without `Origin` remain accepted for non-browser client compatibility. Origin is not authentication because non-browser clients can forge it.

The Worker validates the current published revision, verifies the form is placed on the requested page, and checks each submitted value against the field definition. It rejects unknown or repeated fields, invalid select options, missing required values, invalid consent values, and oversized bodies. A successful post redirects with HTTP 303 to the configured result. Reusing the page's submission key is idempotent, so browser retries do not create another lead.

Request acknowledgement is required for every submission. Marketing opt-in is a separate optional checkbox that starts unchecked. The database stores the submitted values, the exact consent copy and version, the acknowledgement and marketing choice, and their timestamps. Submission values are not returned in the redirect or written to application logs.

Submissions are retained for 90 days by default and removed by the scheduled cleanup. Set `PUBLIC_FORM_RETENTION_DAYS` to a value from 1 to 3650 to change this period. The Worker allows up to 100 submissions per funnel in a rolling 60-second window by default; set `PUBLIC_FORM_SUBMISSIONS_PER_MINUTE` from 1 to 100 to lower the limit for that funnel.

The response route only accepts POST. Leads currently have no application read surface; use secured, operator-controlled database access until a separately authenticated lead-management feature is available. Protect dashboard and editor routes as described in [Security](security.md).

Run `bash scripts/smoke-pr033-public-forms.sh` to exercise the published-page form flow against local Worker and D1 bindings.
