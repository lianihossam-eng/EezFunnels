ALTER TABLE assets ADD COLUMN sha256 TEXT;
ALTER TABLE assets ADD COLUMN format TEXT
    CHECK (format IS NULL OR format IN ('jpeg', 'png'));
ALTER TABLE assets ADD COLUMN width INTEGER
    CHECK (width IS NULL OR width BETWEEN 1 AND 2048);
ALTER TABLE assets ADD COLUMN height INTEGER
    CHECK (height IS NULL OR height BETWEEN 1 AND 2048);
ALTER TABLE assets ADD COLUMN display_name TEXT NOT NULL DEFAULT ''
    CHECK (length(CAST(display_name AS BLOB)) <= 120);
ALTER TABLE assets ADD COLUMN state TEXT NOT NULL DEFAULT 'legacy_unverified'
    CHECK (state IN ('legacy_unverified', 'pending', 'ready'));
ALTER TABLE assets ADD COLUMN archived INTEGER NOT NULL DEFAULT 0
    CHECK (archived IN (0, 1));

CREATE INDEX assets_library_page_idx ON assets(state, archived, id DESC);

CREATE TABLE revision_asset_refs (
    revision_id TEXT NOT NULL REFERENCES revisions(id) ON DELETE CASCADE,
    asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    PRIMARY KEY (revision_id, asset_id)
);

CREATE INDEX revision_asset_refs_asset_idx
    ON revision_asset_refs(asset_id, revision_id);

CREATE TRIGGER revisions_validate_media_manifest
BEFORE INSERT ON revisions
WHEN NEW.schema_version = 2
    AND EXISTS (
        SELECT 1
        FROM json_each(COALESCE(json_extract(NEW.render_manifest_json, '$.assets'), '[]')) manifest
        LEFT JOIN assets ON assets.id = json_extract(manifest.value, '$.asset_id')
        WHERE json_type(manifest.value, '$.asset_id') IS NOT 'text'
            OR json_type(manifest.value, '$.sha256') IS NOT 'text'
            OR json_type(manifest.value, '$.format') IS NOT 'text'
            OR json_type(manifest.value, '$.byte_length') IS NOT 'integer'
            OR json_type(manifest.value, '$.width') IS NOT 'integer'
            OR json_type(manifest.value, '$.height') IS NOT 'integer'
            OR assets.id IS NULL
            OR assets.state != 'ready'
            OR assets.sha256 != json_extract(manifest.value, '$.sha256')
            OR assets.format != json_extract(manifest.value, '$.format')
            OR assets.size_bytes != json_extract(manifest.value, '$.byte_length')
            OR assets.width != json_extract(manifest.value, '$.width')
            OR assets.height != json_extract(manifest.value, '$.height')
    )
BEGIN
    SELECT RAISE(ABORT, 'revision media manifest must reference ready matching assets');
END;

CREATE TRIGGER revisions_record_media_refs
AFTER INSERT ON revisions
WHEN NEW.schema_version = 2
BEGIN
    INSERT INTO revision_asset_refs (revision_id, asset_id)
    SELECT NEW.id, json_extract(manifest.value, '$.asset_id')
    FROM json_each(COALESCE(json_extract(NEW.render_manifest_json, '$.assets'), '[]')) manifest;
END;

CREATE TRIGGER ready_assets_are_immutable
BEFORE UPDATE ON assets
WHEN OLD.state = 'ready'
    AND (
        NEW.id IS NOT OLD.id
        OR NEW.r2_key IS NOT OLD.r2_key
        OR NEW.mime IS NOT OLD.mime
        OR NEW.size_bytes IS NOT OLD.size_bytes
        OR NEW.sha256 IS NOT OLD.sha256
        OR NEW.format IS NOT OLD.format
        OR NEW.width IS NOT OLD.width
        OR NEW.height IS NOT OLD.height
        OR NEW.display_name IS NOT OLD.display_name
        OR NEW.state IS NOT OLD.state
        OR NEW.created_at IS NOT OLD.created_at
    )
BEGIN
    SELECT RAISE(ABORT, 'ready media asset descriptors are immutable');
END;

CREATE TRIGGER ready_assets_must_be_complete
BEFORE INSERT ON assets
WHEN NEW.state = 'ready'
    AND (
        NEW.sha256 IS NULL
        OR NEW.sha256 != NEW.id
        OR NEW.format IS NULL
        OR NEW.width IS NULL
        OR NEW.height IS NULL
        OR NEW.size_bytes <= 0
        OR NEW.size_bytes > 4194304
        OR length(CAST(NEW.display_name AS BLOB)) = 0
        OR NEW.r2_key != ('private/media/v1/' || NEW.id)
        OR (NEW.format = 'jpeg' AND NEW.mime != 'image/jpeg')
        OR (NEW.format = 'png' AND NEW.mime != 'image/png')
    )
BEGIN
    SELECT RAISE(ABORT, 'ready media asset descriptor is incomplete');
END;

CREATE TRIGGER ready_assets_must_be_complete_on_transition
BEFORE UPDATE OF state ON assets
WHEN NEW.state = 'ready'
    AND (
        NEW.sha256 IS NULL
        OR NEW.sha256 != NEW.id
        OR NEW.format IS NULL
        OR NEW.width IS NULL
        OR NEW.height IS NULL
        OR NEW.size_bytes <= 0
        OR NEW.size_bytes > 4194304
        OR length(CAST(NEW.display_name AS BLOB)) = 0
        OR NEW.r2_key != ('private/media/v1/' || NEW.id)
        OR (NEW.format = 'jpeg' AND NEW.mime != 'image/jpeg')
        OR (NEW.format = 'png' AND NEW.mime != 'image/png')
    )
BEGIN
    SELECT RAISE(ABORT, 'ready media asset descriptor is incomplete');
END;

CREATE TRIGGER media_assets_are_never_hard_deleted
BEFORE DELETE ON assets
BEGIN
    SELECT RAISE(ABORT, 'media assets are archived instead of deleted');
END;
