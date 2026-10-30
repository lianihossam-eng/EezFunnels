#!/usr/bin/env python3

import pathlib
import json
import sqlite3
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATION_NAME = "0016_private_service_briefs.sql"
FUNNEL_ID = "11111111-1111-4111-8111-111111111111"
UPSERT = (
    "INSERT INTO funnel_service_briefs "
    "(funnel_id, version, format_version, brief_json, updated_at) "
    "SELECT ?, ?, ?, ?, CURRENT_TIMESTAMP "
    "WHERE EXISTS (SELECT 1 FROM funnels WHERE id = ?) "
    "AND ((? = 0 AND NOT EXISTS (SELECT 1 FROM funnel_service_briefs WHERE funnel_id = ?)) "
    "OR EXISTS (SELECT 1 FROM funnel_service_briefs WHERE funnel_id = ? AND version = ?)) "
    "ON CONFLICT(funnel_id) DO UPDATE SET version = excluded.version, "
    "format_version = excluded.format_version, brief_json = excluded.brief_json, "
    "updated_at = CURRENT_TIMESTAMP "
    "WHERE funnel_service_briefs.version = ? "
    "RETURNING funnel_id, version, format_version, brief_json, updated_at"
)


def fresh_database() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        if migration.name != MIGRATION_NAME:
            connection.executescript(migration.read_text(encoding="utf-8"))
    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES (?, ?, ?)",
        (FUNNEL_ID, "Fixture", "fixture"),
    )
    connection.execute(
        "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) "
        "VALUES ('revision-current', ?, 2, '{}', '{}')",
        (FUNNEL_ID,),
    )
    connection.execute(
        "UPDATE funnels SET published_revision_id = 'revision-current' WHERE id = ?",
        (FUNNEL_ID,),
    )
    connection.executescript(
        (ROOT / "migrations" / MIGRATION_NAME).read_text(encoding="utf-8")
    )
    return connection


def cas_save(connection, expected: int, serialized: str):
    current = connection.execute(
        "SELECT version, brief_json FROM funnel_service_briefs WHERE funnel_id = ?",
        (FUNNEL_ID,),
    ).fetchone()
    current_version = 0 if current is None else current[0]
    if current_version != expected:
        return current if current is not None and current[1] == serialized else None
    if current is not None and current[1] == serialized:
        return current
    next_version = expected + 1
    return connection.execute(
        UPSERT,
        (
            FUNNEL_ID,
            next_version,
            1,
            serialized,
            FUNNEL_ID,
            expected,
            FUNNEL_ID,
            FUNNEL_ID,
            expected,
            expected,
        ),
    ).fetchone()


def assert_rejected(connection, statement: str, parameters: tuple, message: str) -> None:
    try:
        connection.execute(statement, parameters)
    except sqlite3.IntegrityError:
        return
    raise AssertionError(message)


def main() -> int:
    connection = fresh_database()
    assert connection.execute("SELECT COUNT(*) FROM funnel_service_briefs").fetchone()[0] == 0
    columns = {
        row[1] for row in connection.execute("PRAGMA table_info(funnel_service_briefs)")
    }
    assert columns == {
        "funnel_id",
        "version",
        "format_version",
        "brief_json",
        "updated_at",
    }, columns

    first = cas_save(connection, 0, '{"offer":"Service"}')
    assert first is not None and first[1:4] == (1, 1, '{"offer":"Service"}'), first
    second = cas_save(connection, 1, '{"offer":"Updated service"}')
    assert second is not None and second[1:4] == (2, 1, '{"offer":"Updated service"}'), second
    stale_different = cas_save(connection, 1, '{"offer":"Outdated service"}')
    assert stale_different is None, stale_different
    retry_same = cas_save(connection, 1, '{"offer":"Updated service"}')
    assert retry_same is not None and retry_same[0] == 2, retry_same
    stale_insert = cas_save(connection, 0, '{"offer":"First stale draft"}')
    assert stale_insert is None, stale_insert

    escaped_quotes = '"' * 4_096
    escaped_brief = json.dumps(
        {
            "intention": None,
            "audience": escaped_quotes,
            "need": escaped_quotes,
            "offer": escaped_quotes,
            "scope": escaped_quotes,
            "terms": None,
            "action": None,
            "promised_next_step": None,
            "available_evidence": None,
        },
        separators=(",", ":"),
    )
    assert len(escaped_brief.encode("utf-8")) > 20_480
    assert len(escaped_brief.encode("utf-8")) <= 36_864
    assert len(escaped_brief.encode("utf-8")) > 20_480
    assert cas_save(connection, 2, escaped_brief) is not None
    assert connection.execute(
        "SELECT version FROM funnel_service_briefs WHERE funnel_id = ?",
        (FUNNEL_ID,),
    ).fetchone() == (3,)

    connection.execute("DELETE FROM funnel_service_briefs WHERE funnel_id = ?", (FUNNEL_ID,))

    assert_rejected(
        connection,
        "INSERT INTO funnel_service_briefs "
        "(funnel_id, version, format_version, brief_json) VALUES (?, 1, 1, '{}')",
        ("other-funnel",),
        "brief row for a missing funnel was accepted",
    )
    for version in (0, 2_147_483_648):
        assert_rejected(
            connection,
            "INSERT INTO funnel_service_briefs "
            "(funnel_id, version, format_version, brief_json) VALUES (?, ?, 1, '{}')",
            (FUNNEL_ID, version),
            f"invalid brief version was accepted: {version}",
        )
    assert_rejected(
        connection,
        "INSERT INTO funnel_service_briefs "
        "(funnel_id, version, format_version, brief_json) VALUES (?, 1, 2, '{}')",
        (FUNNEL_ID,),
        "unsupported brief format version was accepted",
    )
    assert_rejected(
        connection,
        "INSERT INTO funnel_service_briefs "
        "(funnel_id, version, format_version, brief_json) VALUES (?, 1, 1, 'not-json')",
        (FUNNEL_ID,),
        "invalid brief JSON was accepted",
    )
    assert_rejected(
        connection,
        "INSERT INTO funnel_service_briefs "
        "(funnel_id, version, format_version, brief_json) VALUES (?, 1, 1, ?)",
        (FUNNEL_ID, '{"x":"' + "x" * 36_860 + '"}'),
        "oversized brief JSON was accepted",
    )

    connection.execute(
        "INSERT INTO funnel_service_briefs "
        "(funnel_id, version, format_version, brief_json) VALUES (?, 1, 1, '{}')",
        (FUNNEL_ID,),
    )
    connection.execute("DELETE FROM funnels WHERE id = ?", (FUNNEL_ID,))
    assert connection.execute("SELECT COUNT(*) FROM funnel_service_briefs").fetchone()[0] == 0
    connection.close()
    print(
        "PUB-005 migration: no backfill, format and size constraints, funnel cascade, "
        "version-zero insert, positive-version update, stale-write rejection, and retry passed"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
