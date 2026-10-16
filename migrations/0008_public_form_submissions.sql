CREATE TABLE IF NOT EXISTS public_form_submissions (
    submission_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(submission_id) = 36),
    funnel_id TEXT NOT NULL REFERENCES funnels(id) ON DELETE CASCADE,
    revision_id TEXT NOT NULL REFERENCES revisions(id) ON DELETE CASCADE,
    page_id TEXT NOT NULL CHECK (length(page_id) BETWEEN 1 AND 64),
    form_id TEXT NOT NULL CHECK (length(form_id) BETWEEN 1 AND 64),
    submission_key TEXT NOT NULL CHECK (length(submission_key) = 36),
    request_fingerprint TEXT NOT NULL CHECK (length(request_fingerprint) = 64),
    field_values_json TEXT NOT NULL
        CHECK (json_valid(field_values_json))
        CHECK (json_type(field_values_json) = 'object')
        CHECK (length(CAST(field_values_json AS BLOB)) <= 524288),
    request_consent_acknowledged INTEGER NOT NULL CHECK (request_consent_acknowledged = 1),
    request_consent_copy_version TEXT NOT NULL
        CHECK (length(request_consent_copy_version) BETWEEN 1 AND 64),
    request_consent_copy TEXT NOT NULL
        CHECK (length(request_consent_copy) BETWEEN 1 AND 1024),
    request_consent_recorded_at INTEGER NOT NULL,
    marketing_opt_in INTEGER NOT NULL DEFAULT 0 CHECK (marketing_opt_in IN (0, 1)),
    marketing_opt_in_copy_version TEXT
        CHECK (marketing_opt_in_copy_version IS NULL OR length(marketing_opt_in_copy_version) BETWEEN 1 AND 64),
    marketing_opt_in_copy TEXT
        CHECK (marketing_opt_in_copy IS NULL OR length(marketing_opt_in_copy) BETWEEN 1 AND 1024),
    marketing_opt_in_recorded_at INTEGER,
    submitted_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL CHECK (expires_at > submitted_at),
    UNIQUE (funnel_id, form_id, submission_key),
    CHECK ((marketing_opt_in_copy_version IS NULL) = (marketing_opt_in_copy IS NULL)),
    CHECK ((marketing_opt_in_copy_version IS NULL) = (marketing_opt_in_recorded_at IS NULL)),
    CHECK (marketing_opt_in = 0 OR marketing_opt_in_copy_version IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS public_form_submissions_rate_idx
    ON public_form_submissions(funnel_id, submitted_at);

CREATE INDEX IF NOT EXISTS public_form_submissions_expiry_idx
    ON public_form_submissions(expires_at, submission_id);

CREATE TRIGGER IF NOT EXISTS public_form_submission_must_match_current_revision
BEFORE INSERT ON public_form_submissions
WHEN NOT EXISTS (
    SELECT 1
    FROM funnels
    JOIN revisions ON revisions.id = funnels.published_revision_id
        AND revisions.funnel_id = funnels.id
    WHERE funnels.id = NEW.funnel_id
        AND funnels.published_revision_id = NEW.revision_id
        AND revisions.schema_version = 2
)
BEGIN
    SELECT RAISE(ABORT, 'form submission must match the current published revision');
END;
