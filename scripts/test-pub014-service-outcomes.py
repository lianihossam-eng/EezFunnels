#!/usr/bin/env python3

from datetime import datetime, timezone
import pathlib
import re
import sqlite3
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
FUNNEL_ID = "11111111-1111-4111-8111-111111111111"
REVISION_ID = "revision-pub014"
QUERY_SOURCE = (ROOT / "crates/storage/src/repository.rs").read_text(encoding="utf-8")
QUERY_MATCH = re.search(
    r'const PUBLIC_SERVICE_OUTCOME_REPORT_QUERY: &str = r#"(.*?)"#;',
    QUERY_SOURCE,
    re.DOTALL,
)
assert QUERY_MATCH, "the Worker service outcome report query is present in the repository"
QUERY = QUERY_MATCH.group(1)


def expect_rejected(connection, statement, parameters=()):
    try:
        connection.execute(statement, parameters)
    except sqlite3.IntegrityError:
        return
    raise AssertionError(f"immutable history boundary accepted: {statement}")


def insert_submission(connection, suffix, submitted_at, expires_at, *, status="new"):
    submission_id = f"00000000-0000-4000-8000-{suffix:012d}"
    connection.execute(
        "INSERT INTO public_form_submissions "
        "(submission_id, funnel_id, revision_id, page_id, form_id, submission_key, "
        "request_fingerprint, field_values_json, request_consent_acknowledged, "
        "request_consent_copy_version, request_consent_copy, request_consent_recorded_at, "
        "submitted_at, expires_at, triage_status) "
        "VALUES (?, ?, ?, 'page-home', 'form-contact', ?, ?, '{}', 1, 'v1', 'Request copy', ?, ?, ?, ?)",
        (
            submission_id,
            FUNNEL_ID,
            REVISION_ID,
            f"10000000-0000-4000-8000-{suffix:012d}",
            f"{suffix:064x}",
            submitted_at,
            submitted_at,
            expires_at,
            status,
        ),
    )
    return submission_id


def record_event(connection, submission_id, status, recorded_at, *, close_reason=None, corrects=None):
    current = connection.execute(
        "SELECT follow_up_version FROM public_form_submissions WHERE submission_id = ?",
        (submission_id,),
    ).fetchone()[0]
    version = current + 1
    connection.execute(
        "INSERT INTO public_submission_follow_up_events "
        "(submission_id, version, status, close_reason, recorded_at, provenance, "
        "supersedes_version, corrects_version) VALUES (?, ?, ?, ?, ?, 'owner_follow_up', ?, ?)",
        (submission_id, version, status, close_reason, recorded_at, current or None, corrects),
    )
    connection.execute(
        "UPDATE public_form_submissions SET triage_status = ?, follow_up_version = ?, "
        "close_reason = ? WHERE submission_id = ?",
        (status, version, close_reason if status == "closed" else None, submission_id),
    )


def report(connection, period_days=90):
    cursor = connection.execute(QUERY, (FUNNEL_ID, period_days - 1))
    row = cursor.fetchone()
    assert row is not None, "existing funnel has a summary row even when counts are empty"
    return dict(zip([column[0] for column in cursor.description], row))


def main():
    now = int(datetime.now(timezone.utc).timestamp())
    clock = [now]
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    connection.create_function("unixepoch", 0, lambda: clock[0])
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        if migration.name < "0023_submission_follow_up_history.sql":
            connection.executescript(migration.read_text(encoding="utf-8"))

    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES (?, 'PUB-014 fixture', 'pub014')",
        (FUNNEL_ID,),
    )
    connection.execute(
        "INSERT INTO revisions "
        "(id, funnel_id, schema_version, data_json, render_manifest_json) "
        "VALUES (?, ?, 2, '{}', '{}')",
        (REVISION_ID, FUNNEL_ID),
    )
    connection.execute("UPDATE funnels SET published_revision_id = ? WHERE id = ?", (REVISION_ID, FUNNEL_ID))

    insert_submission(connection, 1, now - 86_400, now + 500_000, status="qualified")
    connection.executescript(
        (ROOT / "migrations" / "0023_submission_follow_up_history.sql").read_text(encoding="utf-8")
    )
    history_started_at = connection.execute(
        "SELECT history_started_at FROM public_submission_follow_up_history WHERE history_id = 1"
    ).fetchone()[0]
    assert history_started_at == now
    expect_rejected(
        connection,
        "UPDATE public_submission_follow_up_history SET history_started_at = history_started_at + 1",
    )
    expect_rejected(connection, "DELETE FROM public_submission_follow_up_history")
    expect_rejected(
        connection,
        "INSERT OR REPLACE INTO public_submission_follow_up_history "
        "(history_id, history_started_at) VALUES (1, 1)",
    )
    assert connection.execute(
        "SELECT history_started_at FROM public_submission_follow_up_history WHERE history_id = 1"
    ).fetchone()[0] == now

    clock[0] = history_started_at + 10_000
    insert_submission(
        connection,
        7,
        clock[0] - 10 * 86_400,
        clock[0] + 500_000,
    )
    insert_submission(
        connection,
        8,
        clock[0] + 86_400,
        clock[0] + 500_000,
    )
    first = insert_submission(connection, 2, history_started_at + 100, clock[0] + 500_000)
    record_event(connection, first, "contacted", history_started_at + 120)
    record_event(connection, first, "qualified", history_started_at + 140)
    record_event(connection, first, "qualified", history_started_at + 160)
    record_event(connection, first, "closed", history_started_at + 180, close_reason="An internal reason")
    record_event(
        connection,
        first,
        "closed",
        history_started_at + 200,
        corrects=4,
    )

    qualified_only = insert_submission(connection, 3, history_started_at + 300, clock[0] + 500_000)
    record_event(connection, qualified_only, "qualified", history_started_at + 320)

    closed = insert_submission(connection, 4, history_started_at + 400, clock[0] + 500_000)
    record_event(connection, closed, "closed", history_started_at + 450, close_reason="Documented reason")

    expired = insert_submission(connection, 5, history_started_at + 500, clock[0] + 10)
    record_event(connection, expired, "contacted", history_started_at + 520)
    clock[0] += 20

    erased = insert_submission(connection, 6, history_started_at + 600, clock[0] + 500_000)
    record_event(connection, erased, "contacted", history_started_at + 620)
    connection.execute(
        "INSERT INTO public_submission_erasure_intents "
        "(submission_id, replay_digest, source_expires_at, deleted_at, retain_until) "
        "VALUES (?, ?, ?, ?, ?)",
        (erased, "a" * 64, clock[0] + 500_000, clock[0], clock[0] + 500_000),
    )

    summary = report(connection)
    assert summary["row_type"] == "summary", summary
    assert summary["history_started_at"] == datetime.fromtimestamp(
        history_started_at, timezone.utc
    ).strftime("%Y-%m-%dT%H:%M:%SZ"), summary
    assert summary["retained_submissions"] == 5, summary
    assert summary["history_covered_submissions"] == 3, summary
    assert summary["current_new"] == 1, summary
    assert summary["current_contacted"] == 0, summary
    assert summary["current_qualified"] == 2, summary
    assert summary["current_closed"] == 2, summary
    assert summary["submissions_taken_in_charge"] == 1, summary
    assert summary["submissions_qualified"] == 2, summary
    assert summary["submissions_closed"] == 2, summary
    assert summary["submissions_closed_with_reason"] == 1, summary
    assert summary["first_contact_count"] == 1, summary
    assert summary["average_first_contact_delay_seconds"] == 20, summary
    assert "Documented reason" not in str(summary) and "An internal reason" not in str(summary)

    weekly_summary = report(connection, period_days=7)
    assert weekly_summary["retained_submissions"] == 4, weekly_summary
    assert weekly_summary["history_covered_submissions"] == 3, weekly_summary
    assert (
        weekly_summary["retained_submissions"]
        - weekly_summary["history_covered_submissions"]
        == 1
    ), weekly_summary

    connection.close()
    print(
        "PUB-014 service outcome report: immutable coverage boundary, retained cohorts, "
        "corrected events, explicit contact, deduplication, and private reasons passed"
    )


if __name__ == "__main__":
    sys.exit(main())
