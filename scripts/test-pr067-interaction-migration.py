#!/usr/bin/env python3

import pathlib
import sqlite3
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATION_NAME = "0015_public_interaction_events.sql"
FUNNEL_ID = "11111111-1111-4111-8111-111111111111"
FINGERPRINT = "a" * 64


def insert_event(connection, event_id, *, revision_id="revision-current", kind="page_viewed", form_id=None):
    connection.execute(
        "INSERT INTO public_interaction_events "
        "(event_id, funnel_id, revision_id, page_id, event_kind, form_id, campaign_id, "
        "replay_fingerprint, received_at, expires_at) "
        "VALUES (?, ?, ?, 'page-home', ?, ?, NULL, ?, 100, 2592100)",
        (event_id, FUNNEL_ID, revision_id, kind, form_id, FINGERPRINT),
    )


def main() -> int:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        if migration.name != MIGRATION_NAME:
            connection.executescript(migration.read_text(encoding="utf-8"))

    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES (?, ?, ?)",
        (FUNNEL_ID, "Fixture", "fixture"),
    )
    connection.executemany(
        "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) "
        "VALUES (?, ?, 2, '{}', '{}')",
        [("revision-current", FUNNEL_ID), ("revision-old", FUNNEL_ID)],
    )
    connection.execute(
        "UPDATE funnels SET published_revision_id = 'revision-current' WHERE id = ?",
        (FUNNEL_ID,),
    )

    migration = ROOT / "migrations" / MIGRATION_NAME
    connection.executescript(migration.read_text(encoding="utf-8"))

    columns = {
        row[1] for row in connection.execute("PRAGMA table_info(public_interaction_events)")
    }
    assert columns == {
        "event_id",
        "funnel_id",
        "revision_id",
        "page_id",
        "event_kind",
        "form_id",
        "campaign_id",
        "replay_fingerprint",
        "received_at",
        "expires_at",
    }, columns

    expiry_index = connection.execute(
        "PRAGMA index_info(public_interaction_events_expiry_idx)"
    ).fetchall()
    assert [row[2] for row in expiry_index] == ["expires_at", "event_id"], expiry_index
    rate_index = connection.execute(
        "PRAGMA index_info(public_interaction_events_rate_idx)"
    ).fetchall()
    assert [row[2] for row in rate_index] == ["funnel_id", "received_at"], rate_index
    report_index = connection.execute(
        "PRAGMA index_info(public_interaction_events_report_idx)"
    ).fetchall()
    assert [row[2] for row in report_index] == [
        "funnel_id",
        "received_at",
        "event_kind",
        "campaign_id",
    ], report_index

    insert_event(connection, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
    insert_event(
        connection,
        "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        kind="form_viewed",
        form_id="form-contact",
    )
    for event_id, kind, form_id in [
        ("cccccccc-cccc-4ccc-8ccc-cccccccccccc", "page_viewed", "form-contact"),
        ("dddddddd-dddd-4ddd-8ddd-dddddddddddd", "form_started", None),
        ("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee", "unknown", None),
    ]:
        try:
            insert_event(connection, event_id, kind=kind, form_id=form_id)
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError(f"invalid event shape was accepted: {(kind, form_id)}")

    try:
        insert_event(
            connection,
            "ffffffff-ffff-4fff-8fff-ffffffffffff",
            revision_id="revision-old",
        )
    except sqlite3.IntegrityError:
        pass
    else:
        raise AssertionError("event for an unpublished revision was accepted")

    expiry_plan = connection.execute(
        "EXPLAIN QUERY PLAN SELECT event_id FROM public_interaction_events "
        "WHERE expires_at <= 200 ORDER BY expires_at LIMIT 10000"
    ).fetchall()
    assert any("public_interaction_events_expiry_idx" in str(row[3]) for row in expiry_plan), expiry_plan

    connection.execute("DELETE FROM funnels WHERE id = ?", (FUNNEL_ID,))
    remaining = connection.execute(
        "SELECT COUNT(*) FROM public_interaction_events"
    ).fetchone()[0]
    assert remaining == 0, remaining

    connection.close()
    print("PR-067 migration: interaction privacy columns, indexes, constraints, publication gate, and funnel cascade passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
