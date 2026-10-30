# PUB-010: D1 row and text-value size

Checked against current primary documentation on 2026-10-30. Cloudflare's [D1 Limits](https://developers.cloudflare.com/d1/platform/limits/) page was last updated Oct 24, 2026.

## Documented limit

Cloudflare documents a maximum of **2,000,000 bytes (2 MB)** for a string, `BLOB`, or table row. The same page separately limits an SQL statement to 100,000 bytes, so a bound JSON text value is subject to the value/row cap rather than being counted as SQL source text. SQLite defines its length limit over string/BLOB/table-row bytes and returns `SQLITE_TOOBIG` when a string or BLOB exceeds the configured compile-time or run-time limit. [Cloudflare D1 Limits](https://developers.cloudflare.com/d1/platform/limits/), [SQLite limits](https://www.sqlite.org/limits.html#max_length), [SQLite result codes](https://www.sqlite.org/rescode.html#toobig)

## Receipt implication

A JSON-serialized receipt stored as one D1 text value that exceeds 2,000,000 bytes is over D1's documented maximum and the write should fail; it must not be treated as safely stored or silently truncated. Cloudflare does not document the exact D1 error string/status for this specific case, so depend on the size limit rather than an error-message match. The whole row has the same cap, so other stored columns also consume its allowance.

Any stricter receipt cap chosen by EezFunnels is an **application constraint**, not the Cloudflare limit. Enforce it before persistence using serialized byte length, leave room for the rest of the row, and surface a bounded-receipt outcome without relying on D1 to accept oversized JSON.
