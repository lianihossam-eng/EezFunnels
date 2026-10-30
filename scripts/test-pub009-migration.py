#!/usr/bin/env python3

import pathlib
import sqlite3


ROOT = pathlib.Path(__file__).resolve().parents[1]
PREVIOUS_MIGRATION = "0018_public_submission_delivery_outbox.sql"
MIGRATION = "0019_public_submission_delivery_state.sql"
FUNNEL_ID = "11111111-1111-4111-8111-111111111111"
DESTINATION_ID = "cccccccc-cccc-4ccc-8ccc-cccccccccccc"
MAPPING_ID = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"
FIRST_SUBMISSION_ID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
SECOND_SUBMISSION_ID = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"


def apply_before(connection: sqlite3.Connection, migration_name: str) -> None:
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        if migration.name < migration_name:
            connection.executescript(migration.read_text(encoding="utf-8"))


def insert_submission(connection: sqlite3.Connection, submission_id: str) -> None:
    connection.execute(
        "INSERT INTO public_form_submissions ("
        "submission_id, funnel_id, revision_id, page_id, form_id, submission_channel, "
        "submission_key, request_fingerprint, field_values_json, "
        "request_consent_acknowledged, request_consent_copy_version, request_consent_copy, "
        "request_consent_recorded_at, submitted_at, expires_at"
        ") VALUES (?, ?, 'revision-current', 'page-home', 'form-contact', 'unattributed', "
        "?, ?, '{\"email\":\"private@example.test\"}', 1, 'request-v1', "
        "'I agree to be contacted.', 100, 100, 100000)",
        (submission_id, FUNNEL_ID, submission_id, "a" * 64),
    )


def insert_task(connection: sqlite3.Connection, submission_id: str, created_at: int) -> None:
    connection.execute(
        "INSERT INTO public_submission_delivery_outbox ("
        "task_id, source_submission_id, funnel_id, revision_id, page_id, form_id, "
        "destination_version_id, mapping_version_id, state, created_at"
        ") VALUES (?, ?, ?, 'revision-current', 'page-home', 'form-contact', ?, ?, 'pending', ?)",
        (submission_id, submission_id, FUNNEL_ID, DESTINATION_ID, MAPPING_ID, created_at),
    )


def expect_rejected(connection: sqlite3.Connection, statement: str, label: str) -> None:
    try:
        connection.execute(statement)
    except sqlite3.IntegrityError:
        return
    raise AssertionError(f"invalid delivery state was accepted: {label}")


def finish_attempt(
    connection: sqlite3.Connection,
    task_id: str,
    lease_token: str,
    transport_kind: str,
    outcome_code: str,
    http_status: int | None,
    retry_delay: int,
    attempt_count: int = 1,
    attempt_timestamp: int = 100,
) -> bool:
    query = (ROOT / "crates/storage/sql/public_submission_delivery_finish.sql").read_text(
        encoding="utf-8"
    )
    parameters = (
        transport_kind,
        transport_kind,
        retry_delay,
        retry_delay,
        transport_kind,
        retry_delay,
        retry_delay,
        retry_delay,
        transport_kind,
        transport_kind,
        retry_delay,
        retry_delay,
        outcome_code,
        http_status,
        transport_kind,
        transport_kind,
        retry_delay,
        retry_delay,
        task_id,
        lease_token,
        attempt_count,
        attempt_timestamp,
    )
    return connection.execute(query, parameters).fetchone() == (task_id,)


def has_due_task(connection: sqlite3.Connection) -> bool:
    query = (ROOT / "crates/storage/sql/public_submission_delivery_due.sql").read_text(
        encoding="utf-8"
    )
    return connection.execute(query).fetchone() is not None


def terminalize_exhausted_and_expired(connection: sqlite3.Connection) -> None:
    query = (
        ROOT / "crates/storage/sql/public_submission_delivery_terminalize.sql"
    ).read_text(encoding="utf-8")
    connection.execute(query)


def main() -> int:
    now = [100]
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    connection.create_function("unixepoch", 0, lambda: now[0])
    apply_before(connection, PREVIOUS_MIGRATION)
    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES (?, 'PUB-009 fixture', 'pub009-fixture')",
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
    connection.executescript(
        (ROOT / "migrations" / PREVIOUS_MIGRATION).read_text(encoding="utf-8")
    )
    connection.execute(
        "INSERT INTO webhook_destination_versions "
        "(destination_version_id, destination_url, created_at) VALUES (?, 'https://hooks.public-site.dev/x', 100)",
        (DESTINATION_ID,),
    )
    connection.execute(
        "INSERT INTO public_form_delivery_mapping_versions "
        "(mapping_version_id, funnel_id, form_id, destination_version_id, selected_fields_json, created_at) "
        "VALUES (?, ?, 'form-contact', ?, '[\"field-email\"]', 100)",
        (MAPPING_ID, FUNNEL_ID, DESTINATION_ID),
    )

    insert_submission(connection, FIRST_SUBMISSION_ID)
    insert_task(connection, FIRST_SUBMISSION_ID, 95)
    insert_submission(connection, SECOND_SUBMISSION_ID)
    connection.executescript(
        (ROOT / "migrations" / MIGRATION).read_text(encoding="utf-8")
    )

    assert connection.execute(
        "SELECT task_id, status, attempt_count, next_attempt_at, updated_at "
        "FROM public_submission_delivery_state WHERE task_id = ?",
        (FIRST_SUBMISSION_ID,),
    ).fetchone() == (FIRST_SUBMISSION_ID, "pending", 0, 95, 95)
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_delivery_outbox"
    ).fetchone()[0] == 1
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_delivery_state"
    ).fetchone()[0] == 1

    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_delivery_outbox WHERE source_submission_id = ?",
        (SECOND_SUBMISSION_ID,),
    ).fetchone()[0] == 0
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_delivery_state WHERE task_id = ?",
        (SECOND_SUBMISSION_ID,),
    ).fetchone()[0] == 0

    third_submission_id = "dddddddd-dddd-4ddd-8ddd-dddddddddddd"
    insert_submission(connection, third_submission_id)
    insert_task(connection, third_submission_id, 100)
    assert connection.execute(
        "SELECT status, attempt_count, next_attempt_at FROM public_submission_delivery_state "
        "WHERE task_id = ?",
        (third_submission_id,),
    ).fetchone() == ("pending", 0, 100)

    connection.execute(
        "UPDATE public_submission_delivery_state SET status = 'leased', attempt_count = 1, "
        "last_attempt_at = 100, next_attempt_at = 160, lease_token = 'accepted-lease', "
        "lease_expires_at = 130 WHERE task_id = ?",
        (FIRST_SUBMISSION_ID,),
    )
    assert finish_attempt(
        connection,
        FIRST_SUBMISSION_ID,
        "accepted-lease",
        "accepted",
        "receiver_accepted",
        202,
        0,
    )
    assert connection.execute(
        "SELECT status, last_outcome_code, last_http_status, completed_at, lease_token "
        "FROM public_submission_delivery_state WHERE task_id = ?",
        (FIRST_SUBMISSION_ID,),
    ).fetchone() == ("accepted", "receiver_accepted", 202, 100, None)

    connection.execute(
        "UPDATE public_submission_delivery_state SET status = 'leased', attempt_count = 1, "
        "last_attempt_at = 100, next_attempt_at = 160, lease_token = 'retry-lease', "
        "lease_expires_at = 130 WHERE task_id = ?",
        (third_submission_id,),
    )
    assert finish_attempt(
        connection,
        third_submission_id,
        "retry-lease",
        "retryable",
        "transport_retryable",
        503,
        60,
    )
    assert connection.execute(
        "SELECT status, next_attempt_at, last_outcome_code, last_http_status, completed_at "
        "FROM public_submission_delivery_state WHERE task_id = ?",
        (third_submission_id,),
    ).fetchone() == ("pending", 160, "transport_retryable", 503, None)

    connection.execute(
        "UPDATE public_submission_delivery_state SET status = 'leased', attempt_count = 5, "
        "last_attempt_at = 100, lease_token = 'exhausted-lease', lease_expires_at = 130 "
        "WHERE task_id = ?",
        (third_submission_id,),
    )
    assert finish_attempt(
        connection,
        third_submission_id,
        "exhausted-lease",
        "retryable",
        "transport_retryable",
        503,
        86400,
        5,
    )
    assert connection.execute(
        "SELECT status, last_outcome_code, last_http_status, completed_at, lease_token "
        "FROM public_submission_delivery_state WHERE task_id = ?",
        (third_submission_id,),
    ).fetchone() == ("failed", "retry_exhausted", 503, 100, None)

    fourth_submission_id = "ffffffff-ffff-4fff-8fff-ffffffffffff"
    fifth_submission_id = "99999999-9999-4999-8999-999999999999"
    sixth_submission_id = "88888888-8888-4888-8888-888888888888"
    for submission_id in (fourth_submission_id, fifth_submission_id, sixth_submission_id):
        insert_submission(connection, submission_id)
        insert_task(connection, submission_id, 100)
    connection.execute(
        "UPDATE public_submission_delivery_state SET status = 'leased', attempt_count = 5, "
        "last_attempt_at = 100, lease_token = 'crashed-final-lease', lease_expires_at = 99 "
        "WHERE task_id = ?",
        (fifth_submission_id,),
    )
    connection.execute(
        "UPDATE public_submission_delivery_state SET status = 'leased', attempt_count = 1, "
        "last_attempt_at = 100, next_attempt_at = 160, lease_token = 'stale-lease', "
        "lease_expires_at = 99 WHERE task_id = ?",
        (sixth_submission_id,),
    )
    assert not finish_attempt(
        connection,
        sixth_submission_id,
        "stale-lease",
        "accepted",
        "receiver_accepted",
        202,
        0,
    )
    assert connection.execute(
        "SELECT status, completed_at, lease_token FROM public_submission_delivery_state "
        "WHERE task_id = ?",
        (sixth_submission_id,),
    ).fetchone() == ("leased", None, "stale-lease")
    now[0] = 86_500
    assert has_due_task(connection)
    terminalize_exhausted_and_expired(connection)
    assert connection.execute(
        "SELECT status, last_outcome_code, completed_at FROM public_submission_delivery_state "
        "WHERE task_id = ?",
        (fourth_submission_id,),
    ).fetchone() == ("failed", "delivery_window_elapsed", 86_500)
    assert connection.execute(
        "SELECT status, last_outcome_code, completed_at FROM public_submission_delivery_state "
        "WHERE task_id = ?",
        (fifth_submission_id,),
    ).fetchone() == ("failed", "retry_exhausted", 86_500)
    assert not has_due_task(connection)

    state_columns = {
        row[1] for row in connection.execute("PRAGMA table_info(public_submission_delivery_state)")
    }
    assert state_columns == {
        "task_id",
        "status",
        "attempt_count",
        "next_attempt_at",
        "lease_token",
        "lease_expires_at",
        "last_attempt_at",
        "last_http_status",
        "last_outcome_code",
        "completed_at",
        "updated_at",
    }, state_columns
    assert not state_columns.intersection(
        {"field_values_json", "submission_key", "request_fingerprint", "secret", "payload_json"}
    )
    expect_rejected(
        connection,
        "UPDATE public_submission_delivery_state SET status = 'leased' "
        "WHERE task_id = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa'",
        "leasing without a token",
    )
    expect_rejected(
        connection,
        "UPDATE public_submission_delivery_state SET attempt_count = 6 "
        "WHERE task_id = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa'",
        "more than five attempts",
    )

    connection.execute(
        "DELETE FROM public_form_submissions WHERE submission_id = ?",
        (FIRST_SUBMISSION_ID,),
    )
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_delivery_outbox WHERE task_id = ?",
        (FIRST_SUBMISSION_ID,),
    ).fetchone()[0] == 0
    assert connection.execute(
        "SELECT COUNT(*) FROM public_submission_delivery_state WHERE task_id = ?",
        (FIRST_SUBMISSION_ID,),
    ).fetchone()[0] == 0
    print("pub009 migration: backfill, reference-only state, attempt transitions, constraints, and cascade passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
