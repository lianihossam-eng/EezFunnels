#!/usr/bin/env python3

import pathlib
import sqlite3
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATION_NAME = "0017_public_submission_erasure.sql"
FUNNEL_ID = "11111111-1111-4111-8111-111111111111"
SUBMISSION_ID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
DIGEST = "a" * 64


def expect_rejected(connection, statement, parameters, label):
    try:
        connection.execute(statement, parameters)
    except sqlite3.IntegrityError:
        return
    raise AssertionError(f"invalid erasure data was accepted: {label}")


def main() -> int:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        if migration.name < MIGRATION_NAME:
            connection.executescript(migration.read_text(encoding="utf-8"))

    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES (?, ?, ?)",
        (FUNNEL_ID, "PUB-007 erasure fixture", "pub007-erasure"),
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
    connection.execute(
        "INSERT INTO public_form_submissions ("
        "submission_id, funnel_id, revision_id, page_id, form_id, submission_key, "
        "request_fingerprint, field_values_json, request_consent_acknowledged, "
        "request_consent_copy_version, request_consent_copy, request_consent_recorded_at, "
        "submitted_at, expires_at) VALUES (?, ?, 'revision-current', 'page-home', "
        "'form-contact', 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', ?, '{\"email\":\"a@example.test\"}', "
        "1, 'request-v1', 'I agree to be contacted.', 100, 100, 200)",
        (SUBMISSION_ID, FUNNEL_ID, "b" * 64),
    )
    connection.executescript(
        (ROOT / "migrations" / MIGRATION_NAME).read_text(encoding="utf-8")
    )

    intent_columns = {
        row[1]
        for row in connection.execute("PRAGMA table_info(public_submission_erasure_intents)")
    }
    assert intent_columns == {
        "submission_id",
        "replay_digest",
        "source_expires_at",
        "deleted_at",
        "retain_until",
        "last_attempt_at",
    }, intent_columns
    marker_columns = {
        row[1]
        for row in connection.execute("PRAGMA table_info(public_submission_erasure_markers)")
    }
    assert marker_columns == {"replay_digest", "expires_at"}, marker_columns
    assert "submission_key" not in marker_columns and "request_fingerprint" not in marker_columns

    connection.execute(
        "INSERT INTO public_submission_erasure_intents "
        "(submission_id, replay_digest, source_expires_at, deleted_at, retain_until) "
        "VALUES (?, ?, 200, 110, 2592110)",
        (SUBMISSION_ID, DIGEST),
    )
    connection.execute(
        "INSERT INTO public_submission_erasure_markers (replay_digest, expires_at) "
        "VALUES (?, 200)",
        (DIGEST,),
    )
    upsert_marker = (
        "INSERT INTO public_submission_erasure_markers (replay_digest, expires_at) "
        "SELECT ?, ? WHERE ? > unixepoch() "
        "ON CONFLICT(replay_digest) DO UPDATE SET expires_at = "
        "MAX(public_submission_erasure_markers.expires_at, excluded.expires_at)"
    )
    marker_now = connection.execute("SELECT unixepoch()").fetchone()[0]
    reused_digest = "d" * 64
    connection.execute(upsert_marker, (reused_digest, marker_now + 300, marker_now + 300))
    connection.execute(upsert_marker, (reused_digest, marker_now + 200, marker_now + 200))
    reused_expiry = connection.execute(
        "SELECT expires_at FROM public_submission_erasure_markers WHERE replay_digest = ?",
        (reused_digest,),
    ).fetchone()[0]
    assert reused_expiry == marker_now + 300, reused_expiry
    hidden = connection.execute(
        "SELECT COUNT(*) FROM public_form_submissions AS source "
        "WHERE NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents AS intent "
        "WHERE intent.submission_id = source.submission_id)"
    ).fetchone()[0]
    assert hidden == 0, hidden

    expect_rejected(
        connection,
        "INSERT INTO public_submission_erasure_markers (replay_digest, expires_at) VALUES (?, 200)",
        ("A" * 64,),
        "uppercase digest",
    )
    expect_rejected(
        connection,
        "INSERT INTO public_submission_erasure_intents "
        "(submission_id, replay_digest, source_expires_at, deleted_at, retain_until) "
        "VALUES ('cccccccc-cccc-4ccc-8ccc-cccccccccccc', ?, 200, 210, 205)",
        ("c" * 64,),
        "retention deadline before deletion",
    )

    connection.execute("DELETE FROM funnels WHERE id = ?", (FUNNEL_ID,))
    assert connection.execute("SELECT COUNT(*) FROM public_form_submissions").fetchone()[0] == 0
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_erasure_intents WHERE submission_id = ?",
        (SUBMISSION_ID,),
    ).fetchone()[0] == 1
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_erasure_markers WHERE replay_digest = ?",
        (DIGEST,),
    ).fetchone()[0] == 1

    late_id = "dddddddd-dddd-4ddd-8ddd-dddddddddddd"
    late_digest = "e" * 64
    connection.execute(
        "INSERT INTO public_submission_erasure_intents "
        "(submission_id, replay_digest, source_expires_at, deleted_at, retain_until) "
        "VALUES (?, ?, 300, 210, 2592210)",
        (late_id, late_digest),
    )
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_erasure_intents WHERE submission_id = ?",
        (late_id,),
    ).fetchone()[0] == 1

    connection.execute(
        "DELETE FROM public_submission_erasure_markers WHERE expires_at <= 200"
    )
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_erasure_markers"
    ).fetchone()[0] == 1
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_erasure_intents"
    ).fetchone()[0] == 2

    connection.close()
    print(
        "PUB-007 migration: identity-only marker shape, expiry bounds, hidden intent, "
        "cascade-independent and late erasure intents, and monotonic marker expiry passed"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
