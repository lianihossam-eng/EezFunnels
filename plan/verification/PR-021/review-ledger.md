# PR-021 local plan review ledger

Status: revised candidate after exact-candidate Pass 3, 2026-10-15. Four independent first-pass reviews, four cross-reviews, and four Pass 3 reports are complete. Pass 3 returned `CORRECTIONS_REQUIRED`; material and precision findings are integrated below. Targeted confirmation of the revised manifest and Sol's exact-head PR review remain pending. This ledger records document-review provenance and decisions; it is not product approval or production evidence.

## Product boundary and candidate decision

The user confirmed one complete public/common product for self-hosted and future private distributions. The funnel owner dashboard and all business features stay common. Only platform accounts/authentication and the private platform administration dashboard are reserved for Phase 8. CRM contacts, form submissions, buyers, and recipients remain business records, not platform users. Cloudflare Access is an optional self-host perimeter, not a chosen SaaS identity model. A typed, trusted `ResourceScope` seam may be designed as common data grows; no account, login, session UI, or admin implementation is planned before Phase 8.

The cross-review exposed one explicit disagreement about Phase 3:

- `global.md` recommended creating a minimal Lead 1:1 with each submission in P3 to retain the broader “lead” promise, with Contact identity/linkage in P4.
- `builder.md`, the final `journeys.md` recommendation, and `business-ai.md` favored a submission-only inbox in P3, with CRM identity in P4.

The integrated plan chooses the second model as a local roadmap decision, not as user/product validation: the dashboard calls the view “Leads (soumissions)” and presents each accepted `FormSubmission` as a prospect row, but does not create a separate Lead/Contact entity, merge by email, or infer a person. This gives the owner a usable intake inbox while deferring cross-submission identity, consent-aware matching, opportunity pipeline, and CRM actions to P4. D1 `FormSubmission` is the sole durable conversion source; `form.submission.accepted` is a versioned non-PII outbox signal carrying an opaque source ID. `page_view` and `form_start` are separate observations. Analytics conversion derives from the source row and its deduplicated projections, never a browser success beacon.

Other resolved cross-pass decisions are in the canonical [roadmap](../../product-roadmap.md) and [PR ledger](../../roadmap-pr-ledger.md): P2 publication controls → version/transport specification → typed editor → media → theme/a11y; P3 deletion-surviving privacy evidence before public capture, explicit route aliases, and verified domain/SEO; P4 event/analytics and explicit CRM identity; P5 separate payment/email provider choices before external effects; P6 AI as a typed proposal/ordinary-command client; P7 recovery and measured release evidence before P8. Phase 7 adds broad readiness work but every PR already has relevant security, privacy, accessibility, boundary, migration, and regression checks.

The public product core remains in the public repository and is version-pinned unchanged in future private distributions. Platform auth/users and private platform admin live in a separate overlay outside that repository, integrated only through stable trusted interfaces in P8. No environment flag adds private behavior to public artifacts; there is no private fork of common builder, CRM, analytics, commerce, email, automation, or AI behavior. This is an explicit delivery-boundary decision for the roadmap, not a claim that such an overlay exists today.

Version axes stay separate: C-01 semantic `schema_version`; internal snapshot `format_version`; render `manifest_version`; renderer/theme digest; form/event/attribution/offer/template/automation versions; actual D1 migration numbers. V2 is proposed for pages/forms/typed documents; V3 is proposed for commerce/offer references. `C-01`'s 1 MiB domain cap does not change the 16 KiB API/MCP command parameter cap. Existing C-03 command receipts remain 24 hours; durable business/event/effect IDs have their own retention. Current migrations 0001–0006 remain immutable. Actual future migration IDs remain unassigned.

No external effect is promised exactly once. Provider, email, worker, and restore ambiguity remains `unknown` until safe reconciliation. A D1 tombstone inside a backup does not protect a deletion newer than that backup; restore must pause dispatch and apply durable redaction/effect evidence before resuming. The durable source of that evidence and the Rust Worker/D1 queue runtime remain `NEEDS_PROTOTYPE`.

## Review passes and traceability

### Pass 1 — four independent workstreams

Completed 2026-10-15. Reviewers received independent work on builder, customer/CRM journeys, commerce/AI, and global architecture/product interactions against the repository and confirmed product boundary. Their source reports were in the shared review workspace; raw reports are not copied into the repository. SHA-256 values below preserve the exact report artifacts without publishing private model reasoning.

| Workstream | Report | SHA-256 | Findings carried into the candidate |
|:--|:--|:--|:--|
| Builder | `builder.md` | `1044e0fcb2575320bbb1a8090abf7d3de8102ca157faa8c00da507e70f5d1df1` | Dashboard publish controls are absent; structural section changes are reorder-only; inspector settings already edit content; API 16 KiB and C-01 1 MiB are separate; upload/staging and accessible recovery need explicit design. |
| Journeys/CRM/domains | `journeys.md` | `f3dbe4193e232d81db648023a51bc5a31188d93af96b9c6f0e75aadf17a6d779` | Submission identity differs from CRM identity; domain transfer needs new DNS proof and old-host/TLS/cache retirement; add follow-ups/segments/deep links; visits need an explicit signal; deletion crosses raw and aggregate records. |
| Commerce/AI | `business-ai.md` | `74b536ac01f555da8f4b77efa9eb67485f148920afda89b77c7a4f014b362748` | Keep trusted actors separate; do not expose visitor/provider/job commands through generic owner MCP; use durable intents/reconciliation and `unknown`; choose provider/secrets/runtime before effects; use one Session command path. |
| Global architecture | `global.md` | `008aabfea2725a8c7cc6db29cf3a81e55d8f0e370225a6ba1d56179e6ae041cc` | Full product remains common; accounts/admin last; trusted scope seam can precede auth; version/reference and migration interactions require explicit gates; cost claims need measured named workloads. This report's P3 Lead recommendation is the documented dissent above. |

### Pass 2 — cross-workstream adversarial confrontation

Completed 2026-10-15. The four reviewers compared each other's findings against the draft, challenged cross-phase contracts and combinations, and produced the following reports. Their IDs were provisional; the lead author assigned canonical PR-022 through PR-052 only after the functional scope was reconciled.

| Cross-review | Report | SHA-256 | Result carried forward |
|:--|:--|:--|:--|
| Builder interactions | `cross-builder.md` | `e8121958cd596b9648285f1bc8f9374bca7a47869887586f18fcd5f2526c75a5` | Schema/transport precedes editor; media follows typed image node; P3 form inbox precedes capture; no CRM identity in P3. |
| Journey interactions | `cross-journeys.md` | `0d92685fb7df4693512d31b5a7f82713b2d5a59c21da7507e9c4ba12c18f2af3` | Domain+SEO bounded; explicit conversion in P4; restore/tombstone and identity edges need explicit acceptance; visitors and observations stay distinct. |
| Commerce/AI interactions | `cross-business-ai.md` | `2c7db0b3a637eddc6f082bbc5d1fac780ea4f980977ef76f65192b19eb304da5` | Resolve provisional PR collisions; distinct V2/V3 schema; explicit command exposure, stable effect identity, provider runtime and restore gates. |
| Global interactions | `cross-global.md` | `eabac1470ce0bac37fd3c26178fa112c5df1f1b9400fdf7a39ad6082bfb58fd5` | Exposed the P3 Lead disagreement and scope seam timing; low-cost fixtures are not price claims; account/provider evidence remains open. |

The P3 issue is not represented as reviewer consensus: the candidate records the dissent, the chosen model, the UX naming, and the P4 conversion boundary. The final review is expected to challenge that choice without changing the user-confirmed platform account/admin boundary.

### Pass 3 — exact frozen candidate, completed with corrections required

All four Luna reviewers read the exact same frozen manifest `cfd4d9b3bb656c65d5e282c70e5c29ae98507d75a733c74bd8a5d8d03285f728` before and after review; their reports are outside the repository and their exact hashes are recorded here. The candidate files were not changed during Pass 3. Each report returned `CORRECTIONS_REQUIRED`. The independent reviews covered all eight phases, 28 phase pairs, six combined journeys, 32 Task-IDs/branches, 112 commit subjects, adapters, version/migration compatibility, tests/prototypes, proof status, and cross-pass findings. This does not approve the revised candidate below.

| Reviewer stream | Exact report | SHA-256 | Findings and disposition in revised candidate |
|:--|:--|:--|:--|
| Builder | `final-builder.md` | `13335f55e2584e1b4a04fb4361697015bec5a213253ce08515a3c1daab8396af` | F1 email provider/deliverability/runtime gate → PR-036/042. |
| Business/AI | `final-business-ai.md` | `a9d7c9cec96ac7d1b0d9e17b7f5abb709084b8f7121b280f516f89f60becb2f7` | F1 → PR-036/042; F2 deletion/outbox/projection/restore → PR-029/030/032/033/048; F3 cost fixtures/buckets → PR-048; F4 source-linked job detail and safe action → PR-043. |
| Journeys/CRM/domains | `final-journeys.md` | `de21a0fa5033d808889546365ceb6bfc5fbf324f002e9b2e2db8922936e150ed` | J1 fail-closed custom-host resolver/epoch → PR-031; J2 published route-rename behavior → PR-027; J3 single conversion source and deletion race merged with F2 → PR-029/030/032/033/048; J4 UTM allowlist/unknown attribution → PR-032/034; J5 automation detail merged with F4 → PR-043. |
| Global architecture | `final-global.md` | `ffecbd2fa3d24d4ef9fdf7c3a2fffbde1e9055c6dbf1033cf2de0c82beca6d12` | Frozen contracts remain untouched; domain transfer disables old mapping before new activation and versions cache binding; public capture blocks on restore-surviving deletion evidence; cache keys are future only; DAG parser covers slash references; upsell event semantics are explicit. |

The findings are integrated as planning rules, not as evidence that the future mechanisms work. Public capture remains disabled unless PR-029 demonstrates deletion-overlay survival across a restore from before deletion. Custom domain activation remains fail-closed through transfer, fresh DNS proof, provider-observed TLS readiness, and mapping epoch changes; no immediate edge purge or client-byte revocation is claimed. Published path rename produces a non-cacheable 307 alias to the current canonical path, with no route-registry loops/chains/collisions; unpublish removes public routes. `FormSubmission` is the only conversion source; outbox is an opaque signal, and projections deduplicate and consult deletion state. UTM attribution accepts only the five named keys with owner-registered campaign IDs, otherwise `unknown`. Email and payment providers have separate decisions; live email is gated on sender health and runtime/provider evidence. Cost fixtures and formulas are explicit but remain hypotheses, never price/capacity promises. Frozen contract files remain unedited; new semantics require separately versioned artifacts and ADRs with compatibility tests. Future private auth/admin are an external overlay pinned to the public core and appear only in P8.

One additional explicit event contract resolves the upsell journey: `upsell.decision.recorded` records accepted/declined buyer choice only; `order.paid` requires verified provider confirmation. These are separate from conversion and are consumed only through allowlisted event rules.

### Revised candidate M2 — exact confirmation complete, now superseded

All four Luna reviewers confirmed manifest M2 (`d0e9662ad2ae401dc83c53fccdefdf255bc95e792ba8c171509eb115ec9cb1fd`) as `PLANNING_READY`, with no remaining material blocker; their reports and hashes are recorded below. Sol's separate exact M2 pre-commit review found one additional material sequencing defect in PR-036/037 and one stale validator transcript. M2 is superseded by M3 below. This is a planning-review status only, not evidence that future runtime work is proven or `PRODUCTION_READY`.

Provenance note: while preparing Sol's minimal correction, I briefly edited the PR-036/037 ledger rows, execution transcript, and this review ledger, then restored them to M2 after root requested that the four M2 rechecks finish first. `sha256sum -c` then passed all 17 M2 entries. The reviewers' before/after hash checks and this restoration verify the endpoint bytes, not continuous immutability during the whole review window; do not describe M2 as continuously unchanged. No Rust source, contract, dependency, migration, or lockfile was touched.

| M2 confirmation stream | Exact report outside the repository | SHA-256 | Result |
|:--|:--|:--|:--|
| Global | `/workspace/shared/eez-roadmap-review/recheck-global.md` | `cc4cc1922c3047785b88760ef7a572c753b05254069e05b6085c2fe0c8f9f820` | `PLANNING_READY`; no remaining material blockers. |
| Builder | `/workspace/shared/eez-roadmap-review/recheck-builder.md` | `9ba05e985950c12759245b09f4a5344a600df09f8ad62221485e12b324c6fa3a` | `PLANNING_READY`; no remaining material blockers. |
| Business/AI | `/workspace/shared/eez-roadmap-review/recheck-business-ai.md` | `ff6169c0f5143a349ed09a19da2426799bf06b9642c05447c63ff78405bb2e6c` | `PLANNING_READY`; no remaining material blockers. |
| Journeys | `/workspace/shared/eez-roadmap-review/recheck-journeys.md` | `80fd74721b9fdfb58407bd32b4296dbb44b83e40a17e5a843a323dea19aef146` | `PLANNING_READY`; only a non-blocking custom-host root-path prototype question remains. |

### Revised candidate M3 — targeted delta confirmation pending

M3 applies Sol's M2 finding by putting the V3 spec-only commit first in PR-037 and requiring its review/acceptance before any implementation commit; PR-036's payment-provider decision remains a prerequisite. It also synchronizes the validator transcript and adds the journeys review's non-blocking root-path question to the PR-031 prototype gate. The ten-line validator output, two SQLite prototypes, JSON parse, whitespace check, and local repository CI have passed for the underlying documentation candidate; this M3 text and ledger delta need a fresh manifest and targeted confirmation. Its exact file digests are in [candidate-hashes.sha256](candidate-hashes.sha256), whose own SHA-256 is communicated with the frozen candidate. Do not call M3 `PLANNING_READY` until reviewers confirm the exact manifest; do not call it `PRODUCTION_READY` on the basis of document review or SQLite prototypes.

Sol's separate pre-commit review of the initial documentation diff found no material blocker and asked for the wording “structural section operations are reorder-only” because inspector content/settings already work. The roadmap wording was corrected. This preliminary review predates the Pass 3 corrections and is not the pending exact-head PR review or merge.

## Prototype evidence and limits

| Artifact | Exact command | Result | Limit |
|:--|:--|:--|:--|
| [Scope probe](prototypes/scope_probe.py) | `python3 plan/verification/PR-021/prototypes/scope_probe.py` | 8 SQLite checks pass; one deliberately unscoped mutation changes rows for both owners. Exact output is [scope_probe_execution.txt](prototypes/scope_probe_execution.txt). | SQLite only; no D1, Worker, R2/KV, runtime auth, migration, restore, latency, or cost proof. |
| [Commerce model](prototypes/commerce-prototype.py) | `python3 plan/verification/PR-021/prototypes/commerce-prototype.py` | Local model covers atomic order/receipt/outbox commit, replay, provider-event duplicate/stale state, ambiguous acceptance as `unknown`, and reconciliation. Exact output is [commerce_prototype_execution.txt](prototypes/commerce_prototype_execution.txt). | SQLite only; no D1 transaction, Rust Worker, Queue/Workflow, real provider, signature, pricing, or production proof. |

These prototypes support counterexample design only. They do not establish account isolation, a durable cloud job runtime, payment safety, deletion-safe restore, cost, or `PRODUCTION_READY`.

## Repository and external proof status

The starting source state was `main` at `3d46ecaf4fc6dd0411ae047841b1001f1ab1f8e6`, with no pre-existing user edits. GitHub PR #21 and the v0.1.0 release are recorded in [STATUS](../../STATUS.md), including hosted runs `38676533684`, `38676533683`, and `38677096705`, and the three platform binaries plus `SHA256SUMS`. That proves published release assets only. Cloudflare production bindings/deploy and Claude Desktop interoperability remain `UNVERIFIED`; no remote Cloudflare operation was performed.

The requested DeepSeek share remains historical. The available share path reached a sign-in redirect, the separate authenticated UI tab was not interactable from this execution, and zero prompts were sent. The user then selected local adversarial iteration. There is no external response, review, or approval in this packet, and DeepSeek is not a gate for the user-authorized local workflow.

## Candidate artifact digests

The authoritative current candidate file digests are stored in [candidate-hashes.sha256](candidate-hashes.sha256). Each manifest covers the complete plan, modified project status/readme/architecture/history notice, this review ledger, and verification scripts/logs present at that freeze. Pass 3 reviewed its own earlier manifest; M2's targeted confirmations and supersession are recorded above. For any material correction, generate a new manifest and request confirmation against its exact bytes.
