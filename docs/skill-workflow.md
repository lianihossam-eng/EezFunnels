# Agent skill workflow

This repository uses project-local skills selected by [scripts/install-project-skills.sh](../scripts/install-project-skills.sh). The installer pins these sources:

| Source | Pinned revision |
| --- | --- |
| Matt Pocock skills | 49dd158d1076134a641b33efb035946536778336 |
| pstack skills | 1dcc2bacde73ddd284803b7af28164d0436198fc |
| Impeccable | 8a0e198da34ebc5b936a9d5975fb7a3b1cf353b7 |

Run the installer to materialize the selected skills for local work. The installer is authoritative if a future change updates a pin. Skills guide the work; the repository's [AGENTS.md](../AGENTS.md), [CONTRIBUTING.md](../CONTRIBUTING.md), [architecture guide](architecture.md), tests, and [pull request template](../.github/PULL_REQUEST_TEMPLATE.md) define the project workflow. Use the tools available in the active environment; Cursor-only runners, models, and hooks are not part of the required flow.

## Skill routing

Every PR uses the common review and evidence profile, then adds only the profiles its changes require.

| Profile | Skills |
| --- | --- |
| Common | principle-test-behavior-not-implementation, principle-prove-it-works, no-comments, interrogate, pr |
| Architecture | architect, codebase-design, principle-boundary-discipline, principle-sequence-verifiable-units |
| Domain | domain-modeling, principle-model-the-domain, principle-type-system-discipline |
| Interface | principle-experience-first and Impeccable design guidance |
| Retries and concurrency | principle-make-operations-idempotent |
| Bug correction | diagnosing-bugs and tdd when a local regression is directly testable |
| Cloudflare and integrations | research with current official sources |
| Instructions and documentation | writing-for-agents |
| Measured performance claims | benchmark-checklist |
| Final series review | reflect |

Load the task-relevant skills from the pinned local source and keep their scope aligned with the PR. Impeccable is a design guide for this series; it does not require an external design engine or provenance workflow.

## PR sequence

1. Read AGENTS.md, CONTRIBUTING.md, and the relevant product and architecture contracts. Identify the Task ID and latest green main, then create its docs/public-XXX, fix/public-XXX, or feat/public-XXX branch from that main.
2. Load the common profile and each applicable profile from the table above. For code changes, use behavior-focused tests and add a regression test for each bug fix; fuzz parsers that accept external input.
3. Keep each commit to one logical change, use a conventional subject, include Task-ID: PUB-XXX in the commit body, and run cargo xtask check before every commit. Do not commit a red state.
4. Use the repository PR template. Explain the problem, behavior changed, scope, compatibility and migrations, verification, and Task ID. Link issue 82.
5. Run cargo xtask ci and the required hosted checks on the final PR head. Review the scoped diff with no-comments and interrogate, then request the GitHub @codex review on that head.
6. Resolve relevant findings. After each change, rerun required checks and renew the latest-head review. Merge only after checks and review are conclusive; verify the merged main before starting the next PR.
7. Attach each created PR to the project chat. Keep a PR open if a required check or review is unavailable, and report the blocker.

The repository PR template takes precedence over generic templates supplied by skills. cargo xtask check, cargo xtask ci, and GitHub checks do not establish that a remote Cloudflare production deployment is ready.
