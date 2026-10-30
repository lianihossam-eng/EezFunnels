#!/usr/bin/env python3

import pathlib
import sqlite3
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATION_NAME = "0018_public_submission_delivery_outbox.sql"
FUNNEL_ID = "11111111-1111-4111-8111-111111111111"
SUBMISSION_ID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
SUBMISSION_KEY = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
DESTINATION_A = "cccccccc-cccc-4ccc-8ccc-cccccccccccc"
DESTINATION_B = "dddddddd-dddd-4ddd-8ddd-dddddddddddd"
MAPPING_A = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"
MAPPING_B = "ffffffff-ffff-4fff-8fff-ffffffffffff"


def apply_before(connection: sqlite3.Connection, migration_name: str) -> None:
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        if migration.name < migration_name:
            connection.executescript(migration.read_text(encoding="utf-8"))


def insert_submission(connection: sqlite3.Connection, submission_id: str, key: str) -> None:
    connection.execute(
        "INSERT INTO public_form_submissions ("
        "submission_id, funnel_id, revision_id, page_id, form_id, submission_channel, "
        "submission_key, request_fingerprint, field_values_json, "
        "request_consent_acknowledged, request_consent_copy_version, request_consent_copy, "
        "request_consent_recorded_at, submitted_at, expires_at"
        ") VALUES (?, ?, 'revision-current', 'page-home', 'form-contact', 'unattributed', "
        "?, ?, '{\"email\":\"private@example.test\"}', 1, 'request-v1', "
        "'I agree to be contacted.', 100, 100, 200)",
        (submission_id, FUNNEL_ID, key, "a" * 64),
    )


def expect_rejected(connection: sqlite3.Connection, statement: str, parameters: tuple, label: str):
    try:
        connection.execute(statement, parameters)
    except sqlite3.IntegrityError:
        return
    raise AssertionError(f"invalid delivery data was accepted: {label}")


def seed_funnel(connection: sqlite3.Connection) -> None:
    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES (?, ?, ?)",
        (FUNNEL_ID, "PUB-008 outbox fixture", "pub008-outbox"),
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


def seed_configuration(connection: sqlite3.Connection) -> None:
    connection.executemany(
        "INSERT INTO webhook_destination_versions "
        "(destination_version_id, destination_url, created_at) VALUES (?, ?, 100)",
        [
            (DESTINATION_A, "https://receiver.invalid/a"),
            (DESTINATION_B, "https://receiver.invalid/b"),
        ],
    )
    connection.executemany(
        "INSERT INTO public_form_delivery_mapping_versions "
        "(mapping_version_id, funnel_id, form_id, destination_version_id, "
        "selected_fields_json, created_at) VALUES (?, ?, 'form-contact', ?, '[\"field-email\"]', 100)",
        [(MAPPING_A, FUNNEL_ID, DESTINATION_A), (MAPPING_B, FUNNEL_ID, DESTINATION_B)],
    )
    connection.execute(
        "UPDATE webhook_installation_delivery SET enabled = 1, destination_version_id = ? "
        "WHERE installation_id = 1",
        (DESTINATION_A,),
    )
    connection.execute(
        "INSERT INTO public_form_delivery_settings "
        "(funnel_id, form_id, enabled, expected_destination_version_id, mapping_version_id) "
        "VALUES (?, 'form-contact', 1, ?, ?)",
        (FUNNEL_ID, DESTINATION_A, MAPPING_A),
    )


def insert_task(connection: sqlite3.Connection, submission_id: str, created_at: int = 100) -> None:
    connection.execute(
        "INSERT INTO public_submission_delivery_outbox "
        "(task_id, source_submission_id, funnel_id, revision_id, page_id, form_id, "
        "destination_version_id, mapping_version_id, state, created_at) "
        "SELECT ?, source.submission_id, source.funnel_id, source.revision_id, source.page_id, "
        "source.form_id, ?, ?, 'pending', ? FROM public_form_submissions AS source "
        "WHERE source.submission_id = ?",
        (submission_id, DESTINATION_A, MAPPING_A, created_at, submission_id),
    )


def main() -> int:
    clock = [100]
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    connection.create_function("unixepoch", 0, lambda: clock[0])
    apply_before(connection, MIGRATION_NAME)
    seed_funnel(connection)
    insert_submission(connection, SUBMISSION_ID, SUBMISSION_KEY)
    connection.executescript(
        (ROOT / "migrations" / MIGRATION_NAME).read_text(encoding="utf-8")
    )

    assert connection.execute(
        "SELECT enabled, destination_version_id FROM webhook_installation_delivery "
        "WHERE installation_id = 1"
    ).fetchone() == (0, None)
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_delivery_outbox"
    ).fetchone()[0] == 0

    outbox_columns = {
        row[1] for row in connection.execute("PRAGMA table_info(public_submission_delivery_outbox)")
    }
    assert outbox_columns == {
        "task_id",
        "source_submission_id",
        "funnel_id",
        "revision_id",
        "page_id",
        "form_id",
        "destination_version_id",
        "mapping_version_id",
        "state",
        "created_at",
    }, outbox_columns
    assert not outbox_columns.intersection(
        {
            "field_values_json",
            "request_consent_copy",
            "submission_key",
            "request_fingerprint",
            "secret",
            "payload_json",
        }
    )

    seed_configuration(connection)
    assert connection.execute(
        "SELECT COUNT(*) FROM retained_visible_public_submission_delivery"
    ).fetchone()[0] == 0

    insert_task(connection, SUBMISSION_ID)
    assert connection.execute(
        "SELECT task_id, source_submission_id, destination_version_id, mapping_version_id, state "
        "FROM public_submission_delivery_outbox"
    ).fetchone() == (SUBMISSION_ID, SUBMISSION_ID, DESTINATION_A, MAPPING_A, "pending")
    assert connection.execute(
        "SELECT COUNT(*) FROM retained_visible_public_submission_delivery"
    ).fetchone()[0] == 1
    expect_rejected(
        connection,
        "UPDATE public_form_submissions SET field_values_json = '{}' WHERE submission_id = ?",
        (SUBMISSION_ID,),
        "changing an accepted source referenced by a task",
    )
    connection.execute(
        "UPDATE public_form_submissions SET triage_status = 'contacted' WHERE submission_id = ?",
        (SUBMISSION_ID,),
    )
    expect_rejected(
        connection,
        "UPDATE public_submission_delivery_outbox SET destination_version_id = ? WHERE task_id = ?",
        (DESTINATION_B, SUBMISSION_ID),
        "retargeting an existing task",
    )
    expect_rejected(
        connection,
        "UPDATE webhook_destination_versions SET destination_url = ? WHERE destination_version_id = ?",
        ("https://receiver.invalid/changed", DESTINATION_A),
        "mutating an immutable destination version",
    )
    expect_rejected(
        connection,
        "UPDATE public_form_delivery_mapping_versions SET selected_fields_json = '[]' "
        "WHERE mapping_version_id = ?",
        (MAPPING_A,),
        "mutating an immutable mapping version",
    )

    connection.execute(
        "INSERT INTO public_submission_erasure_intents "
        "(submission_id, replay_digest, source_expires_at, deleted_at, retain_until) "
        "VALUES (?, ?, 200, 110, 2592110)",
        (SUBMISSION_ID, "b" * 64),
    )
    assert connection.execute(
        "SELECT COUNT(*) FROM retained_visible_public_submission_delivery"
    ).fetchone()[0] == 0
    connection.execute(
        "DELETE FROM public_form_submissions WHERE submission_id = ?",
        (SUBMISSION_ID,),
    )
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_delivery_outbox"
    ).fetchone()[0] == 0
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_erasure_intents WHERE submission_id = ?",
        (SUBMISSION_ID,),
    ).fetchone()[0] == 1

    direct_id = "99999999-9999-4999-8999-999999999999"
    insert_submission(connection, direct_id, "88888888-8888-4888-8888-888888888888")
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_delivery_outbox WHERE source_submission_id = ?",
        (direct_id,),
    ).fetchone()[0] == 0

    connection.execute(
        "UPDATE webhook_installation_delivery SET destination_version_id = ? "
        "WHERE installation_id = 1",
        (DESTINATION_B,),
    )
    assert connection.execute(
        "SELECT COUNT(*) FROM public_form_delivery_settings AS settings "
        "JOIN webhook_installation_delivery AS installation "
        "ON installation.destination_version_id = settings.expected_destination_version_id "
        "WHERE settings.enabled = 1 AND installation.enabled = 1"
    ).fetchone()[0] == 0
    connection.execute(
        "UPDATE public_form_delivery_settings SET expected_destination_version_id = ?, "
        "mapping_version_id = ? WHERE funnel_id = ? AND form_id = 'form-contact'",
        (DESTINATION_B, MAPPING_B, FUNNEL_ID),
    )
    assert connection.execute(
        "SELECT COUNT(*) FROM public_form_delivery_settings AS settings "
        "JOIN webhook_installation_delivery AS installation "
        "ON installation.destination_version_id = settings.expected_destination_version_id "
        "WHERE settings.enabled = 1 AND installation.enabled = 1"
    ).fetchone()[0] == 1

    expired_id = "77777777-7777-4777-8777-777777777777"
    insert_submission(connection, expired_id, "66666666-6666-4666-8666-666666666666")
    connection.execute(
        "UPDATE public_form_submissions SET expires_at = ? WHERE submission_id = ?",
        (clock[0] + 1, expired_id),
    )
    connection.execute(
        "INSERT INTO public_submission_delivery_outbox "
        "(task_id, source_submission_id, funnel_id, revision_id, page_id, form_id, "
        "destination_version_id, mapping_version_id, state, created_at) "
        "SELECT ?, source.submission_id, source.funnel_id, source.revision_id, source.page_id, "
        "source.form_id, ?, ?, 'pending', ? FROM public_form_submissions AS source "
        "WHERE source.submission_id = ?",
        (expired_id, DESTINATION_B, MAPPING_B, clock[0], expired_id),
    )
    assert connection.execute(
        "SELECT COUNT(*) FROM retained_visible_public_submission_delivery "
        "WHERE source_submission_id = ?",
        (expired_id,),
    ).fetchone()[0] == 1
    clock[0] += 2
    assert connection.execute(
        "SELECT COUNT(*) FROM retained_visible_public_submission_delivery "
        "WHERE source_submission_id = ?",
        (expired_id,),
    ).fetchone()[0] == 0

    connection.close()

    fresh = sqlite3.connect(":memory:")
    fresh.execute("PRAGMA foreign_keys = ON")
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        fresh.executescript(migration.read_text(encoding="utf-8"))
    assert fresh.execute(
        "SELECT enabled, destination_version_id FROM webhook_installation_delivery "
        "WHERE installation_id = 1"
    ).fetchone() == (0, None)
    assert fresh.execute(
        "SELECT COUNT(*) FROM public_submission_delivery_outbox"
    ).fetchone()[0] == 0
    fresh.close()

    print(
        "PUB-008 migration: default inbox-only state, no backfill, reference-only tasks, "
        "immutability, triage updates, destination revalidation, expiry visibility, "
        "erasure suppression, cascades, and fresh/upgrade databases passed"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
