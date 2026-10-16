# D22 - V2 document and bounded transfer

Status: proposed for approval in PR-023. Persist complete immutable snapshots.
The local Node benchmark meets typed-patch byte-savings and pooled-size p95
gates, but fails its stricter per-fixture p95 gate in 5 of 32 V2 fixtures. It
does not exercise the Worker/D1/R2 runtime. D22 selects the complete-snapshot
wire for initial V2 runtime use. No V2 reader, stage route, migration, or
Cloudflare readiness is claimed.

## Context

The V1 funnel snapshot is a private `format_version` 1/2 envelope with
`schema_version` 1. Format 2 means theme overrides, not a multipage schema.
Frozen C-01 requires an explicit structural schema version and frozen
C-01_config lists only V1. The current command cap is 16 KiB, the current V1
snapshot ceiling is 1 MiB of UTF-8 `data_json`, and MCP has separate 16 KiB
parameter and 256 KiB response caps. Future page/form content therefore needs
a new structural schema and a bounded byte transport without changing V1.

The V2 structural schema and manifest are proposed in
[C-05](../contracts/C-05_funnel_document_v2.md). The relevant official source
research and limits are recorded in
[PR-023 research](../verification/PR-023/research.md). The benchmark
distinguishes transfer bytes from persisted document bytes and cannot
establish Cloudflare production behavior.

## Decisions

1. **Keep version domains distinct.** V1 uses its exact existing row/schema and
   `format_version` dispatch. V2 introduces `document_type` plus
   `document_version = 2`, structural `schema_version = 2`, and a separate
   `manifest_version = 2`; all row/document/manifest values must agree. Missing
   versions are errors. C-01 and C-01_config stay frozen. C-05 is the sole new
   contract supplement authorizing the V2 structural schema; its merge after
   exact-head review and required checks is the design approval. PR-024 must
   implement the approved C-05 decoder and add V2 to the runtime registry
   explicitly before any V2 save or public render. Until then, runtime support
   remains V1-only.
2. **Store canonical full V2 snapshots.** Every committed V2 revision is an
   immutable complete document in canonical UTF-8 JSON. The revision row
   stores its document and render manifest together. No accepted patch-chain
   representation exists in D1, and historical reads/restores never replay
   edits. The local transport prototype compares typed patches, but the initial
   V2 runtime accepts full snapshots only. Any later decision to accept patches
   must expand one against its exact base and persist the same complete snapshot.
3. **Stage large document bytes separately from commands.** The JSON command
   surface stays capped at 16 KiB. `funnel.document.stage.v2` is an
   idempotent owner mutation that creates stage metadata and returns its opaque
   stage ID; the stage row and mutation receipt are committed together. The
   stage ID is 32 random bytes encoded as 64 lowercase hex characters. The
   stage row stores its hash for lookup; the 24-hour mutation receipt retains
   the exact original result (`stage_id` and original `expires_at`), is
   available only through the owner command path, and is redacted from logs.
   An exact mint replay returns that stored result unchanged throughout the
   receipt window, even if the stage is now expired or consumed; it does not
   report mutable stage status. Every new upload and every new save/finalization
   checks current stage state, so a terminal stage ID cannot be used again. An
   exact active save receipt is intentionally replayed before stage lookup, even
   after the stage is terminal or its object has been deleted. After expiry,
   consumption, or a stale-editor-head conflict, the client re-reads the editor
   head and mints a new stage with a new idempotency key; a stage bound to a
   stale base head is discarded and allowed to expire and clean up. Same key
   with changed input remains an idempotency mismatch.

   Bound staging admission atomically in D1: an installation may have at most
   ten outstanding stage slots, and a funnel may have at most one nonterminal
   stage. The slot is reserved in the same mutation that creates the stage and
   mint receipt, and is released only after the stage is terminal and its D1
   metadata and referenced temporary object are cleaned. If capacity is full,
   mint returns the existing `rate_limit/exceeded` error without creating a
   stage or receipt. Concurrent mint requests must contend on the same D1 capacity
   state; a preflight count outside the mutation is not sufficient. This bounds
   successfully staged bytes referenced by live stages to at most 10 MiB per
   installation. It does not bound retry-generated orphans or unresolved R2
   writes; the attempt-row quota below also does not provide that bound.

   The stage ID is a 256-bit bearer capability, not an owner credential. A
   separate binary HTTPS `PUT` sends it as `Authorization: Bearer <stage_id>`
   and writes only temporary bytes to a server-generated private R2 key. The
   final `funnel.revision.save.v2` JSON command includes the same sensitive
   value in its required `stage_id` parameter so the receipt payload can bind
   it. Neither route accepts the token in a URL; logs and errors redact it.
   Stage mint, upload, and save remain separate: only the validated save can
   create a revision or advance the editor head, and none can publish.
4. **Use two digest domains.** A stage binds an exact raw-upload SHA-256 and
   byte length. The finalizer recomputes it, rejects duplicate JSON keys,
   decodes the closed V2 document, validates all scope, schema, IDs, edges, and
   limits, canonicalizes it with RFC 8785, and recomputes the stored document
   SHA-256. The initial V2 runtime does not accept a patch payload. If a later
   decision accepts patches, expand them against their exact base before these
   same checks and persist the full canonical document. V1 hashes are untouched.
   The V2 manifest has no self-hash.
5. **Reuse editor-head CAS and idempotency, separately from publication.** The
   final save receipt payload binds command ID, funnel ID, stage ID and digest,
   expected editor-head revision, document/manifest versions, and declared
   upload length. At the command boundary, an exact active save receipt is
   classified before stage lookup, R2 reads, expiry, or cleanup, following
   D15's receipt-first rule; a lost response therefore replays after stage
   deletion. Same key with changed command or payload returns the existing
   idempotency mismatch. A new save validates the
   complete document before one D1 batch claims the receipt, confirms that the
   stage is still `uploaded` and unconsumed with `expires_at > database_now`,
   confirms the exact editor-head CAS, checks revision/row limits, inserts the
   full immutable revision, advances only the editor head, updates the funnel
   timestamp, and marks the stage consumed. The expiry and state checks are in
   this final predicate, not only in the earlier validation. Competing distinct-key
   saves from one head have at most one winner and create no receipt or revision
   for a stale loser, following D15. If a different request claims the same
   idempotency key during
   validation, the finalizer rechecks the active receipt after the batch and
   returns its matching replay or payload-mismatch result. The published
   pointer remains a different CAS axis.
6. **Make the stage a narrow bearer capability.** Stage metadata binds the
   installation (the D1/R2 installation that created the record), funnel,
   exact base editor head, document kind and version, manifest version,
   purpose, maximum size, expected raw length, expected SHA-256, expiry, and
   upload-attempt state. The capability permits only those exact bytes. The
   route counts streamed bytes while reading and enforces the 1 MiB cap; it
   does not accept ranges, arbitrary paths, public object access, or
   user-selected R2 keys. A completed same-byte retry returns the existing
   uploaded stage without another R2 write. Different bytes are rejected and
   require a newly minted stage. Finalization re-reads and independently
   verifies the object and stage row.

   Bound D1 metadata admission separately: at most ten attempt-row quota slots
   may be held per installation across `receiving`, `abandoned`, and
   completed-but-not-cleaned attempts. Reserve a slot atomically with the
   `receiving` D1 row before issuing `PUT`; return the stable
   `rate_limit/exceeded` error without touching R2 when full. Release a slot
   after a settled writer and successful cleanup, or when the fixed tombstone
   retention below expires. Each attempt can write at most 1 MiB under a
   globally unique key. These slots bound simultaneously tracked D1 attempt
   rows; they do not establish a strict cap on unresolved provider-side R2
   operations or aggregate orphan bytes after a tombstone expires. Because
   Cloudflare does not document a fixed cancellation or settlement guarantee
   for an interrupted `put()`, an unresolved writer may outlive its D1 row and
   overlap a later admitted attempt. The scheduled prefix sweep below is a
   repeatable discovery and deletion path, not a provider-side concurrency or
   storage-volume bound.

   Every R2 `PUT` has a fresh `attempt_id` and object key. Before writing R2,
   the Worker creates a D1 `receiving` attempt row under a live-stage predicate
   requiring `state = issued` and `expires_at > database_now`, with a hard
   two-minute upload deadline. The handler awaits the R2 operation;
   a timeout racing an un-cancelled write is not evidence that its writer has
   stopped. Only a successful D1 compare-and-set from `receiving` to `complete`
   before the attempt deadline, while the stage is still `issued` and
   `expires_at > database_now`, may combine with the stage transition
   `issued -> uploaded` and publish the object key in stage metadata. If two
   same-byte attempts race, one wins that compare-and-set; the loser becomes
   `abandoned`, cannot replace the winner, and its unique key is eligible for
   cleanup. If R2 writing is interrupted or completion loses while the stage is
   still live and no attempt won, that attempt is abandoned and the stage stays
   `issued`; finalization is denied and a retry uses a fresh attempt key. A
   failed attempt never resets or invalidates a winning `uploaded` stage, and
   an expired stage stays terminal `expired`.
7. **Bound expiration and cleanup.** Stage upload expires 15 minutes after
   mint. D1 state transitions, not a fresh-reference scan alone, protect R2
   object keys. Cleanup may transition a `receiving` attempt to `abandoned`
   only after its two-minute upload deadline plus a two-minute grace period;
   the conditional update must win before deletion is queued. An upload
   completing after that transition cannot change the attempt or stage, so its
   key can never become referenced. It may still create or recreate an orphan
   after an earlier delete. If the writer is confirmed quiescent, cleanup may
   delete the orphan immediately; a timeout alone is not evidence of
   quiescence. Otherwise retain the D1 tombstone for 24 hours after abandonment,
   then retire that row and release its attempt slot. A late write can no
   longer be referenced: every attempt key is globally unique, never reused,
   and the abandoned attempt has no transition that can publish its key into
   stage metadata. This finite fencing rule replaces indefinite per-attempt
   tombstone retention.

   A scheduled R2 stage-prefix sweeper reconciles temporary objects after
   tombstone retirement. Allocate each attempt a monotonically increasing
   installation sequence atomically with its D1 row, and put that sequence as
   fixed-width decimal text before the unique attempt ID in the R2 key under
   `private/stage/`. At the start of each sweep pass, read and persist the
   highest allocated sequence as a fixed high-water mark. Paginate in key order
   only through that sequence; attempts admitted later have a higher sequence
   and cannot extend the in-progress pass. Bound each Cron invocation, persist
   its R2 cursor, and resume it on the next invocation. Stop a pass as soon as
   listing reaches a key whose sequence exceeds its high-water mark, or when
   the current listing reaches its end; then start a new pass from the
   beginning with a newly captured high-water mark. Never retire an old
   sequence range from future passes: a late writer may create an
   object behind the saved cursor after a pass has already visited that key.
   Its finite attempt key is eligible again on the next pass, regardless of
   newer admissions. Retry a failed list/delete page without advancing past
   unprocessed keys. The sequence must never regress or be reused within the
   same R2 namespace; a D1 restore must preserve its high-water counter or
   rotate and register a fresh namespace before V2 staging resumes.

   Each pass lists only `private/stage/`, uses the provider `uploaded`
   timestamp, excludes any live D1 stage reference, and deletes unreferenced
   objects at least 24 hours old. A late write that completes after an earlier
   pass has a new upload timestamp and is removed in a later pass. The sweep
   cursor, fixed high-water mark, failure retry, namespace restore rule, and
   late insertion behind the cursor are required PR-024 behaviors; local
   Wrangler tests cannot establish production Cron or R2 scheduling. D1
   tombstone retirement never depends on a post-quiescence absence check.
   No object key is written to R2 before its attempt row exists.

   Stage expiration first changes the stage to terminal `expired` in D1 and
   denies new attempt completion or finalization; an already-running PUT may
   still leave an orphan. The scheduled prefix sweep finds such objects after
   the per-attempt tombstone is removed. Successful revisions contain their
   full bytes and never
   reference staging objects. Finalization reads and
   verifies the bounded object before its D1 batch. If cleanup wins the stage
   predicate, no revision is committed; if finalization wins before expiry, it
   consumes the stage and the immutable revision is independent of later object
   deletion.
   A lost finalize response uses the 24-hour mutation receipt before stage
   access. D1 and R2 are never described as one atomic transaction.
8. **State the trust boundary honestly.** The current product has an implicit
   single installation owner and no hosted app-user authentication. Cloudflare
   Access is an optional self-hosted perimeter. A stage token attenuates an
   already-authorized operation; it is not identity, authentication, or
   tenant isolation. Anyone able to invoke the owner surface may mint a stage.
   Hosted identity remains in the final private-overlay phase.
9. **Keep MCP bounded.** MCP may expose only commands and projections that fit
   D21's limits. Large raw uploads, chunked reads, and pagination remain
   `NEEDS_PROTOTYPE`; no response is truncated and no binary Worker route is
   assumed to be an MCP transport. Because D21 dynamically exposes registered
   commands through `list_commands` and `execute_command`, PR-024 must exclude
   the V2 stage and save commands from both MCP discovery and invocation until
   a complete bounded MCP transfer path and a separately named, versioned
   D21 addendum are reviewed and approved. Frozen D21 remains unchanged. The UI
   and CLI can use a local file/stream and the binary route
   once PR-024 implements it.

## Exact byte limits

| Boundary | Limit | Counts |
|:--|--:|:--|
| JSON command request | 16,384 bytes | Existing API request contract; command JSON only. |
| Stage raw upload | 1,048,576 bytes | Exact HTTP body octets, before parsing. |
| V2 persisted document | 1,048,576 bytes | UTF-8 canonical `data_json`; manifest excluded. |
| V2 render manifest | 16,384 bytes | UTF-8 serialized manifest. |
| Encoded revision row | 1,100,000 bytes | Document, manifest, IDs, timestamps, and all stored columns. |
| D1 provider ceiling | 2,000,000 bytes | Vendor maximum for a row/value, not an application target. |
| MCP parameters / response | 16,384 / 262,144 bytes | Existing independent D21 limits. |

Full capacity at the frozen theoretical maximum is not promised. One hundred
funnels times one hundred 1 MiB retained snapshots is 10,485,760,000 bytes of
document data before manifests, indexes, receipts, and other tables. This
exceeds decimal 10 GB (10,000,000,000 bytes) by 485,760,000 bytes and is below
binary 10 GiB (10,737,418,240 bytes) by 251,658,240 bytes; Cloudflare's D1
documentation says 10 GB without defining a binary interpretation. PR-023
does not silently reduce the per-document, funnel, or revision limits. An
aggregate storage budget, user-visible quota error, and retention/restore
policy must be decided before making a full-capacity promise; immutable or
published revisions are never silently pruned.

## Prototype decision record

The prototype evaluates **wire encoding only**. Both candidates persist the
same full immutable snapshot; no patch chain is permitted by this decision.
Candidate A uploads the complete canonical document. Candidate B uploads this
closed patch record:

```json
{
  "patch_type": "eezfunnel.document-patch",
  "patch_version": 1,
  "base_document_sha256": "<64 lowercase hex characters>",
  "operations": [
    {"op": "replace_text", "block_id": "<stable ID>", "value": "<plain text>"}
  ]
}
```

The prototype permits only `replace_text` on existing `text` blocks, one
operation per block ID, in ASCII block-ID order. IDs, variants,
references, and fixture limits are checked after expansion against the exact
base-document digest. Insert/delete/reorder/theme/media/form/action changes
are outside this tested candidate. It is eligible only for edits fully
represented by that grammar, with strictly fewer encoded bytes than the
complete snapshot and no more than the 1,048,576-byte upload cap. The benchmark
requires matching output, at least 25% median upload-byte reduction for both
edit families, and no more than 2x local p95 transform time in every V2
page/size/edit fixture. A p95 pooled by document size is descriptive only. This
is not a general structural patch protocol. Both candidates use the same
modeled stage/save command count; patch expansion adds one modeled base-revision
D1 read. These are model counts, not observed provider operations.

The harness checks the legacy V1 one-page snapshot envelopes at
`format_version: 1` and `format_version: 2` with `schema_version: 1`, at exactly
16,384, 131,072, 524,288, and 1,048,575 bytes. These eight compatibility
fixtures verify accepted shape and byte-preserving parse/serialize behavior;
the V2 typed-patch candidate does not apply to them. V2 fixtures cover 1/10/25/50
pages at the same exact canonical UTF-8 sizes. For each V2 page-count and size
pair the benchmark tests a one-block sparse edit and a broad edit affecting 60%
of text blocks, with unchanged-length replacement text so the resulting
document has the exact target size. Five unreported warm-ups per candidate
precede 100 paired samples per V2 fixture. Each fixture has exactly 50
snapshot-first and 50 patch-first measured pairs, deterministically shuffled.
The harness must fail on any rejected valid fixture, output mismatch, oversized
upload, lost operation, or uncounted error; explicit negative fixtures pass
only when rejected. No failed sample is omitted. It reports wire bytes, Node
parse/expand/validate/canonicalize/hash p50/p95/max, per-fixture paired spread,
three modeled client round trips per save, and modeled D1/R2 operation/byte
counts.
Boundary tests also reject 1,048,577-byte upload and canonical documents and
accept valid 1,048,575- and 1,048,576-byte documents.

The paired fixture verdict and raw measurements are recorded in
[PR-023 results](../verification/PR-023/results.json) and
[execution notes](../verification/PR-023/execution.txt). Eight legacy V1-shaped
fixture checks passed shape validation and Node JSON parse/serialize byte
round trips; they do not exercise the Rust V1 decoder. The 32 paired V2
fixtures completed 6,400 candidate executions with exact output/digest parity
and zero errors. The nine explicit negative fixtures and size-boundary checks
passed. Median upload-byte reduction was 98.38896% for sparse edits and
39.93208% for broad edits. Pooled typed-patch/snapshot p95 transform ratios by
document size were 1.7545, 1.8407, 1.6091, and 1.9287 at 16,384, 131,072,
524,288, and 1,048,575 bytes, respectively. However, 5 of 32 individual V2
fixture p95 ratios exceeded 2x; the maximum was 2.6277x for the 1-page,
524,288-byte sparse fixture. The typed-patch candidate therefore fails the
per-fixture latency gate despite passing byte-savings and pooled-size p95.
These are local Node fixture results, not two production-equivalent
implementations under the Worker/D1/R2 runtime. D22 selects complete-snapshot
transfer for every edit in the initial V2 runtime. The typed-patch candidate is
not an accepted request format and has no runtime fallback path. Every saved
revision remains a complete canonical
snapshot. PR-024 must verify snapshot upload, finalization, and direct
save/render parity in local Worker/D1 before enabling V2. Reopen the patch
candidate only through a later decision backed by runtime-equivalent
correctness, latency, memory, and provider-operation evidence.

## Consequences

- V1 rows and V1 hash semantics remain unchanged; V2 is an explicit future
  reader and does not fall through from missing version fields.
- Full-snapshot transfer and persistence keep render, history, and restore
  direct. Typed patches are not selected for initial V2 runtime use, and no
  persisted patch chain exists.
- Stage/R2 support requires a future migration and local Worker/D1/R2 race
  tests; this ADR alone proves no runtime or security property.
- The C-01 maximum-per-row settings cannot be advertised as a simultaneous
  workspace capacity until aggregate quota and exhaustion behavior are
  separately specified.
