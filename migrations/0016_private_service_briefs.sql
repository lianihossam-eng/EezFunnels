CREATE TABLE IF NOT EXISTS funnel_service_briefs (
    funnel_id TEXT PRIMARY KEY NOT NULL REFERENCES funnels(id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 2147483647),
    format_version INTEGER NOT NULL CHECK (format_version = 1),
    brief_json TEXT NOT NULL CHECK (json_valid(brief_json))
        CHECK (length(CAST(brief_json AS BLOB)) <= 36864),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
