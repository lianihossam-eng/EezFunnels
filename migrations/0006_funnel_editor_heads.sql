CREATE TABLE IF NOT EXISTS funnel_editor_migration_guard (
    ok INTEGER NOT NULL CHECK (ok = 1)
);

CREATE TRIGGER IF NOT EXISTS funnel_editor_migration_guard_diagnosis
BEFORE INSERT ON funnel_editor_migration_guard
WHEN EXISTS (
    SELECT funnels.id
    FROM funnels
    LEFT JOIN revisions ON revisions.funnel_id = funnels.id
    GROUP BY funnels.id
    HAVING COUNT(revisions.id) <> 1
)
BEGIN
    SELECT RAISE(
        ABORT,
        'PR-010 editor-head migration stopped: every existing funnel must have exactly one revision; inspect and repair zero/multi-revision funnels before retrying'
    );
END;

INSERT INTO funnel_editor_migration_guard (ok) VALUES (1);
DROP TRIGGER funnel_editor_migration_guard_diagnosis;
DROP TABLE funnel_editor_migration_guard;

CREATE TABLE IF NOT EXISTS funnel_editor_heads (
    funnel_id TEXT PRIMARY KEY NOT NULL REFERENCES funnels(id) ON DELETE CASCADE,
    revision_id TEXT NOT NULL UNIQUE REFERENCES revisions(id) ON DELETE CASCADE
);

INSERT INTO funnel_editor_heads (funnel_id, revision_id)
SELECT funnels.id, revisions.id
FROM funnels
JOIN revisions ON revisions.funnel_id = funnels.id
GROUP BY funnels.id
HAVING COUNT(revisions.id) = 1;

CREATE TRIGGER IF NOT EXISTS funnel_editor_head_insert_matches_owner
BEFORE INSERT ON funnel_editor_heads
WHEN NOT EXISTS (
    SELECT 1 FROM revisions
    WHERE revisions.id = NEW.revision_id
      AND revisions.funnel_id = NEW.funnel_id
)
BEGIN
    SELECT RAISE(ABORT, 'editor head revision must belong to funnel');
END;

CREATE TRIGGER IF NOT EXISTS funnel_editor_head_update_matches_owner
BEFORE UPDATE OF revision_id, funnel_id ON funnel_editor_heads
WHEN NOT EXISTS (
    SELECT 1 FROM revisions
    WHERE revisions.id = NEW.revision_id
      AND revisions.funnel_id = NEW.funnel_id
)
BEGIN
    SELECT RAISE(ABORT, 'editor head revision must belong to funnel');
END;

CREATE TRIGGER IF NOT EXISTS revisions_editor_head_is_retained
BEFORE DELETE ON revisions
WHEN EXISTS (
    SELECT 1 FROM funnel_editor_heads WHERE revision_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'editor head revision cannot be deleted');
END;
