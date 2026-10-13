# Eez Funnel - Status

Current phase: PR-001 (xtask implementation verified locally; pull request pending)
Last updated: 2026-10-13

## Phases

- [x] PR-000 Bootstrap (merged as PR #1 into `main`, merge commit `1256e01`)
- [ ] PR-001 xtask (local implementation verified; PR #2 not yet opened)
- [ ] PR-002 Worker + D1 + R2 + KV
- [ ] PR-003 Schema + seed themes
- [ ] PR-004 Domain model
- [ ] PR-005 Render engine
- [ ] PR-006 5 sections
- [ ] PR-007 5 themes
- [ ] PR-008 Dashboard shell
- [ ] PR-009 Funnel list + creation
- [ ] PR-010 Visual editor
- [ ] PR-011 Inspector
- [ ] PR-012 Undo/redo
- [ ] PR-013 Responsive preview
- [ ] PR-014 Publication /f/:slug
- [ ] PR-015 Command registry
- [ ] PR-016 CLI
- [ ] PR-017 MCP server
- [ ] PR-018 Documentation
- [ ] PR-019 Tests E2E + quality
- [ ] PR-020 Release v0.1.0

## Verified locally

Proof status: IMPLEMENTATION_VERIFIED for the PR-001 tooling on the current branch. This does not
establish integration or production readiness.

- `cargo xtask ci` passes in the isolated target `/tmp/eezfunnel-pr001-final-target`. It runs fmt,
  Clippy with warnings denied, workspace tests, layers, the dashboard WASM build, and `cargo audit`.
- The run passed 23 xtask unit tests, 2 bootstrap integration tests, and 4 PR-001 integration tests.
  The integration tests check fail-fast CI behavior, audit failure propagation, deploy argument
  forwarding from the workspace root, and Wrangler failure propagation.
- The dashboard WASM artifact is 1,432,977 bytes and has the expected WebAssembly magic and version.
- `cargo run -p eez-cli --locked -- --help` and `cargo xtask layers` pass.
- `cargo audit` 0.22.2 scanned 1,308 RustSec advisories and 19 locked dependencies without finding
  a vulnerability. The project stays on Rust 1.85.0. The installer uses pinned Rust 1.88.0 only to
  build the audit tool.
- `plan/verification/PR-001/execution.txt` and `results.json` record the commands and outcomes.

The deploy tests use a mock Wrangler executable. Wrangler is not installed in this environment, and
no Cloudflare credentials or deployment configuration are available. A real deployment remains
unverified.

## Blockers

PR #2 has not been opened, so hosted CI has not yet run on its head. Hosted `cargo xtask ci` must pass
on the current PR head before merge. A Cloudflare environment with Wrangler and its required
credentials is also needed to verify deployment behavior against a real account.

## Next action

Open PR #2 against `main`, then wait for hosted CI on its current head and the reserved PR review.
PR-001 introduces the required `cargo xtask ci` merge gate for itself and every later PR. Local
checks do not authorize a merge or establish production readiness.
