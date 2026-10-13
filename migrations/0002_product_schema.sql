CREATE TABLE IF NOT EXISTS funnels (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(id)) > 0),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    slug TEXT NOT NULL UNIQUE CHECK (length(trim(slug)) > 0),
    published_revision_id TEXT REFERENCES revisions(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS revisions (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(id)) > 0),
    funnel_id TEXT NOT NULL REFERENCES funnels(id) ON DELETE CASCADE,
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    data_json TEXT NOT NULL CHECK (json_valid(data_json))
        CHECK (length(CAST(data_json AS BLOB)) <= 1048576),
    render_manifest_json TEXT NOT NULL CHECK (json_valid(render_manifest_json)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS revisions_funnel_created_idx
    ON revisions(funnel_id, created_at, id);

CREATE TRIGGER IF NOT EXISTS revisions_are_immutable
BEFORE UPDATE ON revisions
BEGIN
    SELECT RAISE(ABORT, 'revision snapshots are immutable');
END;

CREATE TRIGGER IF NOT EXISTS revisions_retention_limit
BEFORE INSERT ON revisions
WHEN (SELECT COUNT(*) FROM revisions WHERE funnel_id = NEW.funnel_id) >= 100
BEGIN
    SELECT RAISE(ABORT, 'funnel revision retention limit reached');
END;

CREATE TRIGGER IF NOT EXISTS funnel_published_revision_matches_owner
BEFORE UPDATE OF published_revision_id ON funnels
WHEN NEW.published_revision_id IS NOT NULL
    AND NOT EXISTS (
        SELECT 1
        FROM revisions
        WHERE revisions.id = NEW.published_revision_id
          AND revisions.funnel_id = NEW.id
    )
BEGIN
    SELECT RAISE(ABORT, 'published revision must belong to funnel');
END;

CREATE TRIGGER IF NOT EXISTS funnel_inserted_published_revision_matches_owner
BEFORE INSERT ON funnels
WHEN NEW.published_revision_id IS NOT NULL
    AND NOT EXISTS (
        SELECT 1
        FROM revisions
        WHERE revisions.id = NEW.published_revision_id
          AND revisions.funnel_id = NEW.id
    )
BEGIN
    SELECT RAISE(ABORT, 'published revision must belong to funnel');
END;

CREATE TABLE IF NOT EXISTS themes (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(id)) > 0),
    name TEXT NOT NULL UNIQUE CHECK (length(trim(name)) > 0),
    tokens_json TEXT NOT NULL CHECK (json_valid(tokens_json)),
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS themes_single_default_idx
    ON themes(is_default)
    WHERE is_default = 1;

CREATE TABLE IF NOT EXISTS assets (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(id)) > 0),
    r2_key TEXT NOT NULL UNIQUE CHECK (length(trim(r2_key)) > 0),
    mime TEXT NOT NULL CHECK (length(trim(mime)) > 0),
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(id)) > 0),
    action TEXT NOT NULL CHECK (length(trim(action)) > 0),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO themes (id, name, tokens_json, is_default) VALUES
    ('minimal', 'Minimal', '{"palette":"neutral","radius":"small"}', 1),
    ('bold', 'Bold', '{"palette":"high-contrast","radius":"none"}', 0),
    ('elegant', 'Elegant', '{"palette":"warm","radius":"soft"}', 0),
    ('playful', 'Playful', '{"palette":"bright","radius":"round"}', 0),
    ('corporate', 'Corporate', '{"palette":"blue","radius":"small"}', 0);
