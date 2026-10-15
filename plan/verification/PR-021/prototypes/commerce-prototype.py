#!/usr/bin/env python3
"""Isolated SQLite prototype for commerce/outbox failure semantics.

This is a standard-library model. It does not exercise D1, Workers, Cloudflare
Queues/Workflows, or a real payment provider.
"""

from __future__ import annotations

import json
import sqlite3
import tempfile
from pathlib import Path


def connect(path: Path) -> sqlite3.Connection:
    db = sqlite3.connect(path)
    db.execute("PRAGMA foreign_keys = ON")
    db.executescript(
        """
        CREATE TABLE IF NOT EXISTS orders (
          order_id TEXT PRIMARY KEY,
          status TEXT NOT NULL,
          source_revision_id TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS command_receipts (
          idempotency_key TEXT PRIMARY KEY,
          payload_json TEXT NOT NULL,
          result_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS outbox (
          event_id TEXT PRIMARY KEY,
          order_id TEXT NOT NULL REFERENCES orders(order_id),
          event_type TEXT NOT NULL,
          dispatched INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS provider_events (
          connection_id TEXT NOT NULL,
          provider_event_id TEXT NOT NULL,
          provider_object_id TEXT NOT NULL,
          object_version INTEGER NOT NULL,
          normalized_state TEXT NOT NULL,
          disposition TEXT NOT NULL,
          PRIMARY KEY (connection_id, provider_event_id)
        );
        CREATE TABLE IF NOT EXISTS payment_projection (
          provider_object_id TEXT PRIMARY KEY,
          object_version INTEGER NOT NULL,
          observed_state TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS payment_attempts (
          attempt_id TEXT PRIMARY KEY,
          idempotency_key TEXT NOT NULL UNIQUE,
          state TEXT NOT NULL,
          provider_reference TEXT
        );
        """
    )
    return db


def create_order(
    db: sqlite3.Connection,
    *,
    key: str,
    payload: dict[str, str],
    fail_before_commit: bool = False,
) -> str:
    payload_json = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    existing = db.execute(
        "SELECT payload_json, result_json FROM command_receipts WHERE idempotency_key = ?",
        (key,),
    ).fetchone()
    if existing:
        if existing[0] != payload_json:
            raise ValueError("idempotency_payload_mismatch")
        return existing[1]

    result_json = json.dumps({"order_id": payload["order_id"], "status": "pending"}, sort_keys=True)
    db.execute("BEGIN IMMEDIATE")
    try:
        db.execute(
            "INSERT INTO orders(order_id, status, source_revision_id) VALUES (?, 'pending', ?)",
            (payload["order_id"], payload["revision_id"]),
        )
        db.execute(
            "INSERT INTO command_receipts(idempotency_key, payload_json, result_json) VALUES (?, ?, ?)",
            (key, payload_json, result_json),
        )
        db.execute(
            "INSERT INTO outbox(event_id, order_id, event_type) VALUES (?, ?, 'order.created')",
            (f"evt-{payload['order_id']}-created", payload["order_id"]),
        )
        if fail_before_commit:
            raise RuntimeError("simulated_crash_before_commit")
        db.commit()
    except BaseException:
        db.rollback()
        raise
    return result_json


def ingest_provider_event(
    db: sqlite3.Connection,
    connection_id: str,
    event_id: str,
    object_id: str,
    object_version: int,
    state: str,
) -> str:
    prior = db.execute(
        "SELECT disposition FROM provider_events WHERE connection_id = ? AND provider_event_id = ?",
        (connection_id, event_id),
    ).fetchone()
    if prior:
        return "duplicate_ignored"

    current = db.execute(
        "SELECT object_version FROM payment_projection WHERE provider_object_id = ?",
        (object_id,),
    ).fetchone()
    if current and object_version <= current[0]:
        disposition = "stale_stored_reconcile_required"
    else:
        disposition = "applied"
        db.execute(
            "INSERT INTO payment_projection(provider_object_id, object_version, observed_state) "
            "VALUES (?, ?, ?) ON CONFLICT(provider_object_id) DO UPDATE SET "
            "object_version = excluded.object_version, observed_state = excluded.observed_state",
            (object_id, object_version, state),
        )
    db.execute(
        "INSERT INTO provider_events(connection_id, provider_event_id, provider_object_id, "
        "object_version, normalized_state, disposition) VALUES (?, ?, ?, ?, ?, ?)",
        (connection_id, event_id, object_id, object_version, state, disposition),
    )
    return disposition


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="eez-commerce-prototype-") as temp:
        path = Path(temp) / "prototype.sqlite"
        db = connect(path)
        payload = {"order_id": "ord-1", "revision_id": "rev-7", "offer_version_id": "offer-v3"}

        try:
            create_order(db, key="checkout-1", payload=payload, fail_before_commit=True)
        except RuntimeError as error:
            print(f"crash_before_commit={error}")
        print(
            "after_precommit_crash="
            f"orders:{db.execute('SELECT COUNT(*) FROM orders').fetchone()[0]},"
            f"receipts:{db.execute('SELECT COUNT(*) FROM command_receipts').fetchone()[0]},"
            f"outbox:{db.execute('SELECT COUNT(*) FROM outbox').fetchone()[0]}"
        )

        create_order(db, key="checkout-1", payload=payload)
        print("crash_after_commit=simulated_before_queue_publish")
        replay = create_order(db, key="checkout-1", payload=payload)
        print(f"same_key_replay_result={replay}")
        print(
            "after_postcommit_replay="
            f"orders:{db.execute('SELECT COUNT(*) FROM orders').fetchone()[0]},"
            f"receipts:{db.execute('SELECT COUNT(*) FROM command_receipts').fetchone()[0]},"
            f"outbox:{db.execute('SELECT COUNT(*) FROM outbox').fetchone()[0]}"
        )

        print("provider_event_v2=" + ingest_provider_event(db, "pc-1", "evt-2", "pay-1", 2, "paid"))
        print("provider_duplicate_evt_2=" + ingest_provider_event(db, "pc-1", "evt-2", "pay-1", 2, "paid"))
        print("provider_out_of_order_v1=" + ingest_provider_event(db, "pc-1", "evt-1", "pay-1", 1, "pending"))
        print(
            "payment_projection="
            + json.dumps(
                db.execute(
                    "SELECT provider_object_id, object_version, observed_state FROM payment_projection"
                ).fetchone()
            )
        )

        db.execute(
            "INSERT INTO payment_attempts(attempt_id, idempotency_key, state) VALUES ('attempt-1', 'pay-key-1', 'unknown')"
        )
        print("provider_accept_then_response_lost=attempt_unknown")
        # The fake provider reports its idempotent remote result during reconciliation.
        db.execute(
            "UPDATE payment_attempts SET state = 'confirmed', provider_reference = 'pay-remote-1' "
            "WHERE idempotency_key = 'pay-key-1' AND state = 'unknown'"
        )
        print(
            "after_reconcile="
            + json.dumps(
                db.execute("SELECT state, provider_reference FROM payment_attempts").fetchone()
            )
        )

        db.close()


if __name__ == "__main__":
    main()
