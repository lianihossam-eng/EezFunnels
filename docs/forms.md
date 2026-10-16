# Public forms

The typed dashboard editor can add a form to a page section, configure its purpose, and edit its fields. Supported fields are text, email, phone, checkbox, and dropdown, with editable submission keys, visible labels, optional help, required status, text-length limits where supported, and dropdown options. Fields can be reordered and removed; every form keeps at least one field. The editor also lets an owner change required request-consent wording and separately enable and word optional marketing consent. Changing required consent wording creates a new copy version for later evidence.

Published V2 documents render these definitions as native HTML forms and accept submissions on the same Worker. The renderer uses native input types and associated labels so browser validation and keyboard interaction work without client-side JavaScript. The dashboard currently configures inline confirmation text; creating multiple pages and selecting a success page remain part of the page-routing work.

There is no dashboard or public API for reading submitted leads. Treat the D1 database and its backups as sensitive because they contain submitted field values and consent evidence.

## Submission behavior

The browser posts URL-encoded values to the same-origin route:

```text
POST /api/public/funnels/:slug/pages/:page_id/forms/:form_id/submissions?key=:uuid
```

The Worker validates the current published revision, verifies the form is placed on the requested page, and checks each submitted value against the field definition. It rejects unknown or repeated fields, invalid select options, missing required values, invalid consent values, and oversized bodies. A successful post redirects with HTTP 303 to the configured result. Reusing the page's submission key is idempotent, so browser retries do not create another lead.

Request acknowledgement is required for every submission. Marketing opt-in is a separate optional checkbox that starts unchecked. The database stores the submitted values, the exact consent copy and version, the acknowledgement and marketing choice, and their timestamps. Submission values are not returned in the redirect or written to application logs.

Each submission also keeps a compact snapshot of its original page title, form purpose, and ordered field names, labels, and kinds. This preserves the meaning of saved answers if a revision is later removed. It does not copy field help or dropdown options, and it does not change the stored answers or consent evidence. Context is removed after no submissions remain for that funnel, revision, page, and form; deleting the whole funnel still removes its submissions and context.

Submissions are retained for 90 days by default and removed by the scheduled cleanup. Set `PUBLIC_FORM_RETENTION_DAYS` to a value from 1 to 3650 to change this period. The Worker allows up to 100 submissions per funnel in a rolling 60-second window by default; set `PUBLIC_FORM_SUBMISSIONS_PER_MINUTE` from 1 to 100 to lower the limit for that funnel.

The response route only accepts POST. Leads currently have no application read surface; use secured, operator-controlled database access until a separately authenticated lead-management feature is available. Protect dashboard and editor routes as described in [Security](security.md).

Run `bash scripts/smoke-pr033-public-forms.sh` to exercise the published-page form flow against local Worker and D1 bindings.
