# Deliver submissions through an operator-configured signed webhook

Status: accepted for the public improvement series.

Support one HTTPS destination configured by the installation operator, disabled by default, with per-form field selection and versioned destination/mapping definitions. Commit a delivery task atomically with an accepted submission, store immutable source references rather than a second persistent copy of submitted values, and reconstruct a deterministic body from the retained source. Sign each request with HMAC-SHA-256 using a separate Worker secret; reject redirects and bound requests to a 10-second timeout, five attempts within 24 hours, and the source's retention expiry. Do not backfill old submissions or retarget an existing task when configuration changes. A 2xx response means only that the receiver accepted the request; duplicates remain possible and the receiver owns deduplication.
