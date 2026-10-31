# Public edition improvement series

The public tracking issue is [#82](https://github.com/lianihossam-eng/EezFunnels/issues/82). Its checklist is the single source of truth for the status and PR links of Task IDs PUB-001 through PUB-021. This page defines the delivery gates and required evidence.

The series starts from EezFunnels commit 65feaeee and the funnel-hacking documentation corpus at f4fc6ea3. Each PR is based on the newest green main and leaves a reviewable state before the next PR begins.

| Gate | Task IDs | Evidence before proceeding |
| --- | --- | --- |
| Visitor journey | PUB-001–PUB-006 | Create and publish a service funnel, correct a form error without losing permitted values, and store one accepted submission. |
| Usable request | PUB-007–PUB-011 | Export and erase a request, recover from webhook failure, verify stable retry identity with a deduplicating local receiver, and record owner follow-up. |
| Honest reporting | PUB-012–PUB-014 | Respect measurement consent and retention, show report coverage, and calculate outcomes only over eligible retained submissions. |
| Page composition | PUB-015–PUB-019 | Build the three service recipes with accessible controls while preserving old revisions and local drafts. |
| Portability and release preparation | PUB-020–PUB-021 | Round-trip a template with available media, verify the application recovery gate with restored D1 rows and intact R2 journals, document the future pilot/rollback boundary, and leave current CI green. |

A gate passes only when its behavior and compatibility checks are recorded in the linked PRs. Closing issue 82 requires all 21 PRs merged, the final main validated, and local verification evidence published in the issue. The series prepares a future remote pilot; it does not deploy, publish a release, or contact a real webhook receiver.

## Verification boundary

The final evidence is local `cargo xtask ci`, hosted repository checks, browser smokes, and targeted integration scripts on the merged commits. The public-form recovery smoke restores erased submission rows and delivery tasks in local D1 while retaining the local R2 journal, then verifies that reconciliation removes suppressed rows and opens the gate. This exercises EezFunnels' recovery protocol; it does not execute Cloudflare D1 Time Travel or prove account-specific backup retention. The [future pilot readiness guide](runbooks/public-pilot-readiness.md) keeps those operator checks explicit.
