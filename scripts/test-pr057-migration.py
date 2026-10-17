#!/usr/bin/env python3

import pathlib
import sqlite3
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATION_NAME = "0014_public_form_submission_funnel_inbox.sql"


def main() -> int:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        if migration.name != MIGRATION_NAME:
            connection.executescript(migration.read_text(encoding="utf-8"))

    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES (?, ?, ?)",
        ("11111111-1111-4111-8111-111111111111", "Fixture", "fixture"),
    )
    connection.execute(
        "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) "
        "VALUES (?, ?, 2, '{}', '{}')",
        ("revision-a", "11111111-1111-4111-8111-111111111111"),
    )
    connection.execute(
        "UPDATE funnels SET published_revision_id = 'revision-a' "
        "WHERE id = '11111111-1111-4111-8111-111111111111'"
    )

    migration = ROOT / "migrations" / MIGRATION_NAME
    connection.executescript(migration.read_text(encoding="utf-8"))
    index_columns = connection.execute(
        "PRAGMA index_info(public_form_submissions_funnel_inbox_idx)"
    ).fetchall()
    assert [row[2] for row in index_columns] == [
        "funnel_id",
        "submitted_at",
        "submission_id",
    ], index_columns
    combined_index_columns = connection.execute(
        "PRAGMA index_info(public_form_submissions_funnel_status_inbox_idx)"
    ).fetchall()
    assert [row[2] for row in combined_index_columns] == [
        "funnel_id",
        "triage_status",
        "submitted_at",
        "submission_id",
    ], combined_index_columns

    funnel_query_plan = connection.execute(
        "EXPLAIN QUERY PLAN SELECT submission_id FROM public_form_submissions "
        "WHERE expires_at > unixepoch() AND funnel_id = ? "
        "AND (? IS NULL OR submitted_at < ? OR (submitted_at = ? AND submission_id < ?)) "
        "ORDER BY submitted_at DESC, submission_id DESC LIMIT ?",
        (
            "11111111-1111-4111-8111-111111111111",
            None,
            None,
            None,
            None,
            26,
        ),
    ).fetchall()
    assert any(
        "public_form_submissions_funnel_inbox_idx" in str(row[3])
        for row in funnel_query_plan
    ), funnel_query_plan
    combined_query_plan = connection.execute(
        "EXPLAIN QUERY PLAN SELECT submission_id FROM public_form_submissions "
        "WHERE expires_at > unixepoch() AND funnel_id = ? AND triage_status = ? "
        "AND (? IS NULL OR submitted_at < ? OR (submitted_at = ? AND submission_id < ?)) "
        "ORDER BY submitted_at DESC, submission_id DESC LIMIT ?",
        (
            "11111111-1111-4111-8111-111111111111",
            "new",
            None,
            None,
            None,
            None,
            26,
        ),
    ).fetchall()
    assert any(
        "public_form_submissions_funnel_status_inbox_idx" in str(row[3])
        for row in combined_query_plan
    ), combined_query_plan

    connection.close()
    print("PR-057 migration: funnel and combined funnel/status keyset queries use their inbox indexes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
