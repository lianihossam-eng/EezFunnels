#!/usr/bin/env python3

import pathlib
import sqlite3
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATIONS = sorted((ROOT / "migrations").glob("*.sql"))


def apply(connection: sqlite3.Connection, migration_name: str) -> None:
    migration = ROOT / "migrations" / migration_name
    connection.executescript(migration.read_text(encoding="utf-8"))


def main() -> int:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in MIGRATIONS:
        if migration.name in {
            "0012_public_form_submission_channel.sql",
            "0013_public_form_submission_status.sql",
            "0014_public_form_submission_funnel_inbox.sql",
            "0021_public_submission_follow_up.sql",
        }:
            continue
        connection.executescript(migration.read_text(encoding="utf-8"))

    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES ('fixture', 'Fixture', 'fixture')"
    )
    connection.execute(
        "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) "
        "VALUES ('revision-a', 'fixture', 2, '{}', '{}')"
    )
    connection.execute(
        "UPDATE funnels SET published_revision_id = 'revision-a' WHERE id = 'fixture'"
    )
    connection.execute(
        "INSERT INTO public_form_submissions ("
        "submission_id, funnel_id, revision_id, page_id, form_id, submission_key, "
        "request_fingerprint, field_values_json, request_consent_acknowledged, "
        "request_consent_copy_version, request_consent_copy, request_consent_recorded_at, "
        "submitted_at, expires_at"
        ") VALUES (?, 'fixture', 'revision-a', 'page-a', 'form-a', ?, ?, '{}', 1, 'v1', 'Consent', 1, 100, 200)",
        (
            "11111111-1111-4111-8111-111111111111",
            "22222222-2222-4222-8222-222222222222",
            "0" * 64,
        ),
    )

    apply(connection, "0012_public_form_submission_channel.sql")
    legacy_channel = connection.execute(
        "SELECT submission_channel FROM public_form_submissions"
    ).fetchone()
    assert legacy_channel == ("unattributed",), legacy_channel

    apply(connection, "0013_public_form_submission_status.sql")
    apply(connection, "0014_public_form_submission_funnel_inbox.sql")
    apply(connection, "0021_public_submission_follow_up.sql")
    follow_up_version = connection.execute(
        "SELECT follow_up_version FROM public_form_submissions"
    ).fetchone()
    assert follow_up_version == (0,), follow_up_version
    legacy_status = connection.execute(
        "SELECT triage_status FROM public_form_submissions"
    ).fetchone()
    assert legacy_status == ("new",), legacy_status

    for status in ("contacted", "qualified", "closed"):
        connection.execute(
            "UPDATE public_form_submissions SET triage_status = ?",
            (status,),
        )
        stored_status = connection.execute(
            "SELECT triage_status FROM public_form_submissions"
        ).fetchone()
        assert stored_status == (status,), stored_status

    query_plan = connection.execute(
        "EXPLAIN QUERY PLAN SELECT submission_id FROM public_form_submissions "
        "WHERE expires_at > unixepoch() AND triage_status = ? "
        "AND (? IS NULL OR submitted_at < ? OR (submitted_at = ? AND submission_id < ?)) "
        "ORDER BY submitted_at DESC, submission_id DESC LIMIT ?",
        ("closed", None, None, None, None, 26),
    ).fetchall()
    assert any(
        "public_form_submissions_status_inbox_idx" in str(row[3]) for row in query_plan
    ), query_plan

    try:
        connection.execute(
            "UPDATE public_form_submissions SET triage_status = 'waiting'"
        )
    except sqlite3.IntegrityError as error:
        assert "CHECK constraint failed" in str(error), error
    else:
        raise AssertionError("migration accepted a status outside the closed set")

    connection.close()
    print("PR-053 migration: legacy submissions default to new; status values are constrained")
    return 0


if __name__ == "__main__":
    sys.exit(main())
