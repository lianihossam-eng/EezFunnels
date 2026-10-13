# Develop the bootstrap workspace

PR-000 provides eight Cargo packages and the `eez` bootstrap binary. Application features remain unimplemented.

1. Install rustup. The repository pins Rust 1.85.0, rustfmt, clippy, and `wasm32-unknown-unknown`.
2. Open the repository root. In this cloud environment, run `source .project-tools/env.sh` to activate its local Rust installation.
3. Run `cargo xtask check`. It runs the format check, clippy with warnings denied, workspace tests, and dependency layer validation. A failed step stops the command.
4. Run `cargo build -p eez-dashboard --target wasm32-unknown-unknown --locked` to check the dashboard package compiles to WASM.
5. Run `cargo run -p eez-cli --locked -- --help` to see the bootstrap CLI. Funnel commands are not available yet.

Run `cargo xtask layers` to check dependencies separately. Runtime packages may depend only on lower layers. Test, build, optional, and target-specific declarations are included.

The product packages contain no application behavior or product tests yet. Workspace tests verify the development tooling. WASM compilation does not verify a working dashboard.

Worker startup and D1 migrations begin in PR-002. Full `cargo xtask ci` and deployment automation belong to PR-001. No production deployment has been verified.
