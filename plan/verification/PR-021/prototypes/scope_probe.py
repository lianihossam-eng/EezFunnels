"""SQLite-only proof sketch for scoped relational keys and atomic publication/outbox.

This probes schema/query properties with Python's sqlite3 module. It does not run
the Rust repository, Cloudflare D1, R2, KV, or authentication code.
"""

import sqlite3


def check(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)
    print(f"PASS {message}")


db = sqlite3.connect(":memory:")
db.execute("PRAGMA foreign_keys = ON")
print(f"SQLite {sqlite3.sqlite_version}; foreign_keys={db.execute('PRAGMA foreign_keys').fetchone()[0]}")
db.executescript(
    """
    CREATE TABLE workspaces (
        workspace_id TEXT PRIMARY KEY
    );
    CREATE TABLE funnels (
        workspace_id TEXT NOT NULL,
        funnel_id TEXT NOT NULL,
        slug TEXT NOT NULL,
        published_revision_id TEXT,
        PRIMARY KEY (workspace_id, funnel_id),
        UNIQUE (workspace_id, slug),
        FOREIGN KEY (workspace_id) REFERENCES workspaces(workspace_id),
        FOREIGN KEY (workspace_id, funnel_id, published_revision_id)
          REFERENCES revisions(workspace_id, funnel_id, revision_id)
          DEFERRABLE INITIALLY DEFERRED
    );
    CREATE TABLE revisions (
        workspace_id TEXT NOT NULL,
        funnel_id TEXT NOT NULL,
        revision_id TEXT NOT NULL,
        manifest_json TEXT NOT NULL,
        PRIMARY KEY (workspace_id, funnel_id, revision_id),
        FOREIGN KEY (workspace_id, funnel_id)
          REFERENCES funnels(workspace_id, funnel_id)
    );
    CREATE TABLE orders (
        workspace_id TEXT NOT NULL,
        order_id TEXT NOT NULL,
        funnel_id TEXT NOT NULL,
        revision_id TEXT NOT NULL,
        PRIMARY KEY (workspace_id, order_id),
        FOREIGN KEY (workspace_id, funnel_id, revision_id)
          REFERENCES revisions(workspace_id, funnel_id, revision_id)
    );
    CREATE TABLE outbox (
        workspace_id TEXT NOT NULL,
        event_key TEXT NOT NULL,
        aggregate_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        PRIMARY KEY (workspace_id, event_key),
        FOREIGN KEY (workspace_id) REFERENCES workspaces(workspace_id)
    );
    """
)
db.executemany("INSERT INTO workspaces VALUES (?)", [("w-a",), ("w-b",)])
db.executemany(
    "INSERT INTO funnels(workspace_id, funnel_id, slug) VALUES (?, ?, ?)",
    [("w-a", "f-1", "offer"), ("w-b", "f-1", "offer")],
)
db.executemany(
    "INSERT INTO revisions VALUES (?, ?, ?, ?)",
    [
        ("w-a", "f-1", "r-a1", '{"price": "10"}'),
        ("w-a", "f-1", "r-a2", '{"price": "12"}'),
        ("w-b", "f-1", "r-b1", '{"price": "99"}'),
    ],
)
check(
    db.execute("SELECT COUNT(*) FROM funnels WHERE slug = 'offer'").fetchone()[0] == 2,
    "same slug and resource IDs can exist in separate workspace scopes",
)
check(
    db.execute(
        "SELECT revision_id FROM revisions WHERE workspace_id = ? AND funnel_id = ?",
        ("w-a", "f-1"),
    ).fetchone()[0]
    == "r-a1",
    "scoped revision lookup selects only the caller's workspace row",
)

try:
    db.execute("INSERT INTO orders VALUES ('w-a', 'o-cross', 'f-1', 'r-b1')")
except sqlite3.IntegrityError:
    check(True, "composite foreign key rejects an order linked to another workspace revision")
else:
    check(False, "composite foreign key rejects an order linked to another workspace revision")

try:
    db.commit()
    db.execute("BEGIN")
    db.execute("UPDATE funnels SET published_revision_id = 'r-b1' WHERE workspace_id = 'w-a' AND funnel_id = 'f-1'")
    db.commit()
except sqlite3.IntegrityError:
    db.rollback()
    check(True, "composite foreign key rejects cross-workspace published pointer")
else:
    check(False, "composite foreign key rejects cross-workspace published pointer")

# Demonstrate that composite keys do not rescue an unscoped query when IDs overlap.
db.execute("UPDATE funnels SET slug = 'bad-unscoped-write' WHERE funnel_id = 'f-1'")
bad_rows = db.execute("SELECT workspace_id FROM funnels WHERE slug = 'bad-unscoped-write' ORDER BY workspace_id").fetchall()
check(
    bad_rows == [("w-a",), ("w-b",)],
    "unscoped query still mutates both matching owners; scope must be mandatory at repository boundary",
)
db.execute("UPDATE funnels SET slug = 'offer' WHERE slug = 'bad-unscoped-write'")
db.commit()

# A publish transition and its event intent commit together. The injected duplicate
# outbox key makes the second transaction fail, so its pointer update must roll back.
db.execute("BEGIN")
changed = db.execute(
    "UPDATE funnels SET published_revision_id = ? WHERE workspace_id = ? AND funnel_id = ? AND published_revision_id IS ?",
    ("r-a1", "w-a", "f-1", None),
).rowcount
check(changed == 1, "compare-and-swap publish claims the expected empty pointer once")
db.execute(
    "INSERT INTO outbox VALUES (?, ?, ?, ?)",
    ("w-a", "publish:f-1:r-a1", "f-1", "funnel.published"),
)
db.commit()

db.execute("BEGIN")
changed = db.execute(
    "UPDATE funnels SET published_revision_id = ? WHERE workspace_id = ? AND funnel_id = ? AND published_revision_id IS ?",
    ("r-a2", "w-a", "f-1", "r-a1"),
).rowcount
check(changed == 1, "second transition claims only the expected current pointer")
try:
    db.execute(
        "INSERT INTO outbox VALUES (?, ?, ?, ?)",
        ("w-a", "publish:f-1:r-a1", "f-1", "funnel.published"),
    )
    db.commit()
except sqlite3.IntegrityError:
    db.rollback()
    pointer = db.execute(
        "SELECT published_revision_id FROM funnels WHERE workspace_id = 'w-a' AND funnel_id = 'f-1'"
    ).fetchone()[0]
    check(pointer == "r-a1", "outbox failure rolls back publication pointer in the same SQLite transaction")
else:
    check(False, "outbox failure rolls back publication pointer in the same SQLite transaction")

print("LIMITS SQLite-only: no D1/Workers/R2/KV transaction proof; no authentication or tenant runtime proof; no restore/backup proof; no latency/cost claim.")
