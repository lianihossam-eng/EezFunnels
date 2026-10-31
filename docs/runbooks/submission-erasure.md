# Submission export, erasure, and D1 recovery

This runbook covers the single-owner public edition. The owner API is expected to sit behind the installation's existing Cloudflare Access or equivalent perimeter. Responses from these routes are private and non-cacheable.

## Export or erase one submission

Export one retained submission as a versioned JSON attachment:

```http
GET /api/submissions/<submission-uuid>/export
```

The export contains the submitted field values, funnel/page/form/revision references, current follow-up status, submission and expiry times, and consent evidence captured with that request. It excludes the raw idempotency key, the value-derived request fingerprint, secrets, the full funnel document, other submissions, and private briefs. An absent, expired, or already-erased submission returns the same 404.

Erase one submission:

```http
DELETE /api/submissions/<submission-uuid>
Origin: https://<owner-host>
```

`204` means the private R2 recovery journal is durable and the targeted D1 row and erasure intent have been purged. Repeating the request remains successful while that journal is retained. A missing ID without a durable journal returns `404`; a successful response is never inferred from absence alone. A `503` is an uncertain result: retry the same request after storage recovers. When the D1 suppression intent was committed, the submission is already hidden from the inbox, status updates, exports, and completion reports; scheduled maintenance will retry the journal and purge steps. Until the journal is durable, the source row remains in D1 so the accepted journal-before-purge order is preserved. Do not claim that its personal values have been physically purged during a prolonged R2 outage.

The anti-replay marker contains only a SHA-256 digest of the funnel, form, and submission-key context plus an expiry. It does not use the field-value fingerprint. When a key is reused after an earlier submission expires, the single marker keeps the latest original expiry among erased submissions with that context; an older journal never shortens a newer marker. The R2 journal contains only the submission ID, the same context digest, and deletion/expiry times. The journal is retained until the later of the source expiry and deletion time plus the recovery horizon.

The horizon is controlled by `SUBMISSION_ERASURE_RECOVERY_HORIZON_DAYS`; it defaults to 30 days and accepts values from 1 through 3,650. Invalid configured values make erasure unavailable until corrected. No setting change extends an existing journal's deadline.

## Reconcile after restoring D1

The recovery guarantee applies only when D1 is restored while the original R2 bucket and its erasure journals remain intact, and reconciliation completes before submission reads or delivery resume. The Worker cannot detect an arbitrary D1 restore after the fact. Do not restore D1 first and close the gate later.

1. Close submission operations and start a recovery generation:

   ```http
   POST /api/submissions/recovery
   Origin: https://<owner-host>
   Content-Type: application/json

   {"action":"close"}
   ```

2. Before preparing, let submission requests admitted before the close finish or reach their bounded Worker invocation limit, and let any scheduled invocation already running at close finish. The recovery gate rejects new submission operations; later scheduled invocations only drain committed erasure intents. Do not prepare until the operator can confirm that no pre-close submission request remains in flight. Keep the owner recovery route available for the next step.

3. While the response phase is `preparing`, repeat the following request with the returned generation. Continue until the phase is `awaiting_restore` and `pending` is zero:

   ```http
   POST /api/submissions/recovery
   Origin: https://<owner-host>
   Content-Type: application/json

   {"action":"prepare","generation":"<generation-uuid>"}
   ```

   This drains pending erasure intents into durable R2 journals. Submission inbox, status, export, report, public form submission, and scheduled submission cleanup remain closed while recovery is in progress. Interaction-event cleanup and document-stage cleanup use separate data and prefixes. Do not proceed to D1 restoration if any request or scheduled operation could still create an erasure intent.

4. Restore the D1 snapshot while retaining the original R2 bucket. Apply the repository's compatible D1 migrations before reopening service. Keep the Worker behind the perimeter while the restore is in progress.

5. Start reconciliation only after D1 restoration is complete and a Worker version compatible with the restored schema and erasure gate is active. Review restored pending delivery tasks against the receiver's stable event-ID ledger, and arrange any required independent dispatch hold before reconciliation can reach its terminal page. Repeat this request with the same generation until the returned phase is `ready`:

   ```http
   POST /api/submissions/recovery
   Origin: https://<owner-host>
   Content-Type: application/json

   {"action":"reconcile","generation":"<generation-uuid>"}
   ```

   Each invocation processes one bounded R2 page. The Worker validates each versioned journal, checks any restored source's funnel/form/key digest and original expiry before deleting it, reapplies still-live replay markers, clears matching restored intents, and advances the R2 checkpoint only after the D1 page succeeds. The gate opens only after a complete terminal page and a final check finds no pending intent.

6. Verify `GET /api/submissions/recovery` reports `ready`. The final reconciliation opens the gate for submission reads and the scheduled delivery dispatcher immediately; Cloudflare Access does not pause Cron. Keep any independent ingress or dispatch hold until post-recovery checks pass, then restore normal traffic. A D1 restore can rewind task attempts after an external receiver accepted a request, so preserve receiver deduplication for stable event IDs. A journal that is listed but malformed, inconsistent, or unavailable keeps the gate closed, as does failed D1 work. The Worker has no independent journal inventory and cannot detect an object already missing from R2; the recovery guarantee therefore requires the original R2 bucket and all retained erasure journals to remain intact. Do not edit the R2 control object to force it open; preserve the restored database and investigate the failed journal or checkpoint first.

The initial R2 recovery control is created conditionally only when there are no erasure intents, replay markers, or journals. If the control object is missing while any erasure state exists, normal submission routes fail closed.

## Outbox and dispatch

The disabled-by-default outbox stores references to submissions and revisions rather than another copy of submitted values. Accepting a form creates a task only when the installation and form are enabled and the form's expected destination version matches the current installation destination. The source and task commit in one D1 batch. A new destination version does not retarget old tasks; each form must be explicitly revalidated. Direct source insertion during restore does not create a task.

When an erasure intent commits, the matching task is deleted in the same D1 batch as the intent and replay marker. The `retained_visible_public_submission_delivery` view also hides a task if its source is expired or has an erasure intent. Source purge cascades to its task. The R2 journal remains the recovery authority, and D1 task visibility does not prove the recovery gate is open.

The scheduled dispatcher requires the recovery-ready gate, acquires and retries only through the retained-visible source predicate, and rechecks the gate and source immediately before transport. An erasure cannot recall a request already sent to a receiver; a request in flight at deletion time remains outside the cancellation guarantee.
