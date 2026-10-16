CREATE TABLE public_form_contexts (
    funnel_id TEXT NOT NULL REFERENCES funnels(id) ON DELETE CASCADE,
    revision_id TEXT NOT NULL,
    page_id TEXT NOT NULL CHECK (length(page_id) BETWEEN 1 AND 64),
    form_id TEXT NOT NULL CHECK (length(form_id) BETWEEN 1 AND 64),
    context_json TEXT NOT NULL
        CHECK (json_valid(context_json))
        CHECK (json_type(context_json) = 'object')
        CHECK (length(CAST(context_json AS BLOB)) <= 1048576),
    PRIMARY KEY (funnel_id, revision_id, page_id, form_id)
);

CREATE TRIGGER public_form_contexts_are_immutable
BEFORE UPDATE ON public_form_contexts
BEGIN
    SELECT RAISE(ABORT, 'public form contexts are immutable');
END;

WITH submission_keys AS (
    SELECT DISTINCT funnel_id, revision_id, page_id, form_id
    FROM public_form_submissions
), reconstructed AS (
    SELECT
        keys.funnel_id,
        keys.revision_id,
        keys.page_id,
        keys.form_id,
        (
            SELECT json_object(
                'page_title', json_extract(page.value, '$.title'),
                'form_purpose', json_extract(form.value, '$.purpose'),
                'fields', json(COALESCE((
                    SELECT json_group_array(json(field_json))
                    FROM (
                        SELECT json_object(
                            'name', json_extract(field.value, '$.name'),
                            'label', json_extract(field.value, '$.label'),
                            'kind', json_extract(field.value, '$.kind')
                        ) AS field_json
                        FROM json_each(form.value, '$.fields') AS field
                        ORDER BY CAST(field.key AS INTEGER)
                    )
                ), '[]'))
            )
            FROM revisions
            JOIN json_each(revisions.data_json, '$.pages') AS page
            JOIN json_each(revisions.data_json, '$.forms') AS form
            WHERE revisions.id = keys.revision_id
                AND revisions.funnel_id = keys.funnel_id
                AND revisions.schema_version = 2
                AND json_extract(page.value, '$.id') = keys.page_id
                AND json_type(page.value, '$.title') = 'text'
                AND length(json_extract(page.value, '$.title')) > 0
                AND json_extract(form.value, '$.id') = keys.form_id
                AND json_type(form.value, '$.purpose') = 'text'
                AND length(json_extract(form.value, '$.purpose')) > 0
                AND json_type(form.value, '$.fields') = 'array'
                AND EXISTS (
                    SELECT 1
                    FROM json_each(page.value, '$.sections') AS section
                    JOIN json_each(section.value, '$.blocks') AS block
                    WHERE json_extract(block.value, '$.kind') = 'form'
                        AND json_extract(block.value, '$.form_id') = keys.form_id
                )
                AND EXISTS (
                    SELECT 1 FROM json_each(form.value, '$.fields')
                )
                AND NOT EXISTS (
                    SELECT 1
                    FROM json_each(form.value, '$.fields') AS field
                    WHERE json_type(field.value, '$.name') IS NOT 'text'
                        OR length(json_extract(field.value, '$.name')) = 0
                        OR json_type(field.value, '$.label') IS NOT 'text'
                        OR length(json_extract(field.value, '$.label')) = 0
                        OR json_type(field.value, '$.kind') IS NOT 'text'
                        OR json_extract(field.value, '$.kind') NOT IN ('text', 'email', 'phone', 'checkbox', 'select')
                )
        ) AS context_json
    FROM submission_keys AS keys
)
INSERT INTO public_form_contexts (funnel_id, revision_id, page_id, form_id, context_json)
SELECT funnel_id, revision_id, page_id, form_id, context_json
FROM reconstructed
WHERE context_json IS NOT NULL
ON CONFLICT (funnel_id, revision_id, page_id, form_id) DO NOTHING;

CREATE TABLE public_form_context_migration_guard (
    valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO public_form_context_migration_guard (valid)
SELECT 0
WHERE EXISTS (
    SELECT 1
    FROM public_form_submissions AS submissions
    WHERE submissions.expires_at > unixepoch()
        AND NOT EXISTS (
            SELECT 1
            FROM public_form_contexts AS contexts
            WHERE contexts.funnel_id = submissions.funnel_id
                AND contexts.revision_id = submissions.revision_id
                AND contexts.page_id = submissions.page_id
                AND contexts.form_id = submissions.form_id
        )
);

DROP TABLE public_form_context_migration_guard;

ALTER TABLE public_form_submissions RENAME TO public_form_submissions_before_context;

CREATE TABLE public_form_submissions (
    submission_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(submission_id) = 36),
    funnel_id TEXT NOT NULL REFERENCES funnels(id) ON DELETE CASCADE,
    revision_id TEXT NOT NULL,
    page_id TEXT NOT NULL CHECK (length(page_id) BETWEEN 1 AND 64),
    form_id TEXT NOT NULL CHECK (length(form_id) BETWEEN 1 AND 64),
    submission_key TEXT NOT NULL CHECK (length(submission_key) = 36),
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

INSERT INTO public_form_submissions (
    submission_id, funnel_id, revision_id, page_id, form_id, submission_key,
    field_values_json, request_consent_acknowledged, request_consent_copy_version,
    request_consent_copy, request_consent_recorded_at, marketing_opt_in,
    marketing_opt_in_copy_version, marketing_opt_in_copy,
    marketing_opt_in_recorded_at, submitted_at, expires_at
)
SELECT
    submission_id, funnel_id, revision_id, page_id, form_id, submission_key,
    field_values_json, request_consent_acknowledged, request_consent_copy_version,
    request_consent_copy, request_consent_recorded_at, marketing_opt_in,
    marketing_opt_in_copy_version, marketing_opt_in_copy,
    marketing_opt_in_recorded_at, submitted_at, expires_at
FROM public_form_submissions_before_context;

DROP TABLE public_form_submissions_before_context;

CREATE INDEX public_form_submissions_rate_idx
    ON public_form_submissions(funnel_id, submitted_at);

CREATE INDEX public_form_submissions_expiry_idx
    ON public_form_submissions(expires_at, submission_id);

CREATE INDEX public_form_submissions_context_idx
    ON public_form_submissions(funnel_id, revision_id, page_id, form_id);

CREATE TRIGGER public_form_submission_must_match_current_revision
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

CREATE TRIGGER public_form_submission_requires_context
BEFORE INSERT ON public_form_submissions
WHEN NOT EXISTS (
    SELECT 1
    FROM public_form_contexts
    WHERE funnel_id = NEW.funnel_id
        AND revision_id = NEW.revision_id
        AND page_id = NEW.page_id
        AND form_id = NEW.form_id
)
BEGIN
    SELECT RAISE(ABORT, 'form submission requires matching public form context');
END;
