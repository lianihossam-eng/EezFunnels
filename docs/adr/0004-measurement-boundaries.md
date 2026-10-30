# Keep campaign measurement page-scoped and consent-based

Status: accepted for the public improvement series.

Accept only exact configured campaign labels for the current page after measurement consent; keep utm_medium-derived channel separate and add no propagation across pages or domains, visitor/session identifier, or campaign-to-submission link. Reports describe their as-of time, collection state, retention, partial day, and eligible retained cohort. Do not present unique visitors, visitor-to-submission rates, person-level attribution, or commercial outcomes: the stored events and retained submissions do not establish those relationships.
