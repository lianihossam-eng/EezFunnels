#!/usr/bin/env python3

import pathlib
import sqlite3
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATIONS = sorted((ROOT / "migrations").glob("000[1-5]_*.sql"))
EDITOR_MIGRATION = ROOT / "migrations" / "0006_funnel_editor_heads.sql"
DIAGNOSIS = "PR-010 editor-head migration stopped: every existing funnel must have exactly one revision"


def database_with_legacy_rows(revision_count: int) -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in MIGRATIONS:
        connection.executescript(migration.read_text(encoding="utf-8"))
    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES ('fixture', 'Fixture', 'fixture')"
    )
    for index in range(revision_count):
        connection.execute(
            "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) "
            "VALUES (?, 'fixture', 1, '{}', '{}')",
            (f"revision-{index}",),
        )
    connection.commit()
    return connection


def assert_ambiguous_funnels_stop(revision_count: int) -> None:
    connection = database_with_legacy_rows(revision_count)
    try:
        sql = EDITOR_MIGRATION.read_text(encoding="utf-8")
        connection.executescript(f"BEGIN;\n{sql}\nCOMMIT;")
    except sqlite3.IntegrityError as error:
        assert DIAGNOSIS in str(error), error
        connection.rollback()
    else:
        raise AssertionError(f"editor-head migration accepted {revision_count} revisions")
    finally:
        objects = connection.execute(
            "SELECT name FROM sqlite_master "
            "WHERE name LIKE 'funnel_editor_%' OR name = 'revisions_editor_head_is_retained'"
        ).fetchall()
        assert objects == [], f"ambiguous migration left partial objects: {objects}"
        connection.close()


def assert_exactly_one_revision_is_backfilled() -> None:
    connection = database_with_legacy_rows(1)
    sql = EDITOR_MIGRATION.read_text(encoding="utf-8")
    connection.executescript(f"BEGIN;\n{sql}\nCOMMIT;")
    rows = connection.execute(
        "SELECT funnel_id, revision_id FROM funnel_editor_heads"
    ).fetchall()
    assert rows == [("fixture", "revision-0")], rows

    try:
        connection.execute(
            "INSERT INTO funnel_editor_heads (funnel_id, revision_id) "
            "VALUES ('fixture', 'missing-revision')"
        )
    except sqlite3.IntegrityError as error:
        assert "editor head revision must belong to funnel" in str(error), error
    else:
        raise AssertionError("editor-head trigger accepted a revision owned by another funnel")
    connection.close()


def main() -> int:
    assert_ambiguous_funnels_stop(0)
    assert_ambiguous_funnels_stop(2)
    assert_exactly_one_revision_is_backfilled()
    print("PR-010 editor-head migration: zero/multi-revision preflight and valid backfill passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
