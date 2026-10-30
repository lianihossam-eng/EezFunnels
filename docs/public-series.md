# Public edition improvement series

The public tracking issue is [#82](https://github.com/lianihossam-eng/EezFunnels/issues/82). Its checklist is the single source of truth for the status and PR links of Task IDs PUB-001 through PUB-021. This page defines the delivery gates and required evidence.

The series starts from EezFunnels commit 65feaeee and the funnel-hacking documentation corpus at f4fc6ea3. Each PR is based on the newest green main and leaves a reviewable state before the next PR begins.

| Gate | Task IDs | Evidence before proceeding |
| --- | --- | --- |
| Visitor journey | PUB-001–PUB-006 | Create and publish a service funnel, correct a form error without losing permitted values, and store one accepted submission. |
| Usable request | PUB-007–PUB-011 | Export and erase a request, recover from webhook failure, verify stable retry identity with a deduplicating local receiver, and record owner follow-up. |
| Honest reporting | PUB-012–PUB-014 | Respect measurement consent and retention, show report coverage, and calculate outcomes only over eligible retained submissions. |
| Page composition | PUB-015–PUB-019 | Build the three service recipes with accessible controls while preserving old revisions and local drafts. |
| Portability and release preparation | PUB-020–PUB-021 | Round-trip a template with available media, document bounded restoration, and leave current CI green. |

A gate passes only when its behavior and compatibility checks are recorded in the linked PRs. Closing issue 82 requires all 21 PRs merged, the final main validated, and local verification evidence published in the issue. The series prepares a future remote pilot; it does not deploy, publish a release, or contact a real webhook receiver.
