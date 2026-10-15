# Eez Funnel - Architecture

## Overview

The current v0.1.0 product is a self hosted, single owner funnel builder on Cloudflare Workers, D1, R2, and KV. The GitHub release publishes CLI binaries; it does not establish that a Cloudflare production deployment is configured or verified.

The full business product remains in the public repository and shares one codebase across self hosted and any future private distribution. Platform user accounts, authentication, and a private platform admin dashboard are reserved for the last roadmap phase. The owner funnel dashboard and business features remain common. See [the product roadmap](product-roadmap.md).

## Layering

L0: domain    (types, contracts, invariants)
L1: render, dashboard-presentation (HTML generation, sandboxed)
L2: storage   (D1, R2, KV bindings)
L3: commands  (dispatcher, policies)
L4: api, dashboard app, cli

Enforced by cargo xtask layers.

## Request flow

Browser -> GET /, /funnels, /funnels/:id -> host allowlist -> dashboard SSR
Browser -> GET /assets/* -> host allowlist -> Worker-first ASSETS binding
Browser -> GET /api/funnels -> commands -> storage::D1 -> bounded JSON (100 funnels)
Browser -> GET /f/:slug -> storage -> render -> HTML
Claude  -> POST /mcp (Bearer) -> mcp::handle -> commands::execute -> JSON

## Data model

D1 (current, v0.1.0):
- funnels(id, name, slug, published_revision_id, created_at, updated_at)
- revisions(id, funnel_id, schema_version, data_json, render_manifest_json, created_at)
- themes(id, name, tokens_json, is_default)
- assets(id, r2_key, mime, size_bytes, created_at)
- audit_logs(id, action, payload_json, created_at)

R2 (current binding; product upload flow absent): assets/<id>

KV is a current binding, but no KV cache read/write path is present in the application crates. Any future public-render/theme cache keying is a proposed design, not current architecture; see the roadmap's custom-host version/cache gate and verify actual cache implementation before planning invalidation work.

## Security

Trust boundaries:
- Browser -> Worker: untrusted.
- Worker -> D1/R2/KV: trusted.
- Claude -> /mcp: untrusted, Bearer token required.
- Owner -> dashboard: implicit trust in the current single owner product.

The current trust boundary does not define hosted platform authentication. Cloudflare Access may be an optional perimeter for self hosted installations. It is not the selected identity model for a future hosted service. CRM contacts, leads, and buyers are business records, not platform users.

Sandbox:
- Renderer never executes user code.
- No script tag from user content.
- Fuzzing on all JSON parsers (10 000 cases per boundary).

Never Crash:
- No unwrap, expect, panic! on non-test paths.
- No unsafe.
- Inputs hostile: get() not [i], checked arithmetic, bounded recursion.

## Deployment

- wrangler deploy from repo root.
- wrangler d1 migrations apply eezfunnel --remote.
- wrangler secret put EEZ_MCP_TOKEN.
- Bindings in wrangler.toml.

These commands describe the deployment workflow. No production Cloudflare binding or deployment has been verified.

## v0.1.0 scope boundary

- Multi-user, auth.
- Landing page.
- Custom subdomains.
- CRM, analytics, payments, emails.
- AI-generated funnels.

These are exclusions from the v0.1.0 release, not exclusions from the product roadmap. See [the product roadmap](product-roadmap.md) for proposed phases and the status of local adversarial review.
