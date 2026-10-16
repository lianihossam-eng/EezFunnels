# Glossary

## Domain

- Funnel - ordered set of pages published under one funnel slug. ID: funnel_id.
- Page - route-level part of a funnel with an ordered path and content. ID: page_id.
- Revision - complete immutable snapshot of a funnel's content and settings. ID: revision_id.
- Published revision - revision currently served by the funnel's public routes.
- Section - typed layout container on a page, such as hero, features, testimonials, cta, or footer.
- Block - typed content or interaction element inside a section.
- Form definition - versioned description of fields, purpose, and completion result.
- Form completion count - successful persisted form submission that has not expired; it is not a conversion rate.
- Field - named input with a type and validation bounds in a form definition.
- Asset - immutable media item referenced by an image block. ID: asset_id.
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


