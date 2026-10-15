# Getting started

This guide runs the Worker, dashboard, and local D1 database on your machine. The local `dev` Wrangler environment uses local bindings; these steps do not migrate a Cloudflare database or deploy a Worker.

## Prerequisites

- Rust 1.85.0 with `rustfmt`, `clippy`, and `wasm32-unknown-unknown`. The repository pins the version and components in `rust-toolchain.toml`.
- Node.js 22 or newer and npm, as required by `package.json`.
- Wrangler is installed by the lockfile through npm.

The dashboard build also needs the pinned `worker-build` CLI. Its installer uses Rust 1.88.0 to install that tool; Cargo still selects Rust 1.85.0 for the application. The Wrangler build command installs `wasm-bindgen` 0.2.129 when needed.

## Install and start

Run these commands from the repository root:

```sh
npm ci
./scripts/install-worker-build.sh
npx wrangler d1 migrations apply DB --local --env dev
npm run dev -- --env dev
```

Open <http://localhost:8787>. Use the **Funnels** navigation to create a funnel from one of the five blueprints. The editor lets you reorder its starter sections, edit the fields available for those sections, choose a theme preset, preview at three widths, and save revisions.

The local database and other Wrangler state are stored under `.wrangler/` by default. To keep local state elsewhere, pass Wrangler's `--persist-to <directory>` option to both the migration and dev commands. Keep the same directory between those commands.

To stop the Worker, press Ctrl+C in the terminal running Wrangler.

## Use the CLI against the local Worker

Build the native CLI and point it at the local Worker:

```sh
cargo build --locked --release -p eez-cli
cargo run -p eez-cli -- config set endpoint http://localhost:8787
cargo run -p eez-cli -- funnels create 'October webinar' --blueprint webinar
cargo run -p eez-cli -- funnels list
```

The CLI also supports `eez funnels publish <funnel-id>` and `eez funnels retry <operation-uuid>`. It reads the current saved revision when publishing. It does not include editor or delete commands. Local HTTP is accepted only for localhost and loopback addresses; remote endpoints must use HTTPS. See the [CLI command reference](commands.md#native-cli).

## Development checks

`cargo xtask check` runs the Rust format check, clippy with warnings denied, workspace tests, and dependency-layer validation:

```sh
cargo xtask check
```

For the full local CI workflow, which also builds the Worker and dashboard assets, runs local funnel and MCP smokes, and audits dependencies, install the pinned audit tool and run:

```sh
./scripts/install-cargo-audit.sh
cargo xtask ci
```

The full workflow uses local Wrangler bindings for its smoke checks. It does not deploy or apply remote migrations. In a prepared Codex cloud environment, run `source .project-tools/env.sh` before Cargo commands when that file exists.

## Next steps

- [Deploy to Cloudflare](deploy.md)
- [CLI and command reference](commands.md)
- [Connect an MCP client](mcp.md)
- [Theme presets and editor controls](themes.md)
- [Security and Cloudflare Access](security.md)
