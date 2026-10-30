CREATE TABLE IF NOT EXISTS webhook_destination_versions (
    destination_version_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(destination_version_id) = 36),
    destination_url TEXT NOT NULL
        CHECK (length(destination_url) BETWEEN 9 AND 2048)
        CHECK (substr(destination_url, 1, 8) = 'https://'),
    created_at INTEGER NOT NULL
);

CREATE TRIGGER IF NOT EXISTS webhook_destination_versions_immutable
BEFORE UPDATE ON webhook_destination_versions
BEGIN
    SELECT RAISE(ABORT, 'webhook destination versions are immutable');
END;

CREATE TABLE IF NOT EXISTS webhook_installation_delivery (
    installation_id INTEGER PRIMARY KEY NOT NULL CHECK (installation_id = 1),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    destination_version_id TEXT REFERENCES webhook_destination_versions(destination_version_id),
    CHECK (enabled = 0 OR destination_version_id IS NOT NULL)
);

INSERT OR IGNORE INTO webhook_installation_delivery (
    installation_id, enabled, destination_version_id
) VALUES (1, 0, NULL);

CREATE TABLE IF NOT EXISTS public_form_delivery_mapping_versions (
    mapping_version_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(mapping_version_id) = 36),
    funnel_id TEXT NOT NULL REFERENCES funnels(id) ON DELETE CASCADE,
    form_id TEXT NOT NULL CHECK (length(form_id) BETWEEN 1 AND 64),
    destination_version_id TEXT NOT NULL
        REFERENCES webhook_destination_versions(destination_version_id),
    selected_fields_json TEXT NOT NULL
        CHECK (json_valid(selected_fields_json))
        CHECK (json_type(selected_fields_json) = 'array')
        CHECK (length(CAST(selected_fields_json AS BLOB)) <= 16384),
    created_at INTEGER NOT NULL,
    UNIQUE (mapping_version_id, funnel_id, form_id, destination_version_id)
);

CREATE TRIGGER IF NOT EXISTS public_form_delivery_mapping_versions_immutable
BEFORE UPDATE ON public_form_delivery_mapping_versions
BEGIN
    SELECT RAISE(ABORT, 'webhook mapping versions are immutable');
END;

CREATE TABLE IF NOT EXISTS public_form_delivery_settings (
    funnel_id TEXT NOT NULL REFERENCES funnels(id) ON DELETE CASCADE,
    form_id TEXT NOT NULL CHECK (length(form_id) BETWEEN 1 AND 64),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    expected_destination_version_id TEXT,
    mapping_version_id TEXT,
    PRIMARY KEY (funnel_id, form_id),
    CHECK ((expected_destination_version_id IS NULL) = (mapping_version_id IS NULL)),
    CHECK (enabled = 0 OR mapping_version_id IS NOT NULL),
    FOREIGN KEY (
        mapping_version_id, funnel_id, form_id, expected_destination_version_id
    ) REFERENCES public_form_delivery_mapping_versions (
        mapping_version_id, funnel_id, form_id, destination_version_id
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS public_form_submissions_delivery_source_idx
    ON public_form_submissions(submission_id, funnel_id, revision_id, page_id, form_id);

CREATE TABLE IF NOT EXISTS public_submission_delivery_outbox (
    task_id TEXT PRIMARY KEY NOT NULL CHECK (length(task_id) = 36),
    source_submission_id TEXT NOT NULL UNIQUE,
    funnel_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    page_id TEXT NOT NULL,
    form_id TEXT NOT NULL,
    destination_version_id TEXT NOT NULL,
    mapping_version_id TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state = 'pending'),
    created_at INTEGER NOT NULL,
    CHECK (task_id = source_submission_id),
    FOREIGN KEY (source_submission_id, funnel_id, revision_id, page_id, form_id)
        REFERENCES public_form_submissions(submission_id, funnel_id, revision_id, page_id, form_id)
        ON DELETE CASCADE,
    FOREIGN KEY (destination_version_id)
        REFERENCES webhook_destination_versions(destination_version_id),
    FOREIGN KEY (mapping_version_id, funnel_id, form_id, destination_version_id)
        REFERENCES public_form_delivery_mapping_versions(
            mapping_version_id, funnel_id, form_id, destination_version_id
        )
);

CREATE INDEX IF NOT EXISTS public_submission_delivery_outbox_created_idx
    ON public_submission_delivery_outbox(created_at, task_id);

CREATE TRIGGER IF NOT EXISTS public_submission_delivery_outbox_references_immutable
BEFORE UPDATE OF task_id, source_submission_id, funnel_id, revision_id, page_id,
    form_id, destination_version_id, mapping_version_id, created_at
ON public_submission_delivery_outbox
WHEN NEW.task_id IS NOT OLD.task_id
    OR NEW.source_submission_id IS NOT OLD.source_submission_id
    OR NEW.funnel_id IS NOT OLD.funnel_id
    OR NEW.revision_id IS NOT OLD.revision_id
    OR NEW.page_id IS NOT OLD.page_id
    OR NEW.form_id IS NOT OLD.form_id
    OR NEW.destination_version_id IS NOT OLD.destination_version_id
    OR NEW.mapping_version_id IS NOT OLD.mapping_version_id
    OR NEW.created_at IS NOT OLD.created_at
BEGIN
    SELECT RAISE(ABORT, 'webhook delivery references are immutable');
END;

CREATE TRIGGER IF NOT EXISTS public_form_submission_delivery_source_immutable
BEFORE UPDATE OF submission_id, funnel_id, revision_id, page_id, form_id,
    submission_channel, submission_key, request_fingerprint, field_values_json,
    request_consent_acknowledged, request_consent_copy_version, request_consent_copy,
    request_consent_recorded_at, marketing_opt_in, marketing_opt_in_copy_version,
    marketing_opt_in_copy, marketing_opt_in_recorded_at, submitted_at, expires_at
ON public_form_submissions
WHEN EXISTS (
    SELECT 1
    FROM public_submission_delivery_outbox AS task
    WHERE task.source_submission_id = OLD.submission_id
)
AND (
    NEW.submission_id IS NOT OLD.submission_id
    OR NEW.funnel_id IS NOT OLD.funnel_id
    OR NEW.revision_id IS NOT OLD.revision_id
    OR NEW.page_id IS NOT OLD.page_id
    OR NEW.form_id IS NOT OLD.form_id
    OR NEW.submission_channel IS NOT OLD.submission_channel
    OR NEW.submission_key IS NOT OLD.submission_key
    OR NEW.request_fingerprint IS NOT OLD.request_fingerprint
    OR NEW.field_values_json IS NOT OLD.field_values_json
    OR NEW.request_consent_acknowledged IS NOT OLD.request_consent_acknowledged
    OR NEW.request_consent_copy_version IS NOT OLD.request_consent_copy_version
    OR NEW.request_consent_copy IS NOT OLD.request_consent_copy
    OR NEW.request_consent_recorded_at IS NOT OLD.request_consent_recorded_at
    OR NEW.marketing_opt_in IS NOT OLD.marketing_opt_in
    OR NEW.marketing_opt_in_copy_version IS NOT OLD.marketing_opt_in_copy_version
    OR NEW.marketing_opt_in_copy IS NOT OLD.marketing_opt_in_copy
    OR NEW.marketing_opt_in_recorded_at IS NOT OLD.marketing_opt_in_recorded_at
    OR NEW.submitted_at IS NOT OLD.submitted_at
    OR NEW.expires_at IS NOT OLD.expires_at
)
BEGIN
    SELECT RAISE(ABORT, 'queued public submission source is immutable');
END;

CREATE VIEW IF NOT EXISTS retained_visible_public_submission_delivery AS
SELECT task.task_id, task.source_submission_id, task.funnel_id, task.revision_id,
    task.page_id, task.form_id, task.destination_version_id, task.mapping_version_id,
    task.state, task.created_at
FROM public_submission_delivery_outbox AS task
JOIN public_form_submissions AS source
    ON source.submission_id = task.source_submission_id
WHERE source.expires_at > unixepoch()
    AND NOT EXISTS (
        SELECT 1
        FROM public_submission_erasure_intents AS intent
        WHERE intent.submission_id = source.submission_id
    );
