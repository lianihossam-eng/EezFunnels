# Project skill workflow

This file routes the locally installed agent skills into the EezFunnels roadmap. The canonical task numbers, commit subjects, dependencies, and acceptance criteria remain in [the PR ledger](../plan/roadmap-pr-ledger.md). Product contracts, `AGENTS.md`, executable tests, and reviewed ADRs take precedence over skill advice.

## Sources and installation

The project-local installer is `bash scripts/install-project-skills.sh`. It pins and verifies these upstream sources:

| Source | Revision | License | Use in EezFunnels |
|:--|:--|:--|:--|
| [Matt Pocock skills](https://github.com/mattpocock/skills/tree/49dd158d1076134a641b33efb035946536778336) | `49dd158d1076134a641b33efb035946536778336` (1.3.1) | MIT | Research, domain modeling, codebase boundaries, bug diagnosis, prototypes, and agent-facing documentation. |
| [Cursor pstack](https://github.com/cursor/plugins/tree/1dcc2bacde73ddd284803b7af28164d0436198fc/pstack) | `1dcc2bacde73ddd284803b7af28164d0436198fc` (0.15.21) | MIT | TDD, evidence, idempotency, boundaries, types, and small verifiable units. Keep this existing pin; do not update the prior local stack automatically. |
| [Impeccable](https://github.com/pbakaus/impeccable/tree/8a0e198da34ebc5b936a9d5975fb7a3b1cf353b7) | `8a0e198da34ebc5b936a9d5975fb7a3b1cf353b7` (4.6.0) | Apache-2.0; retain its `NOTICE` | Optional critique of rendered web UI, microcopy, responsive states, and visual hierarchy. It does not inspect Rust/RSX source or prove behavior or accessibility. |

The installer adds missing selected links and verifies each pinned source revision, selected skill-directory target, and clean source worktree. Fresh installs check out the pinned commit even when upstream's default branch has advanced. Existing mismatched links and local edits fail closed and remain untouched. The installer preserves existing skill links, source checkouts, and `.project-skills/README.md`; it never changes global Codex/Cursor settings, installs hooks, adds application dependencies, or runs upstream commands. The previous 90 Matt/Pstack links remain in this checkout because they were already installed locally. The routing below identifies which skills to load for each task; it does not authorize unrelated Cursor orchestration, global settings, model routing, or automatic PR actions. Apply `principle-never-block-on-the-human` as an autonomous default: resolve routine implementation choices from the approved roadmap and current issue; reserve blockers for actual missing prerequisites, such as credentials needed for a remote operation.

Impeccable's pinned skill files are installed for local discovery, but its CLI downloads a separate engine whose binary does not yet have an independently pinned digest. Until that provenance is verified in a reviewed pilot, do not execute the CLI, install hooks, use `live`, call the remote `shape` service, or run `init`. No engine is needed to install or inspect the skill text. In a later UI task, an approved pilot may use it only as guidance against the rendered Dioxus page; Playwright, axe-core, keyboard checks, and assistive-technology checks remain the evidence.

The eight skills from the [Snyk UI/UX article](https://snyk.io/fr/articles/top-claude-skills-ui-ux-engineers/) were reviewed and are not added as another stack:

| Skill | Decision |
|:--|:--|
| Anthropic `frontend-design` | No separate install; overlaps the selected UI guidance and its source workflow is not a Dioxus implementation proof. |
| Vercel `web-design-guidelines` | Useful reference, but duplicates the selected guidance and would add another independently changing rule set. |
| Vercel `react-best-practices` | React/Next-specific; the dashboard is Rust/Dioxus. |
| Vercel `composition-patterns` | React component/API patterns do not map to the Rust/Dioxus component model. |
| UI UX Pro Max | Broad design catalogs overlap Impeccable and are not needed as another active skill stack. |
| Bencium `controlled-ux-designer` | Its question-heavy workflow conflicts with the project's autonomous delivery instructions. |
| AccessLint | Duplicates the existing Playwright/axe-core path and does not inspect the Dioxus Rust source. |
| Vercel `react-native-best-practices` | React Native-specific and unrelated to the web dashboard. |

Reference revisions reviewed: [Anthropic skills](https://github.com/anthropics/skills/tree/dbd4588f9e1033efb41dad4bef2f7947c8993d44) `dbd4588f9e1033efb41dad4bef2f7947c8993d44`; [Vercel agent-skills](https://github.com/vercel-labs/agent-skills/tree/063bee94c3f4df8453406c830b0a7df0f2860278) `063bee94c3f4df8453406c830b0a7df0f2860278`; [UI UX Pro Max](https://github.com/nextlevelbuilder/ui-ux-pro-max-skill/tree/56ea9cf174ed86786187daa9de0e6247cb0747e4) `56ea9cf174ed86786187daa9de0e6247cb0747e4`; [Bencium marketplace](https://github.com/Bencium/marketplace/tree/5de46a39b4640fa419560cd0b67e4c4b50dac802) `5de46a39b4640fa419560cd0b67e4c4b50dac802`; AccessLint skills `2e9d7336678302d0bc08848e92542560295b34d3`.

## Skill routing by task

Use only skills relevant to the current Task-ID. TDD and project validation apply to behavior changes; domain, boundary, and idempotency principles apply where their invariants are touched. Impeccable is optional and only useful for a UI surface after the engine gate above is resolved. No skill can establish a provider, Cloudflare, privacy, security, accessibility, migration, or production claim by itself.

| Task-ID | Skills to use | Why and limit |
|:--|:--|:--|
| PR-022 | `tdd`, `principle-prove-it-works`, `principle-experience-first` | Publication controls expose existing commands; CAS, replay, and public-route tests prove correctness. |
| PR-023 | `research`, `prototype`, `benchmark-checklist`, `domain-modeling`, `principle-type-system-discipline`, `no-comments`, `interrogate` | Compare bounded transfer models, model the immutable document contract, challenge cross-phase dependencies, and preserve distinct 16 KiB, 1 MiB, and MCP limits. |
| PR-024 | `architect`, `codebase-design`, `domain-modeling`, `tdd`; optional Impeccable only after the engine pilot | Build typed editing on the accepted V2 model; use a design critique only for an unresolved rendered interaction. |
| PR-025 | `domain-modeling`, `tdd`, `principle-prove-it-works`; optional Impeccable for the rendered picker | UI review can improve picker states; R2 digest, authorization, retention, and GC need direct tests. |
| PR-026 | `principle-experience-first`, `tdd`; optional Impeccable after the engine pilot | Refine themes and responsive states. Playwright/axe, keyboard, screen-reader, and contrast checks are the evidence. |
| PR-027 | `domain-modeling`, `codebase-design`, `tdd`; optional Impeccable after the engine pilot | Model pages and owner navigation together so later P3–P6 spaces remain discoverable. |
| PR-028 | `domain-modeling`, `tdd`, `principle-boundary-discipline`; optional Impeccable for form/error copy | Keep schema preview separate from collection, submission handling, and marketing consent. |
| PR-029 | `domain-modeling`, `principle-boundary-discipline`, `principle-prove-it-works`; optional Impeccable for the submissions inbox | A good inbox does not prove deletion, export, retention, or restore behavior. |
| PR-030 | `tdd`, `principle-make-operations-idempotent`, `principle-prove-it-works` | Prove visitor scope, durable capture, replay, and deletion gates; no UX skill can prove intake safety. |
| PR-031 | `research`, `domain-modeling`, `tdd`; optional Impeccable for settings/preview UI | Official Cloudflare and DNS evidence plus route tests prove host ownership and SEO behavior. |
| PR-032 | `research`, `domain-modeling`, `principle-boundary-discipline` | Decide event identity, retention, and consent behavior before enabling collection. |
| PR-033 | `benchmark-checklist`, `principle-make-operations-idempotent`, `principle-prove-it-works`; optional Impeccable for report readability | Metric definitions, denied/absent-consent behavior, deletion, and deterministic fixtures are independent of visual quality. |
| PR-034 | `domain-modeling`, `principle-boundary-discipline`, `tdd`; optional Impeccable for Contact/Opportunity screens | Keep submissions, contacts, opportunities, and platform accounts as distinct records. |
| PR-035 | `domain-modeling`, `tdd`, `principle-experience-first`; optional Impeccable for search/deep links | Verify query bounds, navigation context, and keyboard/mobile behavior. |
| PR-036 | `research`, `prototype`, `benchmark-checklist`, `principle-boundary-discipline` | Use dated official provider/runtime evidence; choose the email provider once here. |
| PR-037 | `domain-modeling`, `architect`, `tdd`; optional Impeccable for catalog/checkout preview | Review the V3 specification commit before product commits, then review the final exact PR head again. |
| PR-038 | `research`, `principle-make-operations-idempotent`, `principle-prove-it-works`; optional Impeccable for connection/rotation UI | Connection state and secret handling are distinct from verified signatures, replay protection, and inbox dedupe. |
| PR-039 | `domain-modeling`, `principle-make-operations-idempotent`, `tdd`; optional Impeccable for buyer checkout states | Browser return and provider handoff never prove payment; keep `unknown` honest. |
| PR-040 | `principle-make-operations-idempotent`, `principle-prove-it-works`, `principle-boundary-discipline`; optional Impeccable for order detail | Provider observations, exact-target confirmations, and reconciliation tests prove lifecycle behavior. |
| PR-041 | `domain-modeling`, `principle-experience-first`, `tdd`; optional Impeccable for the buyer upsell step | Make price and consent clear; buyer acceptance does not equal a paid order. |
| PR-042 | `research`, `principle-make-operations-idempotent`, `principle-boundary-discipline`; optional Impeccable for campaign review | Provider selection belongs to PR-036. Recheck consent and deletion immediately before dispatch; separate attribution IDs from email campaign IDs. |
| PR-043 | `domain-modeling`, `principle-make-operations-idempotent`, `principle-boundary-discipline`, `tdd`; optional Impeccable for rule/job states | Preview and activate typed rules explicitly; test a closed event/action/actor/replay matrix. |
| PR-044 | `research`, `principle-boundary-discipline`, `benchmark-checklist` | Bound provider egress, secret handling, usage, retention, and spend; no UX skill grants authority. |
| PR-045 | `domain-modeling`, `principle-type-system-discipline`, `tdd`; optional Impeccable for proposal diff/review | Proposals use shared commands; publication and external effects remain separate owner actions. |
| PR-046 | `research`, `domain-modeling`, `principle-prove-it-works`; optional Impeccable for evidence/uncertainty presentation | Link every summary to allowed source evidence and display uncertainty honestly. |
| PR-047 | `principle-boundary-discipline`, `principle-make-operations-idempotent`, `principle-prove-it-works`; optional Impeccable for confirmation/history | Fresh target-bound confirmation and audit tests prove authority; visual clarity does not. |
| PR-048 | `benchmark-checklist`, `create-verification-skill`, `principle-prove-it-works` | Add a recurring verification skill only when a real gap is found; use combined restore and external-effect journeys. |
| PR-049 | `research`, `domain-modeling`, `prototype`, `principle-boundary-discipline` | Compare identity and scope topology with real Worker/D1 evidence; keep this decision public and synthetic. |
| PR-050 | `domain-modeling`, `principle-boundary-discipline`, `principle-make-operations-idempotent` in the separate private overlay | Implement hosted identity only after PR-048 and PR-049; pin the exact public core and keep private code outside this repo. |
| PR-051 | `principle-boundary-discipline`, `principle-prove-it-works` in the separate private overlay; optional UI critique only in that repo | Deny-by-default admin scope and audit tests matter; public owner features remain unchanged. |
| PR-052 | `principle-prove-it-works`, `principle-test-behavior-not-implementation` in the separate private overlay | Prove distribution parity against one pinned public core; no auth/admin code enters the common repository. |

The per-task Conventional Commit subjects already in the ledger remain the implementation sequence. This skill map does not add commits to product tasks, reorder PR-022–PR-052, move identity earlier, or change the public/private boundary.

## End-to-end proof and GitHub workflow

Use the repo's existing sequence for every task: branch from the latest merged `main`; follow the exact branch and commit subjects in the ledger; include `Task-ID: PR-0XX` in each commit body; run `cargo xtask check` before each commit and `cargo xtask ci` before merge; update `plan/STATUS.md`; open `[PR-0XX] <scope>`; request a Sol review on the exact head and tag `@codex` for independent review or questions; resolve review threads and require green hosted checks before Sol merges. Never treat a Cursor playbook, local agent output, or skill score as a review or merge.

PR-048 must test the whole business path in addition to isolated features: provider accepts checkout then loses its response; buyer retries; buyer accepts or declines an upsell; only a verified callback marks paid; a refund and an already queued email are reconciled; restore applies deletion/consent state before dispatch; unknown external effects are not blindly retried. A second AI journey covers a stale proposal, rejection and later owner acceptance, separate publication, and no charge/send from proposal acceptance. Keep local integration proof distinct from authorized Cloudflare production proof.
