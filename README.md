# Eez Funnel

> Self-hosted funnel builder. Rust + Cloudflare Workers + D1 + R2. MIT.

[![CI](https://github.com/lianihossam-eng/EezFunnels/actions/workflows/ci.yml/badge.svg)](https://github.com/lianihossam-eng/EezFunnels/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Current status

PR-000 supplies the Rust workspace and development checks. The Worker, dashboard, funnel commands, and deployment are not implemented yet. The v0.1.0 features below describe the planned application.

## What is Eez Funnel?

A funnel builder you deploy on your own Cloudflare account. Open the URL, land on the dashboard, build funnels, publish them on `/f/:slug`. No signup, no SaaS account, no monthly fee.

**Single-user by design.** v0.1.0 assumes you are the only user. Auth, landing page, and custom subdomains are planned for v0.2.

## Planned features (v0.1.0)

- Dashboard at `/`
- 5 sections: hero, features, testimonials, cta, footer
- 5 themes: minimal, bold, elegant, playful, corporate
- Visual editor with drag-and-drop, keyboard navigation, undo/redo
- Responsive preview (mobile, tablet, desktop)
- Publication at `/f/:slug`
- CLI: `eez funnels list`, `eez deploy`
- MCP server at `/mcp` with `execute_command`

## Quickstart

### Prerequisites

The bootstrap requires Rust 1.85.0. The later Cloudflare workflow also requires Node.js 20+, a Cloudflare account, and Wrangler.

### Planned deployment workflow

These commands require the Worker implementation and Cloudflare resource IDs from later phases. They do not run the PR-000 workspace.

```bash
git clone https://github.com/lianihossam-eng/EezFunnels.git
cd EezFunnels
wrangler login
wrangler d1 create eezfunnel
wrangler d1 migrations apply eezfunnel --remote
wrangler deploy
```

Live at `https://eezfunnel-<your-subdomain>.workers.dev`.

### Develop the bootstrap workspace

Install Rust 1.85.0 with rustfmt, clippy, and the wasm32-unknown-unknown target. In the prepared cloud environment, activate the project-local installation first.

```bash
source .project-tools/env.sh
cargo xtask check
cargo build -p eez-dashboard --target wasm32-unknown-unknown --no-default-features --features browser --locked
cargo run -p eez-cli --locked -- --help
```

On a machine with rustup on PATH, omit the first command. The checked-in rust-toolchain.toml selects the compiler and components. The WASM build validates packaging; it does not start a dashboard.

See docs/getting-started.md for the current development workflow.

## Architecture

```
Cloudflare Worker (Rust, Axum)
  GET /              -> Dashboard (Dioxus WASM)
  GET /f/:slug       -> Funnel published (SSR)
  /api/*             -> REST API
  /mcp               -> MCP server (HTTP)
  /assets/*          -> Assets from R2
  Bindings: D1, R2, KV
```

See `plan/architecture.md`.

## Documentation

- `docs/getting-started.md`
- `docs/deploy.md`
- `docs/commands.md`
- `docs/mcp.md`
- `docs/themes.md`
- `docs/security.md`

## Roadmap

- **v0.1.0** - self-hosted single-user dashboard, editor, publication, MCP
- **v0.2.0** - authentication, landing page, custom subdomains
- **v0.3.0** - CRM, analytics, AI funnel generation

## License

MIT. See `LICENSE`.
