ALTER TABLE assets ADD COLUMN sha256 TEXT;
ALTER TABLE assets ADD COLUMN status TEXT NOT NULL DEFAULT 'ready'
    CHECK (status IN ('pending', 'ready'));

CREATE INDEX IF NOT EXISTS assets_status_created_idx
    ON assets(status, created_at DESC, id);

CREATE TRIGGER IF NOT EXISTS assets_workspace_quota_before_insert
BEFORE INSERT ON assets
WHEN (SELECT COUNT(*) FROM assets) >= 1000
  OR (SELECT COALESCE(SUM(size_bytes), 0) FROM assets) + NEW.size_bytes > 536870912
BEGIN
    SELECT RAISE(ABORT, 'media asset quota reached');
END;
