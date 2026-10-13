# D10 - Render section content

## Problem

The domain `Section` is intentionally an immutable id/type/order record, while PR-006 needs content
that can render five distinct section shapes. C-01 leaves the serialization format out of scope, so
the renderer needs a typed content boundary without moving storage or wire-format decisions into the
domain layer.

## Usage

```rust
let contents = BTreeMap::from([
    (
        "hero-1".to_owned(),
        SectionContent::Hero {
            title: "Build a better funnel".to_owned(),
            subtitle: "A short supporting line".to_owned(),
            cta_label: "Get started".to_owned(),
            cta_url: "https://example.com/start".to_owned(),
            image: None,
        },
    ),
]);
let html = Renderer::new().render(&revision, &theme, &contents)?;
```

## Shape

The render crate owns a closed `SectionContent` enum with `Hero`, `Features`, `Testimonials`, `Cta`,
and `Footer` variants. Small typed item structs carry feature cards, testimonials, footer links, and
the optional hero image. Callers provide a `BTreeMap` keyed by the domain section ID. Rendering
requires exactly one content value per revision section, rejects extra or missing keys, and rejects
a content variant that does not match the section type. This keeps all section policy behind one
renderer entry point and leaves domain persistence metadata unchanged.

`RenderManifest::config_hash` binds the immutable content map to that revision. Before constructing
the manifest, callers calculate the value with the public `content_config_hash(&contents)` function
and store it as `config_hash`. The
versioned `content-sha256-v1` digest covers sorted section IDs, each enum variant, every field,
optional-value markers, and ordered list lengths and entries, using explicit length-prefixed UTF-8
fields. `Renderer::render` validates the map and then recomputes the digest; a mismatch returns
`SectionContentHashMismatch` before emitting HTML. Any content edit therefore requires a new digest
and manifest, and the producer must assign a new revision ID as required by C-01. Reusing a
manifest while changing map contents is rejected.

Section text is HTML-text encoded and URL attributes are HTML-attribute encoded. URL values may be
absolute HTTPS URLs written with an explicit `https://` authority, a valid host, and no user
information, or same-worker root-relative paths
that start with one slash and resolve to the fixed same-origin base. Protocol-relative paths are
rejected. This supports the architecture's `/assets/*` R2 routes without opening external-host
references. Raw whitespace, controls, quotes, backslashes, and values over 2,048 bytes are rejected
before parsing. The `url` crate is added as a direct workspace dependency so validation uses the
existing locked standards-based parser instead of a hand-written URL parser; this adds no new
locked package. Item lists are capped at C-01's `max_blocks_per_section` value of 50, and the
aggregate UTF-8 bytes of section IDs and content strings are capped at C-01's 1 MiB funnel payload
limit before HTML allocation.

## Synthesis decision

The typed render-layer map is selected. The domain remains independent of content presentation and
serialization. Only one implementation model was available for this task, so no cross-model arena
was run; the concrete alternatives below were compared against the existing layer and contract
boundaries.

## Tradeoffs accepted

- Callers must build a section-ID keyed map, in exchange for explicit matching between immutable
  section metadata and the content being rendered.
- URL references are limited to explicit absolute `https://` authorities (case-insensitive scheme)
  or same-worker root-relative paths, in exchange for supporting `/assets/*` while rejecting
  `https:example.com`, `https:/example.com`, `javascript:`, `data:`, and protocol-relative URLs.
- Content types live in `eez-render`, in exchange for keeping C-01's domain model free of a wire or
  storage representation.

## Alternatives considered

- Add section payload fields directly to domain `Section`. This exposes presentation details in L0
  and implicitly chooses a C-01 serialization shape, so it loses the layer and contract boundary.
- Pass five separate payload maps into `Renderer::render`. This exposes caller coordination and
  duplicates exact-coverage checks, so its larger interface hides less than the single typed map.
- Accept arbitrary JSON objects and inspect keys while rendering. This defers type errors to runtime
  and leaves callers dependent on string keys, so it is weaker than a closed enum.

## Open questions and risks

- Should a later product contract allow `mailto:`, arbitrary relative links, or non-HTTPS external
  URLs? PR-006 accepts only absolute HTTPS with explicit authority and same-worker root-relative
  paths.
- The renderer's 1 MiB check covers section IDs and content strings, not the serialized JSON
  punctuation or other funnel metadata; storage/API must retain the full C-01 serialized-payload
  check as an independent boundary.

## Verification focus

Keep one rendered snapshot per section, test hostile text and URL values before accepting content,
and retain deterministic 10,000-case checks for both text and URL boundaries.
