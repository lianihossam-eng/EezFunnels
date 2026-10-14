# PR-016 throughput checkpoint

- [x] **Blocking first steps.** Ground `apps/cli`, API routes, shared command dispatcher, Wrangler behavior, and the existing tag-triggered release workflow. Complete design selection before implementation because create and publish semantics are underspecified by the original plan.
- [x] **Independent workstreams.** The Linux/macOS/Windows build matrix now also runs on pull requests, with artifact upload remaining tag-only. CLI parsing, config persistence, API transport, idempotency, and real Worker integration are one code-coupled workstream; no independent source change is safe before the shared request/config types are settled.
- [x] **Shared mutable state.** Keep `apps/cli` and its package manifest under one implementation owner. CLI integration tests and the smoke script must use the same command/config/API data shapes; serialize edits to those files.
- [x] **Smallest safe decomposition.** Use one implementation owner after architecture selection. Splitting the CLI's command parser, config, network client, and retry state across writers would create an unnecessary shared interface before the user behavior is fixed.

Selected design: complete-operation `Funnels` methods with immutable retry tickets, recorded in `plan/decisions/D20_native_cli.md`. Implementation is delegated to one owner; the parent will review the shared diff and perform verification.
