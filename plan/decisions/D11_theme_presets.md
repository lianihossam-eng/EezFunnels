# D11 - Preset theme ownership

Status: accepted for PR-007.

The five built-in themes are a closed domain catalog. `ThemePreset` owns each stable ID, display
name, and typed token set. Its token JSON is serialized from the same ordered set used by the
renderer; the renderer remains responsible for CSS validation and the content-addressed theme
version from D9. A migration updates the existing seed rows, and storage verification checks that
the database catalog matches the domain catalog.

`theme.set` accepts a preset ID and applies it to an immutable revision by producing a fresh
revision ID, the current renderer version, and the manifest theme version. It preserves section
content, assets hash, and config hash. The caller receives the corresponding validated renderer
tokens to render that new revision. Persistence remains the responsibility of the command session
added later; this command has no mutable in-memory or database side effect.

The renderer advances to `renderer-v2` when it adds the trusted stylesheet that consumes preset
tokens. Existing `renderer-v1` manifests keep the prior unstyled output; unknown renderer versions
remain rejected. This keeps stored v1 output stable while new theme changes opt into the new
presentation.

Two shapes were considered. Making D1 rows authoritative would require loading mutable theme data
into the render path and would make output depend on a database lookup. Keeping the closed preset
catalog in the domain makes the same IDs and token values available to commands, rendering, and
storage verification without coupling the renderer to storage. The migration is still the D1 seed
representation, and integration checks reject drift between it and the catalog.

The implementation uses the workspace's existing `serde` and `serde_json` packages to encode the
typed preset and ordered token map. No new package or contract behavior is introduced. C-01 stays
unchanged; theme selection follows its immutable-revision rule.
