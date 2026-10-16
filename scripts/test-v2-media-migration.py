#!/usr/bin/env python3

import json
import pathlib
import sqlite3
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATIONS = sorted((ROOT / "migrations").glob("*.sql"))
MEDIA_MIGRATION = ROOT / "migrations" / "0009_v2_media_library.sql"
ASSET_ID = "a" * 64


def apply_before_media(connection: sqlite3.Connection) -> None:
    for migration in MIGRATIONS:
        if migration.name == MEDIA_MIGRATION.name:
            break
        connection.executescript(migration.read_text(encoding="utf-8"))
    connection.execute(
        "INSERT INTO assets (id, r2_key, mime, size_bytes) VALUES (?, ?, ?, ?)",
        ("legacy", "legacy/object", "image/svg+xml", 100),
    )
    connection.commit()


def expect_integrity_error(connection: sqlite3.Connection, message: str, operation) -> None:
    try:
        operation()
    except sqlite3.IntegrityError as error:
        assert message in str(error), error
    else:
        raise AssertionError(f"expected {message!r} integrity error")


def main() -> int:
    connection = sqlite3.connect(":memory:")
    connection.execute("PRAGMA foreign_keys = ON")
    apply_before_media(connection)
    connection.executescript(MEDIA_MIGRATION.read_text(encoding="utf-8"))

    legacy = connection.execute(
        "SELECT state, archived, sha256 FROM assets WHERE id = 'legacy'"
    ).fetchone()
    assert legacy == ("legacy_unverified", 0, None), legacy

    connection.execute(
        "INSERT INTO assets (id, r2_key, mime, size_bytes, sha256, format, width, height, display_name, state) "
        "VALUES (?, ?, 'image/png', 16, ?, 'png', 2, 2, 'pixel.png', 'pending')",
        (ASSET_ID, f"private/media/v1/{ASSET_ID}", ASSET_ID),
    )
    connection.execute("UPDATE assets SET state = 'ready' WHERE id = ?", (ASSET_ID,))

    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES ('fixture', 'Fixture', 'fixture')"
    )
    manifest_asset = {
        "asset_id": ASSET_ID,
        "sha256": ASSET_ID,
        "format": "png",
        "byte_length": 16,
        "width": 2,
        "height": 2,
    }
    connection.execute(
        "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) "
        "VALUES ('revision-1', 'fixture', 2, '{}', ?)",
        (json.dumps({"assets": [manifest_asset]}),),
    )
    reference = connection.execute(
        "SELECT revision_id, asset_id FROM revision_asset_refs"
    ).fetchall()
    assert reference == [("revision-1", ASSET_ID)], reference
    connection.execute(
        "UPDATE funnels SET published_revision_id = 'revision-1' WHERE id = 'fixture'"
    )
    assert connection.execute(
        "SELECT COUNT(*) FROM revision_asset_refs refs "
        "JOIN revisions ON revisions.id = refs.revision_id "
        "JOIN funnels ON funnels.id = revisions.funnel_id "
        "AND funnels.published_revision_id = revisions.id WHERE refs.asset_id = ?",
        (ASSET_ID,),
    ).fetchone() == (1,)

    expect_integrity_error(
        connection,
        "matching assets",
        lambda: connection.execute(
            "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) "
            "VALUES ('revision-bad', 'fixture', 2, '{}', ?)",
            (json.dumps({"assets": [{**manifest_asset, "byte_length": 99}]}),),
        ),
    )
    expect_integrity_error(
        connection,
        "archived instead of deleted",
        lambda: connection.execute("DELETE FROM assets WHERE id = ?", (ASSET_ID,)),
    )
    expect_integrity_error(
        connection,
        "descriptors are immutable",
        lambda: connection.execute(
            "UPDATE assets SET size_bytes = 17 WHERE id = ?", (ASSET_ID,)
        ),
    )
    connection.execute("UPDATE assets SET archived = 1 WHERE id = ?", (ASSET_ID,))
    assert connection.execute(
        "SELECT archived FROM assets WHERE id = ?", (ASSET_ID,)
    ).fetchone() == (1,)

    connection.close()
    print("V2 media migration: legacy isolation, manifest binding, publication refs, archival and immutability passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
