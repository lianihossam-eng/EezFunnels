# PUB-008: Cloudflare D1 transaction notes for the outbox

Research checked against primary documentation on 2026-10-30. The Cloudflare D1 `D1Database` page showed a last-updated date of 2026-06-22 when accessed.

## Documented behavior

- `D1Database.batch()` accepts an ordered list of prepared statements in one call. Cloudflare documents that statements execute sequentially and non-concurrently, and that batch results are returned in statement order.
- Cloudflare describes a batch as a SQL transaction: if a statement in the sequence fails, D1 returns an error for that statement and aborts or rolls back the whole sequence. D1 otherwise operates in auto-commit mode. [Cloudflare D1 Database, `batch()`](https://developers.cloudflare.com/d1/worker-api/d1-database/#batch)
- SQLite's `ON CONFLICT ... DO NOTHING` omits a conflicting insert and performs a no-op. That is a handled conflict, not a failed statement that should be expected to abort a D1 batch. [SQLite UPSERT](https://www.sqlite.org/lang_upsert.html)

## Implications for PUB-008

- Put the accepted-submission insert and its outbox insert in the same D1 batch so a failed outbox statement rolls back the source insert as well. A separate D1 call would be a separate auto-commit boundary; writes to R2 or an HTTP receiver are outside this D1 transaction.
- Make task creation conditional on a newly accepted submission. The public retry path can hit the submission's unique-key conflict; because `DO NOTHING` is not an error, a later batch statement must not infer that a new submission was inserted merely because the source row exists. Encode the condition inside the D1 write path, and keep the canonical-submission lookup ahead of task creation as specified by the series contract.
- Verify the behavior with the Worker/D1 runtime used by the repository: a forced failure in the outbox statement leaves neither row; a duplicate submission retry creates no task; inbox-only submissions create none; and changing the active destination or mapping cannot retarget an existing task.

The documentation establishes atomicity for one D1 batch. It does not prescribe the conditional outbox pattern; that choice and its failure-path tests remain implementation responsibilities.

## Runtime evidence (PUB-008)

On 2026-10-30, `bash scripts/smoke-pr033-public-forms.sh` passed. Its service scenarios ran against the repository's local Wrangler Worker/D1 runtime; `scripts/test-pub008-migration.py` separately applied the migration against in-memory SQLite for fresh/upgrade schema, trigger, cascade, and expiry-view assertions. The Worker/D1 scenarios verified that `changes()` in the statement following `INSERT ... RETURNING` is `1` only for a newly accepted source, concurrent retries with one idempotency key produce one canonical source and one task, and a handled duplicate creates no second task. They also verified inbox-only acceptance when delivery is disabled, no backfill when delivery is enabled after an earlier acceptance, and immutable destination/mapping references across configuration changes.

The smoke injected a trigger that aborts the outbox insert. The Worker returned the existing storage-failure response and the batch left neither source nor task; retrying with the same key after removing the trigger succeeded with one source/task pair. Worker/D1 coverage also exercised source and task immutability, erasure suppression, and D1 restore reconciliation while the recovery gate remained closed. Expiry visibility is asserted by the migration test under SQLite. This is local integration evidence only: it did not contact a live webhook receiver, exercise remote Cloudflare deployment behavior, or prove production concurrency characteristics.
