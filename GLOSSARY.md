# Glossary

## Domain

- Funnel - ordered sections attached to a slug, rendered into a public page. ID: funnel_id.
- Revision - immutable snapshot. ID: revision_id.
- Published revision - served at /f/:slug.
- Section - composable block (hero, features, testimonials, cta, footer).
- Block - sub-element inside a section's settings.
- Theme - design tokens applied to a funnel.
- Design token - named value (e.g. --color-primary).
- Render manifest - references that determine exact output of a revision.
- Schema version - version of the funnel JSON structure.

## Architecture

- Command - user-visible action with stable ID and JSON parameters.
- Dispatcher - single entry point Session::execute.
- Renderer - turns funnel + theme into HTML. Sandboxed.
- Repository - mediates access to D1, R2, KV.
- Audit event - structured record of decision or action.
- Correlation ID - UUID propagated through every request.

## Deployment

- Worker - Cloudflare Workers runtime.
- D1 - SQLite-based serverless database.
- R2 - object storage for assets.
- KV - key-value store for caching.
- Wrangler - Cloudflare CLI.

## MCP

- MCP - Model Context Protocol.
- Tool - callable function: execute_command, list_commands, list_funnels, get_funnel.
- Bearer token - required for /mcp. Env: EEZ_MCP_TOKEN.

## Skills

- pstack - cursor/plugins/pstack.
- mattpocock/skills - delivery methodology.
- poteto-mode - pstack entry point.
