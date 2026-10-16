#!/usr/bin/env python3

import json
import pathlib
import sqlite3
import sys
import time


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATION = ROOT / "migrations" / "0011_public_form_context_retention.sql"
FIXTURE = ROOT / "crates/domain/tests/fixtures/document_v2_minimal.json"


def apply_before_context(connection: sqlite3.Connection) -> None:
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        if migration == MIGRATION:
            break
        connection.executescript(migration.read_text(encoding="utf-8"))
    connection.commit()


def document_with_form() -> str:
    document = json.loads(FIXTURE.read_text(encoding="utf-8"))
    document["funnel_id"] = "context-funnel"
    document["pages"][0]["sections"][0]["blocks"] = [
        {"id": "block-form", "kind": "form", "form_id": "form-lead"}
    ]
    document["forms"] = [
        {
            "id": "form-lead",
            "fields": [
                {"id": "field-name", "kind": "text", "name": "name", "label": "Name", "required": True},
                {"id": "field-email", "kind": "email", "name": "email", "label": "E-mail & <work>", "required": True},
                {"id": "field-consent", "kind": "checkbox", "name": "follow_up", "label": "May I follow up?", "required": False},
            ],
            "purpose": "Contact request",
            "request_consent": {"copy_version": "request-v1", "copy": "I agree to be contacted."},
            "result": {"kind": "inline", "message": "Thanks."},
        }
    ]
    return json.dumps(document, ensure_ascii=False, separators=(",", ":"))


def insert_funnel_revision(connection: sqlite3.Connection, data_json: str) -> None:
    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES ('context-funnel', 'Context fixture', 'context-fixture')"
    )
    connection.execute(
        "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) "
        "VALUES ('context-revision', 'context-funnel', 2, ?, '{}')",
        (data_json,),
    )
    connection.execute(
        "UPDATE funnels SET published_revision_id = 'context-revision' WHERE id = 'context-funnel'"
    )


def insert_submission(
    connection: sqlite3.Connection,
    submission_id: str,
    page_id: str,
    form_id: str,
    expires_at: int,
) -> None:
    now = int(time.time())
    connection.execute(
        "INSERT INTO public_form_submissions ("
        "submission_id, funnel_id, revision_id, page_id, form_id, submission_key, "
        "field_values_json, request_consent_acknowledged, request_consent_copy_version, "
        "request_consent_copy, request_consent_recorded_at, marketing_opt_in, submitted_at, expires_at"
        ") VALUES (?, 'context-funnel', 'context-revision', ?, ?, ?, '{\"name\":\"Élodie\"}', "
        "1, 'request-v1', 'I agree to be contacted.', ?, 0, ?, ?)",
        (submission_id, page_id, form_id, submission_id, now - 1, now - 1, expires_at),
    )


def open_database() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    apply_before_context(connection)
    return connection


def apply_context_migration(connection: sqlite3.Connection) -> None:
    migration = MIGRATION.read_text(encoding="utf-8")
    try:
        connection.executescript(f"BEGIN;\n{migration}\nCOMMIT;")
    except sqlite3.Error:
        if connection.in_transaction:
            connection.execute("ROLLBACK")
        raise


def expect_integrity_error(connection: sqlite3.Connection, message: str, operation) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        assert message in str(error), error
    else:
        raise AssertionError(f"expected {message!r} integrity error")


def test_backfill_retention_guard_and_funnel_cascade() -> None:
    connection = open_database()
    insert_funnel_revision(connection, document_with_form())
    now = int(time.time())
    insert_submission(
        connection,
        "00000000-0000-0000-0000-000000000001",
        "page-home",
        "form-lead",
        now + 86_400,
    )
    insert_submission(
        connection,
        "00000000-0000-0000-0000-000000000002",
        "page-home",
        "form-lead",
        now + 172_800,
    )
    connection.commit()

    apply_context_migration(connection)

    context_row = connection.execute(
        "SELECT context_json FROM public_form_contexts "
        "WHERE funnel_id = 'context-funnel' AND revision_id = 'context-revision' "
        "AND page_id = 'page-home' AND form_id = 'form-lead'"
    ).fetchone()
    assert context_row is not None
    context = json.loads(context_row[0])
    expected_context = {
        "page_title": "Home",
        "form_purpose": "Contact request",
        "fields": [
            {"name": "name", "label": "Name", "kind": "text"},
            {"name": "email", "label": "E-mail & <work>", "kind": "email"},
            {"name": "follow_up", "label": "May I follow up?", "kind": "checkbox"},
        ],
    }
    assert context == expected_context, context
    context_text = context_row[0]
    assert context_text == json.dumps(
        expected_context, ensure_ascii=False, separators=(",", ":")
    ), context_text
    assert len(context_text.encode("utf-8")) <= 1_048_576
    assert "field-name" not in context_text and "options" not in context_text

    original_rows = connection.execute(
        "SELECT submission_id, field_values_json, request_consent_copy, request_consent_recorded_at, expires_at "
        "FROM public_form_submissions ORDER BY submission_id"
    ).fetchall()
    assert len(original_rows) == 2
    foreign_keys = connection.execute(
        "PRAGMA foreign_key_list(public_form_submissions)"
    ).fetchall()
    assert {row[2] for row in foreign_keys} == {"funnels"}, foreign_keys

    large_context = json.dumps(
        {"page_title": "🙂" * 17_000, "form_purpose": "Contact", "fields": []},
        ensure_ascii=False,
        separators=(",", ":"),
    )
    assert len(large_context.encode("utf-8")) > 65_536
    connection.execute(
        "INSERT INTO public_form_contexts (funnel_id, revision_id, page_id, form_id, context_json) "
        "VALUES ('context-funnel', 'large-context-revision', 'page-large', 'form-large', ?)",
        (large_context,),
    )

    expect_integrity_error(
        connection,
        "immutable",
        lambda: connection.execute(
            "UPDATE public_form_contexts SET context_json = '{}' WHERE revision_id = 'context-revision'"
        ),
    )
    expect_integrity_error(
        connection,
        "matching public form context",
        lambda: insert_submission(
            connection,
            "00000000-0000-0000-0000-000000000003",
            "page-home",
            "form-unmapped",
            now + 86_400,
        ),
    )

    connection.execute("DELETE FROM revisions WHERE id = 'context-revision'")
    assert connection.execute(
        "SELECT COUNT(*) FROM public_form_submissions WHERE revision_id = 'context-revision'"
    ).fetchone() == (2,)
    assert connection.execute(
        "SELECT COUNT(*) FROM public_form_contexts WHERE revision_id = 'context-revision'"
    ).fetchone() == (1,)
    rows_after_revision_delete = connection.execute(
        "SELECT submission_id, field_values_json, request_consent_copy, request_consent_recorded_at, expires_at "
        "FROM public_form_submissions ORDER BY submission_id"
    ).fetchall()
    assert rows_after_revision_delete == original_rows

    connection.execute("DELETE FROM funnels WHERE id = 'context-funnel'")
    assert connection.execute(
        "SELECT COUNT(*) FROM public_form_submissions WHERE funnel_id = 'context-funnel'"
    ).fetchone() == (0,)
    assert connection.execute(
        "SELECT COUNT(*) FROM public_form_contexts WHERE funnel_id = 'context-funnel'"
    ).fetchone() == (0,)
    connection.close()


def test_active_unreconstructable_submission_fails_and_rolls_back() -> None:
    connection = open_database()
    insert_funnel_revision(connection, document_with_form())
    insert_submission(
        connection,
        "00000000-0000-0000-0000-000000000004",
        "page-missing",
        "form-lead",
        int(time.time()) + 86_400,
    )
    connection.commit()

    try:
        apply_context_migration(connection)
    except sqlite3.IntegrityError as error:
        assert "CHECK constraint failed" in str(error), error
    else:
        raise AssertionError("migration accepted an active submission with no reconstructable context")

    objects = connection.execute(
        "SELECT name FROM sqlite_master WHERE name IN "
        "('public_form_contexts', 'public_form_submissions_before_context', 'public_form_context_migration_guard')"
    ).fetchall()
    assert objects == [], objects
    foreign_keys = connection.execute(
        "PRAGMA foreign_key_list(public_form_submissions)"
    ).fetchall()
    assert "revisions" in {row[2] for row in foreign_keys}, foreign_keys
    assert connection.execute(
        "SELECT COUNT(*) FROM public_form_submissions WHERE submission_id = '00000000-0000-0000-0000-000000000004'"
    ).fetchone() == (1,)
    connection.close()


def main() -> int:
    test_backfill_retention_guard_and_funnel_cascade()
    test_active_unreconstructable_submission_fails_and_rolls_back()
    print("Public form context migration: backfill, 1 MiB bound, immutable context, revision retention, funnel cascade and rollback passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
