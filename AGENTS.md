# Guidance for coding agents

Read [the architecture guide](docs/architecture.md) before changing code that crosses a crate boundary.

## Layer ownership

- `eez-domain` owns business types, validation, and invariants. It has no framework, rendering, or persistence dependency.
- `eez-render` and `eez-dashboard-presentation` are presentation libraries. They depend on the domain, not on storage or commands.
- `eez-storage` owns Cloudflare bindings and repositories and depends on the domain.
- `eez-commands` owns user-visible operations, orchestration, and policy. Keep HTTP request and response types in the API adapter.
- `eez-api`, `eez-dashboard`, and `eez-cli` are entry points. They adapt HTTP, browser, and CLI inputs to the shared command layer where applicable.
- `xtask` is a development tool, not a product dependency.

The allowed crate dependencies are checked by `cargo xtask layers` and form an explicit allowlist. Update that list only with a reviewed architecture change.

## Implementation rules

- Preserve the existing command IDs and their behavior when extracting internal modules.
- Keep public HTTP decoding and response construction in `apps/api`; put business validation, repository orchestration, and persistence in `eez-commands`.
- Keep dashboard modules organized around a clear responsibility and a small internal interface.
- Do not add a dependency without discussing its purpose and impact in the PR.
- Avoid panic paths for production input. Do not use `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, or `unsafe` outside tests.
- Add behavior tests for changes and regression tests for bug fixes. Do not write tests that merely enforce a file layout.

## Verification and pull requests

- Run `cargo xtask check` before each commit.
- Run `cargo xtask ci` before merging; it includes Rust checks, the Worker WASM build, dashboard assets, local smoke tests, and dependency audit.
- Keep commits scoped and use conventional commit subjects. Open an issue for non-trivial work, and link the issue from each pull request.
- Describe the behavior, scope, compatibility effects, and verification in each PR.
