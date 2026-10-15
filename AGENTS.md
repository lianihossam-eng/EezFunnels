# Eez Funnel - Agent Operating Contract

Read this before doing anything.

## Start here

Every session reads, in order:

1. plan/STATUS.md
2. plan/architecture.md
3. This file

## Project in one line

Self-hosted single-user funnel builder on Cloudflare Workers, D1, R2, KV.

## Non-negotiables

### Never Crash

Crash loses user work. Outranks feature work.

Forbidden on non-test paths:
- unwrap(), expect()
- panic!, unreachable!, todo!, unimplemented!
- unsafe

Inputs are hostile: get() not [i], checked arithmetic, bounded recursion.

Every crash fix ships with a synthetic regression test.

No untrusted input can produce an uncontrolled panic on a production path.

### Clean-room

Never read or copy code from Shopify, ClickFunnels, Systeme.io, GoHighLevel,
Kartra, or any other funnel builder. No GPL/AGPL/LGPL.

### Command-based

Every user-visible action is a command with a stable ID and JSON parameters.
UI, CLI, MCP dispatch the same commands through Session::execute.

### Layering

L0: domain    (types, contracts, invariants)
L1: render    (HTML generation, sandboxed)
L2: storage   (D1, R2, KV bindings)
L3: commands  (dispatcher, policies)
L4: api, dashboard, cli

Enforced by cargo xtask layers.

## Workflow

### Before writing code

- Read `docs/skill-workflow.md` and load only the skills relevant to the Task-ID.
- Use `architect` and `codebase-design` for cross-boundary changes; use `domain-modeling` when changing domain invariants.
- Use the documented bug-diagnosis method for regressions. Do not assume Cursor-only playbooks or runners are available.

### While writing code

- `tdd`: failing behavior test first.
- Atomic commits. Conventional messages. Task-ID: PR-XXX.

### Before commit

- cargo xtask check must pass (fmt, clippy -D warnings, tests, layers).
- Apply `no-comments` and `interrogate` as local diff checks; do not depend on Cursor tools or block on user questions.

### Before merge

- PR-000 bootstrap gate requires `cargo xtask check`, the dashboard WASM build, a public GitHub
  repository, and green hosted CI on the current PR head. PR-000 does not require `cargo xtask ci`;
  PR-001 introduces it.
- Starting with PR-001, `cargo xtask ci` (check, WASM, and `cargo audit`) must pass before merging
  PR-001 and every later PR.
- Hosted checks must pass on the exact PR head. Request Sol's exact-head review and tag `@codex`
  for independent review or questions; resolve review threads before Sol merges.
- Update plan/STATUS.md.

## Escalation

Stop and open blocked/agent-stop if:
1. A contract is violated by a reproducible counter-example.
2. External dependency unavailable > 1 hour.
3. A required secret is missing.

Otherwise, continue with the next independent PR.

## Skill triggers

| Trigger | Skill |
|:--------|:------|
| New session | Read status/architecture/AGENTS |
| Any planned task | Read docs/skill-workflow.md and the PR ledger entry |
| Cross-boundary change | architect + codebase-design |
| Domain invariant | domain-modeling |
| External provider/Cloudflare behavior | research using official, current sources |
| Unproven runtime or repeated verification gap | prototype; create-verification-skill only for a recurring proof |
| Before code | tdd |
| Before commit | no-comments + interrogate (local checks) |
| Numeric claim | benchmark-checklist |
| UI change | Impeccable only under its pilot/provenance gate; browser and accessibility checks remain required |
| Opening PR | Follow docs/skill-workflow.md and the PR ledger; opening-a-pr is a Cursor-oriented playbook, not the project workflow |
| End of PR | Update plan/STATUS.md |
| Retro | /reflect |

## Proof statuses

- CONTRACT_APPROVED
- IMPLEMENTATION_VERIFIED
- INTEGRATION_VERIFIED
- PRODUCTION_READY

Never conflate. A green test is not a production readiness signal.

## What not to do

- Do not modify a contract.
- Project skills are advisory; contracts, roadmap, tests, and exact-head review are authoritative.
- Do not run Cursor-only automation, hooks, model routing, or merge playbooks in the Codex workflow.
- Do not declare PASS without execution.txt + results.json.
- Do not add a dependency without an ADR.
- Do not skip /interrogate.
- Do not use unwrap on user-input paths.
- Do not commit code that does not compile.
- Do not write comments that restate the code.
