# Eez Funnel - Status

Current phase: PR-000 (local bootstrap verified; remote review pending)
Last updated: 2026-10-13

## Phases

- [ ] PR-000 Bootstrap (local checks verified; PR and hosted CI pending)
- [ ] PR-001 xtask
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

Proof status: IMPLEMENTATION_VERIFIED for the bootstrap tooling only.

- Eight Cargo members build with Rust 1.85.0.
- cargo xtask check passes fmt, clippy with warnings denied, tooling tests, and layers.
- The dashboard package compiles to WASM. It does not render a dashboard yet.
- The eez bootstrap CLI shows help and rejects unavailable commands.
- An initial failing layering test and final execution evidence are in plan/verification/PR-000/.

The application libraries remain empty. Product behavior, Cloudflare deployment, the full cargo xtask ci gate, and production readiness are unverified.

## Blockers

The runtime currently denies api.github.com, so opening the GitHub PR and inspecting hosted CI are blocked. The domain addition is saved in the environment configuration draft. Git read and push access to the selected repository work.

## Next action

Finish PR-000 remote review after api.github.com access is applied. Then execute PR-001 for the full ci, audit, and deployment automation. No merge or release is authorized by a local check alone.
