# Architecture

Eez Funnel is a single-owner, self-hosted funnel builder. The Rust workspace has five product layers, plus `xtask` for repository checks. The layer number is a constraint on ownership and dependency direction; it does not by itself authorize every lower-numbered dependency.

| Layer | Crates | Owns |
| --- | --- | --- |
| L0 | `eez-domain` | Business types, validation, and invariants. |
| L1 | `eez-render`, `eez-dashboard-presentation` | Published HTML rendering and dashboard presentation. |
| L2 | `eez-storage` | D1, R2, KV bindings and repositories. |
| L3 | `eez-commands` | User-visible operations, policy, and orchestration over domain and storage. |
| L4 | `eez-api`, `eez-dashboard`, `eez-cli` | HTTP Worker, browser application, and CLI entry points. |
| Tool | `xtask` | Formatting, lint, test, build, smoke, audit, and architecture checks. |

Product dependencies move toward lower layers. The checker also compares each internal crate dependency against this exact allowlist:

| Crate | May depend on |
| --- | --- |
| `eez-domain` | — |
| `eez-render` | `eez-domain` |
| `eez-dashboard-presentation` | `eez-domain` |
| `eez-storage` | `eez-domain` |
| `eez-commands` | `eez-domain`, `eez-render`, `eez-storage` |
| `eez-api` | `eez-domain`, `eez-render`, `eez-storage`, `eez-commands`, `eez-dashboard-presentation` |
| `eez-dashboard` | `eez-dashboard-presentation` |
| `eez-cli` | `eez-domain` |
| `xtask` | No product crate dependency is currently declared. |

## Request ownership

The HTTP adapter validates transport concerns such as method, headers, origin, body size, and content type. It decodes request bodies into typed inputs and maps command results and errors to HTTP responses. Business rules, published-document checks, repository coordination, and writes belong in `eez-commands`. Repositories implement storage access in `eez-storage`; business types and invariants belong in `eez-domain`.

The dashboard presentation crate owns browser views and client-side state. It may call the Worker through its internal HTTP client, but it does not own backend business rules or persistence. The CLI and MCP adapters dispatch the same supported command behavior rather than duplicating it.

## Checks

Run `cargo xtask layers` to validate workspace members, fixed package locations, product layer order, tool isolation, and the explicit internal dependency list. Run `cargo xtask check` for formatting, Clippy, tests, and layer validation. Run `cargo xtask ci` before merging for the additional Worker WASM build, dashboard asset build, local Worker smokes, and dependency audit. These checks validate the repository; they do not establish a production Cloudflare deployment.
