#!/usr/bin/env python3

from datetime import date, datetime, time, timedelta, timezone
import pathlib
import re
import sqlite3
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
FUNNEL_ID = "11111111-1111-4111-8111-111111111111"
REVISION_ID = "revision-current"
FINGERPRINT = "a" * 64
QUERY_SOURCE = (ROOT / "crates/storage/src/repository.rs").read_text(encoding="utf-8")
QUERY_MATCH = re.search(
    r'const PUBLIC_INTERACTION_REPORT_QUERY: &str = r#"(.*?)"#;',
    QUERY_SOURCE,
    re.DOTALL,
)
assert QUERY_MATCH, "the Worker interaction report query is present in the repository"
QUERY = QUERY_MATCH.group(1)


def insert_event(connection, event_number, *, received_at, expires_at, kind, campaign_id=None):
    event_id = f"00000000-0000-4000-8000-{event_number:012d}"
    form_id = None if kind == "page_viewed" else "form-contact"
    connection.execute(
        "INSERT INTO public_interaction_events "
        "(event_id, funnel_id, revision_id, page_id, event_kind, form_id, campaign_id, "
        "replay_fingerprint, received_at, expires_at) "
        "VALUES (?, ?, ?, 'page-home', ?, ?, ?, ?, ?, ?)",
        (
            event_id,
            FUNNEL_ID,
            REVISION_ID,
            kind,
            form_id,
            campaign_id,
            f"{event_number:064x}",
            received_at,
            expires_at,
        ),
    )


def report_rows(connection, period_days):
    return connection.execute(
        QUERY,
        (FUNNEL_ID, period_days - 1, period_days),
    ).fetchall()


def main() -> int:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        connection.executescript(migration.read_text(encoding="utf-8"))

    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES (?, ?, ?)",
        (FUNNEL_ID, "Report fixture", "report-fixture"),
    )
    connection.execute(
        "INSERT INTO revisions "
        "(id, funnel_id, schema_version, data_json, render_manifest_json) "
        "VALUES (?, ?, 2, '{}', '{}')",
        (REVISION_ID, FUNNEL_ID),
    )
    connection.execute(
        "UPDATE funnels SET published_revision_id = ? WHERE id = ?",
        (REVISION_ID, FUNNEL_ID),
    )

    now = int(datetime.now(timezone.utc).timestamp())
    today = datetime.fromtimestamp(now, timezone.utc).date()
    first_day = today - timedelta(days=6)
    first_day_start = int(datetime.combine(first_day, time.min, timezone.utc).timestamp())
    today_start = int(datetime.combine(today, time.min, timezone.utc).timestamp())
    tomorrow_start = today_start + 86_400
    expires = now + 86_400

    insert_event(
        connection,
        1,
        received_at=first_day_start,
        expires_at=expires,
        kind="page_viewed",
    )
    insert_event(
        connection,
        2,
        received_at=now,
        expires_at=expires,
        kind="page_viewed",
        campaign_id="spring-launch",
    )
    insert_event(
        connection,
        3,
        received_at=today_start + 60,
        expires_at=expires,
        kind="form_viewed",
    )
    insert_event(
        connection,
        4,
        received_at=today_start + 120,
        expires_at=expires,
        kind="form_viewed",
    )
    insert_event(
        connection,
        5,
        received_at=today_start + 180,
        expires_at=expires,
        kind="form_started",
    )
    insert_event(
        connection,
        6,
        received_at=first_day_start - 1,
        expires_at=expires,
        kind="page_viewed",
    )
    insert_event(
        connection,
        7,
        received_at=now - 1,
        expires_at=now,
        kind="form_started",
    )
    insert_event(
        connection,
        8,
        received_at=tomorrow_start,
        expires_at=tomorrow_start + 86_400,
        kind="page_viewed",
    )

    rows = report_rows(connection, 7)
    daily_rows = [row for row in rows if row[0] == "day_kind"]
    campaign_rows = [row for row in rows if row[0] == "campaign"]
    assert len(daily_rows) == 7 * 3, daily_rows
    assert [row[1] for row in daily_rows[::3]] == [
        (today - timedelta(days=offset)).isoformat() for offset in range(6, -1, -1)
    ], daily_rows
    assert all(row[2] is not None and row[3] is None for row in daily_rows), daily_rows
    daily = {(row[1], row[2]): row[4] for row in daily_rows}
    assert daily[(first_day.isoformat(), "page_viewed")] == 1, daily
    assert daily[(today.isoformat(), "page_viewed")] == 1, daily
    assert daily[(today.isoformat(), "form_viewed")] == 2, daily
    assert daily[(today.isoformat(), "form_started")] == 1, daily
    assert sum(daily.values()) == 5, daily
    assert campaign_rows == [
        ("campaign", None, None, None, 4),
        ("campaign", None, None, "spring-launch", 1),
    ], campaign_rows

    for period_days in (30, 90):
        period_rows = report_rows(connection, period_days)
        period_daily = [row for row in period_rows if row[0] == "day_kind"]
        assert len(period_daily) == period_days * 3, (period_days, len(period_daily))
        assert period_daily[-1][1] == today.isoformat(), period_daily[-1]
        assert sum(row[4] for row in period_daily) == 6, period_days

    connection.close()
    print("PR-069 interaction report query: UTC bounds, zero fill, expiry, and campaign grouping passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
