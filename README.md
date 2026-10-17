# Eez Funnel

An open source, single-owner funnel builder that you run on your own Cloudflare account. The v0.1.0 GitHub Release is published with CLI binaries for Linux, macOS, and Windows; it predates the typed editor described below. The application is still under development, and the release does not verify a Cloudflare production deployment. Cloudflare usage is billed under your account where applicable.

[![CI](https://github.com/lianihossam-eng/EezFunnels/actions/workflows/ci.yml/badge.svg)](https://github.com/lianihossam-eng/EezFunnels/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## What you can do

- Create and manage funnels from five starter blueprints: tripwire, video sales letter, webinar, product launch, and lead generation.
- Edit typed pages, sections, supported content blocks, actions, and theme settings in the dashboard; preview at mobile, tablet, and desktop widths.
- Preview conversion of existing version 1 funnels before saving a typed revision. The editor reports unsupported content before conversion.
- Review revision history, save immutable revisions, and publish or unpublish them at `/f/:slug`.
- Use the native `eez` CLI to list, create, and publish funnels, retry an ambiguous mutation, or forward a Wrangler deploy command.
- Connect an MCP client to `/mcp` with a Worker Bearer secret. The server exposes `list_commands`, `execute_command`, `list_funnels`, and `get_funnel`.

The current release has no built-in user login or multi-user roles. It does not provide funnel JSON import/export, custom CSS or user-defined theme presets, image uploads, CRM, analytics, or AI generation. See [Security](docs/security.md) before exposing an instance publicly.

## Scope

Eez Funnel is an open source funnel builder designed for self-hosted use on Cloudflare. The current release provides a single-owner dashboard without built-in accounts or multi-user roles.

## Demo

This animated capture shows the current dashboard, the webinar creation form, and its editor with the mobile preview selected. The sample content was created through the running local app.

![Eez Funnel dashboard, webinar creation, and mobile editor demo](docs/assets/demo.gif)

## Quickstart

### Requirements

- Rust 1.85.0 with `rustfmt`, `clippy`, and the `wasm32-unknown-unknown` target. The checked-in `rust-toolchain.toml` pins these for the workspace.
- Node.js 22 or newer and npm. Wrangler is installed from the locked npm dependencies.
- A Cloudflare account is needed only for a remote deployment. Local development uses local D1/R2/KV bindings.

### Run locally

From the repository root:

```sh
npm ci
./scripts/install-worker-build.sh
npx wrangler d1 migrations apply DB --local --env dev
npm run dev -- --env dev
```

Open <http://localhost:8787>.  The Wrangler build command installs the pinned `wasm-bindgen` tool if it is missing. The separate `worker-build` CLI is installed by the script above; the application itself stays on Rust 1.85.0.

Run `cargo xtask check` for formatting, clippy, workspace tests, and layer validation. For the full local CI workflow, install the pinned audit tool with `./scripts/install-cargo-audit.sh`, then run `cargo xtask ci`.

See [Getting started](docs/getting-started.md) for CLI and local workflow details, and [Deploy](docs/deploy.md) before creating or changing remote Cloudflare resources.

## Architecture

The Rust Cloudflare Worker serves the Dioxus dashboard and the funnel API. D1 stores funnels, revisions, and command receipts; R2 and KV bindings are configured by the Worker. The dashboard assets are built into `apps/dashboard/dist`. Cloudflare production bindings and deployment remain unverified.

| Path | Current behavior |
|:---|:---|
| `/`, `/funnels`, `/funnels/:id` | Dashboard, funnel list, and editor |
| `/api/funnels` | List funnels or create a version 1 funnel |
| `/api/funnels/v2` | Create a typed version 2 funnel |
| `/api/funnels/:id` | Read or delete a funnel |
| `/api/funnels/:id/document` and `/revisions-v2` | Read a typed revision and its revision history |
| `/api/funnels/:id/edit-v2` and `/preview-v2` | Apply typed edits and preview a document |
| `/api/funnels/:id/convert-v2/preview` and `/convert-v2` | Preview and save a version 1 conversion |
| `/api/funnels/:id/stages-v2` and `/revisions-v2` | Upload and save a typed revision |
| `/api/funnels/:id/preview` | Preview proposed editor state |
| `/api/funnels/:id/revisions` | Save an editor revision |
| `/api/funnels/:id/publish` and `/unpublish` | Change the published revision pointer |
| `/api/v2/stage-capacity` | Read the configured staged-save capacity |
| `/f/:slug` | Render a published funnel |
| `/mcp` | Stateless Streamable HTTP MCP endpoint |
| `/health` | Worker health response |

Commands, the native CLI, and MCP behavior are documented in [Commands](docs/commands.md), [Themes](docs/themes.md), and [MCP setup](docs/mcp.md). The architecture is summarized above.

## License

MIT. See [LICENSE](LICENSE).
