CREATE TABLE IF NOT EXISTS v2_document_stages (
    stage_id_hash TEXT PRIMARY KEY NOT NULL
        CHECK (length(stage_id_hash) = 64 AND stage_id_hash NOT GLOB '*[^0-9a-f]*'),
    funnel_id TEXT NOT NULL REFERENCES funnels(id) ON DELETE CASCADE,
    base_revision_id TEXT NOT NULL REFERENCES revisions(id),
    purpose TEXT NOT NULL CHECK (purpose = 'full_snapshot'),
    document_type TEXT NOT NULL CHECK (document_type = 'eezfunnel.document'),
    document_version INTEGER NOT NULL CHECK (document_version = 2),
    schema_version INTEGER NOT NULL CHECK (schema_version = 2),
    manifest_version INTEGER NOT NULL CHECK (manifest_version = 2),
    maximum_bytes INTEGER NOT NULL CHECK (maximum_bytes = 1048576),
    expected_length INTEGER NOT NULL CHECK (expected_length BETWEEN 1 AND 1048576),
    expected_sha256 TEXT NOT NULL
        CHECK (length(expected_sha256) = 64 AND expected_sha256 NOT GLOB '*[^0-9a-f]*'),
    expires_at INTEGER NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('issued', 'uploaded', 'consumed', 'expired')),
    object_key TEXT UNIQUE,
    uploaded_length INTEGER,
    uploaded_sha256 TEXT,
    slot_held INTEGER NOT NULL DEFAULT 1 CHECK (slot_held IN (0, 1)),
    created_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER)),
    CHECK (
        (state = 'issued' AND object_key IS NULL AND uploaded_length IS NULL AND uploaded_sha256 IS NULL)
        OR
        (state IN ('uploaded', 'consumed') AND object_key IS NOT NULL AND uploaded_length IS NOT NULL AND uploaded_sha256 IS NOT NULL)
        OR
        (state = 'expired' AND ((object_key IS NULL AND uploaded_length IS NULL AND uploaded_sha256 IS NULL) OR (object_key IS NOT NULL AND uploaded_length IS NOT NULL AND uploaded_sha256 IS NOT NULL)))
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS v2_document_stages_one_active_per_funnel
    ON v2_document_stages(funnel_id)
    WHERE state IN ('issued', 'uploaded');

CREATE INDEX IF NOT EXISTS v2_document_stages_slots_idx
    ON v2_document_stages(slot_held, state);

CREATE TABLE IF NOT EXISTS v2_document_stage_attempts (
    attempt_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(attempt_id) = 64 AND attempt_id NOT GLOB '*[^0-9a-f]*'),
    stage_id_hash TEXT NOT NULL REFERENCES v2_document_stages(stage_id_hash) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK (state IN ('receiving', 'complete', 'abandoned')),
    sequence TEXT CHECK (sequence IS NULL OR (length(sequence) = 20 AND sequence NOT GLOB '*[^0-9]*')),
    object_key TEXT UNIQUE,
    expected_length INTEGER NOT NULL CHECK (expected_length BETWEEN 1 AND 1048576),
    expected_sha256 TEXT NOT NULL
        CHECK (length(expected_sha256) = 64 AND expected_sha256 NOT GLOB '*[^0-9a-f]*'),
    deadline_at INTEGER NOT NULL,
    tombstone_expires_at INTEGER,
    quota_held INTEGER NOT NULL DEFAULT 1 CHECK (quota_held IN (0, 1)),
    created_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER)),
    CHECK (
        (state = 'receiving' AND tombstone_expires_at IS NULL AND ((sequence IS NULL AND object_key IS NULL) OR (sequence IS NOT NULL AND object_key IS NOT NULL)))
        OR
        (state = 'complete' AND sequence IS NOT NULL AND object_key IS NOT NULL AND tombstone_expires_at IS NULL)
        OR
        (state = 'abandoned' AND tombstone_expires_at IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS v2_document_stage_attempts_cleanup_idx
    ON v2_document_stage_attempts(quota_held, state, deadline_at);

CREATE TABLE IF NOT EXISTS v2_document_stage_sweep_state (
    singleton_id INTEGER PRIMARY KEY NOT NULL CHECK (singleton_id = 1),
    high_water_sequence TEXT,
    cursor TEXT,
    updated_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER))
);

INSERT OR IGNORE INTO v2_document_stage_sweep_state (singleton_id) VALUES (1);
