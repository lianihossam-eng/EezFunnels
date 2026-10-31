#!/usr/bin/env python3

import pathlib
import sqlite3


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATION = "0022_funnel_campaigns.sql"
FUNNEL_ID = "11111111-1111-4111-8111-111111111111"
REVISION_ID = "revision-pub012"


def apply_before(connection: sqlite3.Connection, migration_name: str) -> None:
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        if migration.name < migration_name:
            connection.executescript(migration.read_text(encoding="utf-8"))


def expect_rejected(connection: sqlite3.Connection, statement: str, parameters=()) -> None:
    try:
        connection.execute(statement, parameters)
    except sqlite3.IntegrityError:
        return
    raise AssertionError(f"invalid campaign registry data was accepted: {statement}")


def open_database() -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    return connection


def test_fresh_install() -> None:
    connection = open_database()
    apply_before(connection, "9999_after_pub012.sql")
    assert connection.execute(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'funnel_campaigns'"
    ).fetchone()[0] == 1
    connection.close()


def test_upgrade_preserves_events_and_bounds_registry() -> None:
    connection = open_database()
    apply_before(connection, MIGRATION)
    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES (?, 'PUB-012 fixture', 'pub012')",
        (FUNNEL_ID,),
    )
    connection.execute(
        "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) "
        "VALUES (?, ?, 2, '{}', '{}')",
        (REVISION_ID, FUNNEL_ID),
    )
    connection.execute(
        "UPDATE funnels SET published_revision_id = ? WHERE id = ?",
        (REVISION_ID, FUNNEL_ID),
    )
    connection.execute(
        "INSERT INTO public_interaction_events (event_id, funnel_id, revision_id, page_id, "
        "event_kind, campaign_id, replay_fingerprint, received_at, expires_at) "
        "VALUES ('00000000-0000-4000-8000-000000000012', ?, ?, 'page-home', 'page_viewed', "
        "'legacy-campaign', ?, 100, 200)",
        (FUNNEL_ID, REVISION_ID, "a" * 64),
    )

    connection.executescript((ROOT / "migrations" / MIGRATION).read_text(encoding="utf-8"))
    assert connection.execute(
        "SELECT campaign_id FROM public_interaction_events WHERE event_id = "
        "'00000000-0000-4000-8000-000000000012'"
    ).fetchone()[0] == "legacy-campaign"
    assert connection.execute("SELECT COUNT(*) FROM funnel_campaigns").fetchone()[0] == 0

    connection.execute(
        "INSERT INTO funnel_campaigns (funnel_id, campaign_id) VALUES (?, 'spring-launch')",
        (FUNNEL_ID,),
    )
    connection.execute(
        "INSERT INTO public_interaction_events (event_id, funnel_id, revision_id, page_id, "
        "event_kind, campaign_id, replay_fingerprint, received_at, expires_at) "
        "VALUES ('00000000-0000-4000-8000-000000000013', ?, ?, 'page-home', 'page_viewed', "
        "'spring-launch', ?, 110, 210)",
        (FUNNEL_ID, REVISION_ID, "b" * 64),
    )
    connection.execute(
        "DELETE FROM funnel_campaigns WHERE funnel_id = ? AND campaign_id = 'spring-launch'",
        (FUNNEL_ID,),
    )
    assert connection.execute(
        "SELECT campaign_id FROM public_interaction_events WHERE event_id = "
        "'00000000-0000-4000-8000-000000000013'"
    ).fetchone()[0] == "spring-launch"

    for invalid in ["", "-launch", "has space", "a" * 65, "campaign/one"]:
        expect_rejected(
            connection,
            "INSERT INTO funnel_campaigns (funnel_id, campaign_id) VALUES (?, ?)",
            (FUNNEL_ID, invalid),
        )
    connection.execute(
        "INSERT INTO funnel_campaigns (funnel_id, campaign_id) VALUES (?, 'Spring-launch')",
        (FUNNEL_ID,),
    )

    capacity = 100
    insert_campaign = (
        "INSERT INTO funnel_campaigns (funnel_id, campaign_id) "
        "SELECT ?, ? WHERE EXISTS (SELECT 1 FROM funnels WHERE id = ?) "
        "AND (SELECT COUNT(*) FROM funnel_campaigns WHERE funnel_id = ?) < ? "
        "ON CONFLICT (funnel_id, campaign_id) DO NOTHING RETURNING campaign_id"
    )
    for index in range(capacity - 1):
        result = connection.execute(
            insert_campaign,
            (FUNNEL_ID, f"campaign-{index:03}", FUNNEL_ID, FUNNEL_ID, capacity),
        ).fetchone()
        assert result is not None, index
    rejected = connection.execute(
        insert_campaign,
        (FUNNEL_ID, "capacity-overflow", FUNNEL_ID, FUNNEL_ID, capacity),
    ).fetchone()
    assert rejected is None
    assert connection.execute(
        "SELECT COUNT(*) FROM funnel_campaigns WHERE funnel_id = ?", (FUNNEL_ID,)
    ).fetchone()[0] == capacity

    connection.execute("DELETE FROM funnels WHERE id = ?", (FUNNEL_ID,))
    assert connection.execute("SELECT COUNT(*) FROM funnel_campaigns").fetchone()[0] == 0
    connection.close()


def main() -> None:
    test_fresh_install()
    test_upgrade_preserves_events_and_bounds_registry()
    print(
        "PUB-012 migration: fresh and existing installs, historical events, exact IDs, "
        "100-ID capacity, and funnel cascade passed"
    )


if __name__ == "__main__":
    main()
