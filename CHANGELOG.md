# Changelog

All notable changes to this project are documented in this file.

The format is based on Keep a Changelog, and this project adheres to Semantic Versioning.

## [Unreleased]

### Added
- Typed version 2 funnel documents with page, section, block, theme, and revision history editing.
- Preview-first conversion for version 1 funnels, with explicit blockers for unsupported content.
- Bounded R2 staging for typed revisions, including local capacity reporting and cleanup.
- Funnel-scoped form completion counts by UTC date, stable form ID, and closed campaign-channel category, limited to retained submissions.
- Follow-up statuses for retained submissions in the dashboard inbox, with filtering by status.

### Changed
- Dashboard funnel creation and editing use typed version 2 documents while preserving version 1 reads and conversion.

## [0.1.0] - 2026-10-15

### Added
- Dashboard Dioxus WASM at /
- Funnel model with revisions and render manifests
- 5 sections: hero, features, testimonials, cta, footer
- 5 themes: minimal, bold, elegant, playful, corporate
- Visual editor with drag-and-drop and keyboard navigation
- Undo/redo over JSON diffs
- Responsive preview (375, 768, 1280 px)
- Publication at /f/:slug
- CLI: eez funnels list, eez funnels create, eez deploy
- MCP server at /mcp with execute_command
- Cloudflare Workers runtime configured for D1, R2, and KV
