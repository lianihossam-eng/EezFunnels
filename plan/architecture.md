# Eez Funnel - Architecture

## Overview

Self-hosted single-user funnel builder on Cloudflare Workers, D1, R2, KV.

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

D1:
- funnels(id, name, slug, published_revision_id, created_at, updated_at)
- revisions(id, funnel_id, schema_version, data_json, render_manifest_json, created_at)
- themes(id, name, tokens_json, is_default)
- assets(id, r2_key, mime, size_bytes, created_at)
- audit_logs(id, action, payload_json, created_at)

R2: assets/<id>

KV: cache:funnel:<slug>:<revision_id>, cache:theme:<id>

## Security

Trust boundaries:
- Browser -> Worker: untrusted.
- Worker -> D1/R2/KV: trusted.
- Claude -> /mcp: untrusted, Bearer token required.
- Owner -> dashboard: implicit trust.

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

## Non-goals for v0.1.0

- Multi-user, auth.
- Landing page.
- Custom subdomains.
- CRM, analytics, payments, emails.
- AI-generated funnels.
