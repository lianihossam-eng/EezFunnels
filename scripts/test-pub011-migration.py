#!/usr/bin/env python3

import pathlib
import sqlite3


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATION = "0021_public_submission_follow_up.sql"
FUNNEL_ID = "11111111-1111-4111-8111-111111111111"
SUBMISSION_ID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
OTHER_SUBMISSION_ID = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"


def apply_before(connection: sqlite3.Connection, migration_name: str) -> None:
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        if migration.name < migration_name:
            connection.executescript(migration.read_text(encoding="utf-8"))


def expect_rejected(connection: sqlite3.Connection, statement: str, parameters=()) -> None:
    try:
        connection.execute(statement, parameters)
    except sqlite3.IntegrityError:
        return
    raise AssertionError(f"invalid follow-up data was accepted: {statement}")


def main() -> None:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    apply_before(connection, MIGRATION)
    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES (?, 'PUB-011 fixture', 'pub011')",
        (FUNNEL_ID,),
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
        "submission_id, funnel_id, revision_id, page_id, form_id, submission_channel, "
        "submission_key, request_fingerprint, field_values_json, request_consent_acknowledged, "
        "request_consent_copy_version, request_consent_copy, request_consent_recorded_at, "
        "submitted_at, expires_at, triage_status"
        ") VALUES (?, ?, 'revision-current', 'page-home', 'form-contact', 'unattributed', "
        "'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', ?, '{\"email\":\"private@example.test\"}', "
        "1, 'request-v1', 'I agree to be contacted.', 100, 100, 4102444800, 'contacted')",
        (SUBMISSION_ID, FUNNEL_ID, "a" * 64),
    )
    connection.execute(
        "INSERT INTO public_form_submissions ("
        "submission_id, funnel_id, revision_id, page_id, form_id, submission_channel, "
        "submission_key, request_fingerprint, field_values_json, request_consent_acknowledged, "
        "request_consent_copy_version, request_consent_copy, request_consent_recorded_at, "
        "submitted_at, expires_at"
        ") VALUES (?, ?, 'revision-current', 'page-home', 'form-contact', 'unattributed', "
        "'cccccccc-cccc-4ccc-8ccc-cccccccccccc', ?, '{\"email\":\"other@example.test\"}', "
        "1, 'request-v1', 'I agree to be contacted.', 100, 100, 4102444800)",
        (OTHER_SUBMISSION_ID, FUNNEL_ID, "b" * 64),
    )

    connection.executescript((ROOT / "migrations" / MIGRATION).read_text(encoding="utf-8"))
    legacy_state = connection.execute(
        "SELECT triage_status, follow_up_version, next_action, due_on, close_reason "
        "FROM public_form_submissions WHERE submission_id = ?",
        (SUBMISSION_ID,),
    ).fetchone()
    assert legacy_state == ("contacted", 0, None, None, None), legacy_state
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_follow_up_events WHERE submission_id = ?",
        (SUBMISSION_ID,),
    ).fetchone()[0] == 0

    connection.execute(
        "INSERT INTO public_submission_follow_up_events ("
        "submission_id, version, status, next_action, due_on, close_reason, recorded_at, "
        "provenance, supersedes_version"
        ") VALUES (?, 1, 'qualified', 'Send the proposal', '2028-02-29', NULL, 120, "
        "'owner_follow_up', NULL)",
        (SUBMISSION_ID,),
    )
    connection.execute(
        "UPDATE public_form_submissions SET triage_status = 'qualified', follow_up_version = 1, "
        "next_action = 'Send the proposal', due_on = '2028-02-29' WHERE submission_id = ?",
        (SUBMISSION_ID,),
    )
    connection.execute(
        "INSERT INTO public_submission_follow_up_events ("
        "submission_id, version, status, next_action, due_on, close_reason, recorded_at, "
        "provenance, supersedes_version, corrects_version"
        ") VALUES (?, 2, 'closed', NULL, NULL, 'Not a fit', 130, 'owner_follow_up', 1, 1)",
        (SUBMISSION_ID,),
    )
    connection.execute(
        "UPDATE public_form_submissions SET triage_status = 'closed', follow_up_version = 2, "
        "next_action = NULL, due_on = NULL, close_reason = 'Not a fit' "
        "WHERE submission_id = ?",
        (SUBMISSION_ID,),
    )
    connection.execute(
        "INSERT INTO public_submission_follow_up_receipts ("
        "submission_id, idempotency_key, request_digest, result_version, expires_at"
        ") VALUES (?, 'pub011-key-1', ?, 2, 4102444800)",
        (SUBMISSION_ID, "c" * 64),
    )
    expect_rejected(
        connection,
        "INSERT INTO public_submission_follow_up_receipts ("
        "submission_id, idempotency_key, request_digest, result_version, expires_at"
        ") VALUES (?, 'pub011-key-1', ?, 0, 4102444800)",
        (OTHER_SUBMISSION_ID, "e" * 64),
    )
    assert connection.execute(
        "SELECT supersedes_version FROM public_submission_follow_up_events "
        "WHERE submission_id = ? AND version = 2",
        (SUBMISSION_ID,),
    ).fetchone()[0] == 1

    expect_rejected(
        connection,
        "UPDATE public_submission_follow_up_events SET close_reason = 'edited' "
        "WHERE submission_id = ? AND version = 2",
        (SUBMISSION_ID,),
    )
    expect_rejected(
        connection,
        "DELETE FROM public_submission_follow_up_events WHERE submission_id = ? AND version = 1",
        (SUBMISSION_ID,),
    )
    expect_rejected(
        connection,
        "INSERT INTO public_submission_follow_up_events ("
        "submission_id, version, status, recorded_at, provenance, supersedes_version"
        ") VALUES (?, 3, 'qualified', 140, 'owner_follow_up', 1)",
        (SUBMISSION_ID,),
    )
    expect_rejected(
        connection,
        "UPDATE public_form_submissions SET close_reason = 'Not a fit', triage_status = 'new' "
        "WHERE submission_id = ?",
        (SUBMISSION_ID,),
    )
    expect_rejected(
        connection,
        "INSERT INTO public_submission_follow_up_receipts ("
        "submission_id, idempotency_key, request_digest, result_version, expires_at"
        ") VALUES (?, 'pub011-key-too-long', ?, 2, 4102444801)",
        (SUBMISSION_ID, "d" * 64),
    )
    expect_rejected(
        connection,
        "INSERT INTO public_submission_follow_up_receipts ("
        "submission_id, idempotency_key, request_digest, result_version, expires_at"
        ") VALUES (?, 'pub011-key-2', ?, 2, 100000)",
        (SUBMISSION_ID, "not-a-sha256-digest"),
    )

    connection.execute(
        "INSERT INTO public_submission_follow_up_events ("
        "submission_id, version, status, close_reason, recorded_at, provenance, supersedes_version, corrects_version"
        ") VALUES (?, 3, 'closed', 'Not a fit', 140, 'owner_follow_up', 2, NULL)",
        (SUBMISSION_ID,),
    )
    connection.execute(
        "UPDATE public_form_submissions SET follow_up_version = 3 WHERE submission_id = ?",
        (SUBMISSION_ID,),
    )
    expect_rejected(
        connection,
        "INSERT INTO public_submission_follow_up_events ("
        "submission_id, version, status, recorded_at, provenance, supersedes_version, corrects_version"
        ") VALUES (?, 4, 'closed', 150, 'owner_follow_up', 3, 1)",
        (SUBMISSION_ID,),
    )
    expect_rejected(
        connection,
        "INSERT INTO public_submission_follow_up_events ("
        "submission_id, version, status, recorded_at, provenance, supersedes_version, corrects_version"
        ") VALUES (?, 4, 'closed', 150, 'owner_follow_up', 3, 2)",
        (SUBMISSION_ID,),
    )

    long_history_id = OTHER_SUBMISSION_ID
    for version in range(1, 1_105):
        connection.execute(
            "INSERT INTO public_submission_follow_up_events ("
            "submission_id, version, status, recorded_at, provenance, supersedes_version, corrects_version"
            ") VALUES (?, ?, 'new', ?, 'owner_follow_up', ?, ?)",
            (
                long_history_id,
                version,
                200 + version,
                version - 1 if version > 1 else None,
                1 if version == 2 else None,
            ),
        )
        connection.execute(
            "UPDATE public_form_submissions SET follow_up_version = ?, triage_status = 'new' "
            "WHERE submission_id = ?",
            (version, long_history_id),
        )
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_follow_up_events WHERE submission_id = ?",
        (long_history_id,),
    ).fetchone()[0] == 1_104

    connection.execute("DELETE FROM public_form_submissions WHERE submission_id = ?", (SUBMISSION_ID,))
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_follow_up_events WHERE submission_id = ?",
        (SUBMISSION_ID,),
    ).fetchone()[0] == 0
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_follow_up_receipts WHERE submission_id = ?",
        (SUBMISSION_ID,),
    ).fetchone()[0] == 0
    connection.execute(
        "DELETE FROM public_form_submissions WHERE submission_id = ?",
        (long_history_id,),
    )
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_follow_up_events WHERE submission_id = ?",
        (long_history_id,),
    ).fetchone()[0] == 0
    connection.close()
    print(
        "PUB-011 migration: legacy status stays undated, events are immutable and linked, "
        "corrections target ordinary events and deletion handles long linked histories"
    )


if __name__ == "__main__":
    main()
