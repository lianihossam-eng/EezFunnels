#!/usr/bin/env python3

import pathlib
import sqlite3
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATION_NAME = "0012_public_form_submission_channel.sql"
MIGRATIONS = sorted((ROOT / "migrations").glob("*.sql"))


def main() -> int:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in MIGRATIONS:
        if migration.name < MIGRATION_NAME:
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

    migration = ROOT / "migrations" / MIGRATION_NAME
    connection.executescript(migration.read_text(encoding="utf-8"))
    legacy_channel = connection.execute(
        "SELECT submission_channel FROM public_form_submissions"
    ).fetchone()
    assert legacy_channel == ("unattributed",), legacy_channel

    try:
        connection.execute(
            "UPDATE public_form_submissions SET submission_channel = 'raw-campaign'"
        )
    except sqlite3.IntegrityError as error:
        assert "CHECK constraint failed" in str(error), error
    else:
        raise AssertionError("migration accepted a value outside the closed channel set")

    connection.close()
    print("PR-052 migration: legacy submissions default to unattributed; channel set is constrained")
    return 0


if __name__ == "__main__":
    sys.exit(main())
