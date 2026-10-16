# C-05 - Funnel Document Schema Version 2

Status: proposed by PR-023. This is a new, separately versioned contract. It
does not edit or reinterpret frozen C-01 or C-01_config. Merging PR-023 after
exact-head review and required checks is the governance approval for this
separate V2 supplement; it does not add runtime support. Runtime support is
absent until the version-2 reader, compatibility registry, and any required
migrations are implemented and verified by PR-024.

## Objective

Define the immutable, multi-page funnel document that later structural editor,
forms, and media work can share across the renderer, dashboard, CLI, and
commands. This contract describes data shape only; it does not ship those
features or authorize collection, publication, or external effects.

## Version routing

The database revision column `schema_version` remains the structural
compatibility version defined by C-01. A future version-2 row is readable only
when all of the following hold:

- The row has structural `schema_version = 2`.
- Its document JSON has `document_type = "eezfunnel.funnel-document"`,
  `document_version = 2`, and structural `schema_version = 2`.
- Its render manifest has `manifest_type = "eezfunnel.render-manifest"`,
  `manifest_version = 2`, and structural `schema_version = 2`.
- Each discriminator is checked against the V2 closed decoder; no decoder is
  selected from a missing, unknown, or conflicting value.

The current V1 reader remains byte-for-byte compatible with its existing
dispatch: row `schema_version = 1`, required embedded `schema_version = 1`,
and legacy `format_version` 1 or 2. Legacy `format_version = 2` continues to
mean theme overrides only. A missing or unknown version, unsupported row
version, or disagreement between row, document, and manifest fails closed.
No field is inferred from a missing version. V1 serialization, digest
semantics, and render manifests do not change.

`schema_version = 2` is a proposed new structural compatibility version, not a
claim that the current frozen C-01_config or released v0.1.0 runtime supports
it. PR-024 must explicitly add V2 to the runtime compatibility registry while
preserving V1 reads and saves. The old `format_version` field is not used by
V2.

## Document shape

The top-level object has exactly these required fields and rejects unknown
fields. The `theme` record has exactly `{preset_id, overrides}` and rejects
unknown fields at both levels:

```json
{
  "document_type": "eezfunnel.funnel-document",
  "document_version": 2,
  "schema_version": 2,
  "funnel_id": "funnel-id",
  "blueprint_id": "lead_gen",
  "theme": {"preset_id": "minimal", "overrides": {}},
  "entry_page_id": "page-entry",
  "pages": [],
  "forms": []
}
```

The JSON above illustrates field names; a funnel must contain at least one
valid page. Records have the exact required fields stated below and reject
unknown fields. The block, action, image-alt, and form-field variants are
tagged unions; ordinary records are not tagged. Optional fields are identified
explicitly below. The normative field and graph rules are:

- **Identifiers:** funnel, revision, page, section, block, form, field, and
  asset IDs are distinct typed strings matching
  `[A-Za-z0-9][A-Za-z0-9_-]{0,63}`. Funnel, page, section, block, form,
  field, and asset IDs are opaque and stable across edits; every accepted save
  that creates a revision allocates a fresh revision ID as required by C-01.
  Object IDs are unique within their type across the document; form field IDs
  are unique across the document as well. Asset IDs are references, not object
  definitions, so the same asset ID may appear in multiple image blocks.
  Copy/duplicate operations allocate fresh IDs for every copied node. Page,
  form, and block references must resolve to the correct typed object.
  `asset_id` is a typed opaque reference whose existence and availability are
  checked by the later media contract. Validation compares IDs byte-for-byte;
  it does not case-fold or normalize Unicode.
- **Stored identity:** on trusted save, `document.funnel_id` must equal both
  the target funnel ID and the funnel ID of the revision row being created.
  On every stored read, it must equal the row's funnel ID. A mismatch fails
  closed before rendering or returning the document. The manifest's
  `revision_id` must equal the stored revision row ID.
- **Blueprint and record shapes:** `blueprint_id` is one of `tripwire`, `vsl`,
  `webinar`, `launch`, or `lead_gen`. A page record has exactly `{id, slug,
  title, sections}`. A section record has exactly `{id, type, blocks}`. Page
  titles are 1–8,192 UTF-8 bytes. Every block carries its own `id`; the closed
  block variants have these exact shapes: heading
  `{id, kind:"heading", level, text}`, text
  `{id, kind:"text", text}`, list
  `{id, kind:"list", items}`, button
  `{id, kind:"button", label, action}`, image
  `{id, kind:"image", asset_id, alt}`, and form
  `{id, kind:"form", form_id}`. `level` is an integer from 1 to 6. An image
  `alt` value is exactly `{kind:"text", value}` or `{kind:"decorative"}`.
  A button action is exactly `{kind:"navigate_page", page_id}` or
  `{kind:"external_https", url}`. A form block renders the referenced form
  definition as one accessible form: its ordered fields plus the built-in
  `request_consent` control and, when configured, the `marketing_opt_in`
  control. These controls are not generic fields and use those exact stable
  role keys in later submission payloads. Choice evidence binds each role key
  to the exact `form_id`, `revision_id`, source `page_id`, `copy_version`, and
  boolean choice. A form's declared success-page result is the only
  event-conditioned graph edge. Collection, submission, and evidence storage
  are outside this contract; button actions cannot submit forms.
  Unknown or extra fields fail for every record and variant.
- **Pages and routes:** `pages` is an ordered array of 1–50 pages; array order
  is the editorial order, so no duplicated numeric `order` field exists.
  `entry_page_id` identifies exactly one page whose `slug` is the empty string
  and whose public path is `/f/:funnel_slug`. Every other page has a lower-case
  ASCII slug matching `[a-z0-9]+(?:-[a-z0-9]+)*`, at most 64 bytes; slugs are
  unique and route to `/f/:funnel_slug/:page_slug`. Page IDs remain stable when
  a page title or slug changes. Custom hostnames are outside this document.
- **Sections and layout:** each page has 0–100 sections; the whole document has
  at most 100 sections, preserving the existing section ceiling. Sections are
  rendered in array order as a vertical stack. A section has one stable ID, a
  closed `type` (`hero`, `features`, `testimonials`, `cta`, or `footer`), and an
  ordered `blocks` array of 0–50 blocks. There is no arbitrary grid, CSS,
  markup, script, or recursive child layout.
- **Blocks:** `blocks` is a closed tagged union with the exact shapes above.
  Heading text, list items, button labels, and non-decorative alt text are
  non-empty; text-block values may be empty. Every text value is UTF-8 and at
  most 8,192 bytes, and is rendered as text, never interpreted as HTML, CSS,
  or script. A list has 1–50 items. An external URL is at most 2,048 UTF-8
  bytes. Its input must contain an ASCII scheme followed by `://`, with the
  scheme equal to `https` case-insensitively and a non-empty authority before
  any `/`, `?`, or `#`; relative spellings such as `https:example.com` fail.
  The input contains no control characters, whitespace, quotes, angle
  brackets, or backslashes. Every percent escape must contain two hex digits;
  escapes decoding to bytes `00`–`20`, `7f`, `22`, `27`, `3c`, `3e`, or `5c`
  fail. A WHATWG URL parser must produce scheme `https`, a non-empty host, and
  no user information. Persist and render the parser's canonical serialized
  URL, never the original spelling; canonicalization replaces the submitted
  value before document serialization and digesting. Rust/JavaScript parity
  fixtures must cover authority, backslash, user-info, and canonicalization
  cases.
  Image asset existence, digest, and public delivery are verified by the later
  media contract. The image variant is part of the V2 schema, but PR-024 must
  reject saves/finalization of image-bearing documents until PR-025 provides
  immutable asset metadata and the complete upload/finalize path. The editor
  must not create placeholder asset records or digests. PR-025 enables those
  saves only after the metadata and media route checks pass.
- **Actions and page edges:** each `navigate_page` action references an
  existing page ID; `external_https` follows the URL rule above and adds no
  internal graph edge. A form with a `success_page` result references one
  existing `page_id`.
  Each `form` block references an existing form; one form may be placed on
  multiple pages, but at most once on any one page, and every form definition
  must be placed at least once. Build the directed edge set by adding
  `source_page -> target_page` for every `navigate_page` button and
  `source_page -> form.result.page_id` for every placed form whose result kind
  is `success_page`. An `inline` result adds no edge. Deduplicate identical
  source/target pairs before graph validation. These are potential
  navigation/reachability edges: a `navigate_page` edge is followed on click,
  and a `success_page` edge is followed only after a valid form submission.
  The declared success-page result is the only event-conditioned edge; no
  arbitrary conditional-routing rule is accepted.
  This graph must be acyclic, and every non-entry page must be reachable from
  the entry page; terminal pages are allowed. No implicit, dangling,
  cross-funnel, or arbitrary URL edge is accepted. Clients do not send a second
  edge list that could disagree.
- **Forms:** a form has exactly `{id, fields, purpose, request_consent,
  marketing_opt_in?, result}` and 1–50 ordered fields. `purpose` is non-empty
  plain text, 1–1,024 UTF-8 bytes. `request_consent` and optional
  `marketing_opt_in` each have exactly `{copy_version, copy}`; `copy_version`
  matches the stable identifier pattern above and `copy` is non-empty plain
  text, 1–1,024 UTF-8 bytes. A change to the meaning or copy requires a new
  `copy_version`. The request-handling acknowledgment is distinct from purchase
  authorization. The request-handling control is required and starts
  unchecked. If present, the optional marketing control is separate and also
  starts unchecked; neither field records a person's choice in the document.
  These fields define purpose and displayed copy, not legal compliance.

  `result` is exactly `{kind:"inline", message}` or
  `{kind:"success_page", page_id}`. `message` is non-empty plain text,
  1–1,024 UTF-8 bytes. `page_id` must identify an existing page. A form field
  is a closed tagged record: `text`, `email`, and `phone` fields have exactly
  `{id, name, label, help?, kind, required, validation?}`;
  `checkbox` has exactly `{id, name, label, help?, kind:"checkbox", required}`;
  `select` has exactly
  `{id, name, label, help?, kind:"select", required, options}`. `name` matches
  `[a-z][a-z0-9_]{0,63}` and is unique within that form; field IDs are globally
  unique. The generic `name` values `request_consent` and `marketing_opt_in`
  are reserved for their built-in consent controls and are rejected on generic
  fields. Labels are 1–1,024 UTF-8 bytes. Present `help` is non-empty plain
  text, at most 1,024 UTF-8 bytes. `required` is a JSON boolean. `select` has
  1–50 unique, non-empty plain-text options, each at most 1,024 UTF-8 bytes.
  Option uniqueness is byte-for-byte and case-sensitive, with no Unicode
  normalization. `validation`, allowed only for `text`, `email`, and `phone`,
  has exactly `{min_length?, max_length?}`. Lengths count Unicode code points,
  not grapheme clusters; omitted minimum is 0 and omitted maximum is the
  field-kind cap: 8,192 for `text`, 320 for `email`, and 64 for `phone`. A
  supplied minimum is 0 through the kind cap; a supplied maximum is 1 through
  the kind cap; when both are present, minimum must not exceed maximum.
  `email` uses the HTML standard's single-email-address validity semantics,
  with no deliverability claim or arbitrary regex. `phone` is free text with no
  country-specific format rule. When capture is later enabled, required
  `text`, `email`, and `phone` fields must be non-empty; optional fields may be
  absent or empty, and length validation applies to non-empty values. Required
  `select` fields must contain one listed option; an optional one may be
  absent, but any present value must equal one listed option. A required
  checkbox accepts only `true`; an optional one may be absent, `false`, or
  `true`.
  PR-030 defines the exact submission payload encoding and enforces
  `request_consent = true` before capture. If an optional marketing control is
  configured, its choice is a separate boolean; an omitted choice is treated
  as `false`, and a payload cannot invent an unconfigured control. Submission
  evidence records the form, revision, source page, role key, `copy_version`,
  and boolean choice so that it proves exactly which text was displayed. Form
  definitions are presentation and validation schema only: submissions,
  stored choices, marketing permission, retention, deletion, and legal basis
  are governed separately.

  Owner preview of a form is inert and has no submission or network effects;
  PR-028 owns that preview. Public rendering and publication of any
  form-bearing revision fail closed until PR-030 enables capture after the
  PR-029 deletion-overlay restore proof passes. Before that gate, no public
  form or success result is served.
- **Theme:** `preset_id` is one of `minimal`, `bold`, `elegant`, `playful`, or
  `corporate`. `overrides` is a closed partial record: every listed property is
  optional, omitted properties inherit from the selected preset, and unknown
  properties fail. `colors` is itself an optional partial record containing
  only `color-background`, `color-border`, `color-muted`, `color-on-primary`,
  `color-primary`, `color-surface`, and `color-text`; each supplied value is an
  opaque `#RRGGBB` string. `type_scale` is `compact` (0.875), `default` (1),
  `large` (1.125), or `display` (1.25), and overrides `font-scale`.
  `font_family` is `system_ui`, `arial`, `georgia`, `verdana`, or
  `trebuchet_ms`, mapped respectively to `system-ui, sans-serif`,
  `Arial, sans-serif`, `Georgia, serif`, `Verdana, sans-serif`, and
  `Trebuchet MS, sans-serif`; it overrides `font-family`.
  `spacing_scale` is `compact` (3rem), `default` (4rem), `relaxed` (5rem),
  `airy` (4.5rem), or `spacious` (6rem), and overrides `spacing-section`.
  `corner_scale` is `square` (0), `soft` (0.375rem), `rounded` (0.75rem),
  `large` (1rem), or `extra_large` (1.5rem), and overrides `radius-card`.
  `content_width` is `narrow` (56rem), `default` (72rem), or `wide` (80rem),
  and overrides `content-width`. `button_shape` is `square` (0),
  `rounded` (0.375rem), `soft` (0.75rem), or `pill` (9999px), and overrides
  `radius-button`. `shadow` is `none`, `subtle`, or `raised`, and overrides
  `shadow-card` using the fixed values below. `responsive` is an optional
  record with optional `tablet` and `mobile` partial override records using
  the same property shapes as `overrides`, except they cannot contain
  `responsive`; nested buckets fail. Unknown buckets and properties fail.
  Desktop resolves preset then base overrides; tablet and mobile each
  independently resolve preset, base overrides, then their own partial
  override (tablet does not flow into mobile). Mobile is
  0–639 CSS px, tablet is 640–1023 CSS px, and desktop is 1024 CSS px and
  above. Responsive changes are renderer-owned tokens, never client CSS. The
  V1 `ThemeOverrides` shape is unchanged.

### Theme profile v1

`theme_profile_version` is exactly `theme-profile-v1`. The preset defaults below
are closed maps of CSS token name to CSS value and are part of this contract;
they preserve the existing built-in palette, font, spacing, radius, and shadow
values. `content-width` defaults to `72rem` for every preset. Color values are
case-sensitive strings. Font stacks are the exact comma-separated strings
listed above.

| Preset | Background | Border | Muted | On primary | Primary | Surface | Text | Font | Font scale | Button radius | Card radius | Card shadow | Section spacing |
|:--|:--|:--|:--|:--|:--|:--|:--|:--|--:|:--|:--|:--|:--|
| `minimal` | `#f8fafc` | `#e2e8f0` | `#64748b` | `#ffffff` | `#334155` | `#ffffff` | `#1f2937` | `system-ui, sans-serif` | `1` | `0.375rem` | `0.75rem` | `0 1px 3px rgba(15, 23, 42, 0.08)` | `4rem` |
| `bold` | `#111113` | `#3f3f46` | `#a1a1aa` | `#1c1917` | `#f97316` | `#18181b` | `#fafafa` | `Arial, sans-serif` | `1.08` | `0` | `0` | `0 8px 32px rgba(0, 0, 0, 0.32)` | `6rem` |
| `elegant` | `#faf7f2` | `#e8ded1` | `#7c6f64` | `#fffaf5` | `#8b5e4c` | `#fffdf9` | `#3b3028` | `Georgia, serif` | `1.04` | `0.75rem` | `1rem` | `0 4px 20px rgba(59, 48, 40, 0.08)` | `5rem` |
| `playful` | `#fff7fd` | `#f0d8fb` | `#715a81` | `#ffffff` | `#d946ef` | `#ffffff` | `#38204d` | `system-ui, sans-serif` | `1.05` | `1.5rem` | `1.5rem` | `0 8px 24px rgba(192, 38, 211, 0.18)` | `4.5rem` |
| `corporate` | `#f1f5f9` | `#cbd5e1` | `#475569` | `#ffffff` | `#2563eb` | `#ffffff` | `#0f172a` | `Arial, sans-serif` | `1` | `0.375rem` | `0.375rem` | `0 2px 8px rgba(15, 23, 42, 0.08)` | `4rem` |

The `shadow` override maps `none` to `none`, `subtle` to
`0 1px 3px rgba(15, 23, 42, 0.08)`, and `raised` to
`0 8px 32px rgba(0, 0, 0, 0.32)`. `font-scale` is a decimal CSS multiplier;
scale override values serialize as `0.875`, `1`, `1.125`, or `1.25`.
`responsive` buckets resolve to the complete token map, not a sparse map.
The effective map has exactly these string-valued keys:
`color-background`, `color-border`, `color-muted`, `color-on-primary`,
`color-primary`, `color-surface`, `color-text`, `font-family`, `font-scale`,
`radius-button`, `radius-card`, `shadow-card`, `spacing-section`, and
`content-width`. The hashed object is exactly
`{"profile_version":"theme-profile-v1","desktop":<complete-map>,"tablet":<complete-map>,"mobile":<complete-map>}`;
each responsive map is independently resolved using the rules above. RFC 8785
JCS determines serialized property order. No preset ID or unrecognized token
is added to this object.

## Bounds and validation

- Maximum document nesting depth is 8, counting the root object as depth 1.
- Every V2 document has at most 50 pages, 100 sections total, 5,000 blocks,
  100 forms, and 50 fields per form, subject to the stricter byte caps below.
- The staged raw request is at most 1,048,576 bytes.
- The persisted `data_json` is the UTF-8 RFC 8785 JSON Canonicalization Scheme
  (JCS) serialization of the validated typed document and is at most 1,048,576
  bytes. This is the V2 meaning of the existing per-document ceiling; it
  excludes the manifest and row metadata.
- The serialized V2 render manifest is at most 16,384 UTF-8 bytes. The encoded
  complete revision row, including document, manifest, IDs, and timestamps, is
  at most 1,100,000 bytes. This application limit leaves headroom below D1's
  documented 2,000,000-byte row/value ceiling; implementations must measure
  encoded bytes, not character count.
- Unknown properties and duplicate JSON object keys are rejected before typed
  construction. The domain validator checks ID uniqueness, slug collisions,
  typed references, entry-page uniqueness, graph acyclicity/reachability, and
  all limits after parsing. Schema validation alone is not graph validation.
- JSON numbers are finite safe integers only. No NaN, infinity, fractional
  values, arbitrary precision numbers, or Unicode normalization is accepted.
  Strings are preserved as Unicode scalar values and JCS performs its defined
  UTF-16 property-name ordering and ECMAScript primitive serialization.

## Manifest and digests

`RenderManifestV2` is a separate closed record with exactly these fields:
`manifest_type = "eezfunnel.render-manifest"`, `manifest_version = 2`,
`schema_version = 2`, `revision_id`, `renderer_version`,
`document_validator_version = "funnel-document-v2-validator-v1"`,
`canonicalizer_profile = "rfc8785-jcs-v1"`,
`theme_profile_version = "theme-profile-v1"`, `theme_tokens_sha256`,
`document_sha256`, and `assets_manifest_sha256`. It has no self-digest field.
Every digest is lowercase hexadecimal SHA-256.

`revision_id` is a 1–64 byte ASCII identifier matching
`[A-Za-z0-9][A-Za-z0-9_-]{0,63}` and must equal the immutable revision row ID.
For a V2 manifest, `renderer_version` is exactly `renderer-v3`; the V2 runtime
registry accepts that exact value and no prefix/range matches. PR-024 must
implement and test the `renderer-v3` V2 dispatch before V2 writes are enabled.
An unknown renderer version fails closed. Any later renderer version needs a
reviewed registry entry that states which structural schema versions it can
read. Existing V1 renderer dispatch and manifests remain unchanged.

- `raw_upload_sha256` is SHA-256 over the exact received HTTP body octets,
  encoded as 64 lowercase hexadecimal characters. It protects transfer
  integrity and is never the stored document identity.
- `document_sha256` is SHA-256 over the exact UTF-8 JCS bytes persisted as
  `data_json`, including `document_version` and `schema_version`. The trusted
  finalizer recomputes it after parsing, validation, and canonicalization.
- `theme_tokens_sha256` is SHA-256 over the prefix
  `eezfunnel-theme-tokens-v2\0` followed by JCS UTF-8 bytes of the exact
  resolved object defined in Theme profile v1, including all three complete
  breakpoint maps and `profile_version`. Both manifest generation and render
  recompute this value and verify `theme_profile_version`.
- `assets_manifest_sha256` is SHA-256 over the V2 asset-manifest domain prefix
  `eezfunnel-assets-manifest-v2\0` followed by the JCS UTF-8 bytes of records
  `{asset_id, sha256}`, sorted by ASCII `asset_id` bytes. The input contains
  exactly one record for each distinct `asset_id` referenced by an image block
  and no unreferenced assets; repeated uses of one asset are deduplicated.
  `sha256` is the lowercase 64-character digest from the authoritative
  immutable asset metadata defined by the later media contract. Missing
  metadata or conflicting digests for one asset ID fail finalization. Until
  PR-025 provides that metadata and the upload/finalize path, PR-024 rejects
  image-bearing saves/finalization; a placeholder or client-supplied asset
  digest is never accepted. The empty referenced set is hashed as `[]`. The
  V1 asset-manifest bytes and V1 hashes remain exactly as D13 defines them.
- The manifest has no digest of its own. In particular, `document_sha256` is
  not a manifest digest. The renderer checks manifest version, revision ID,
  document digest, theme digest, renderer version, and asset digest before
  using the document.

The canonicalization profile is RFC 8785 JCS over I-JSON-compatible input and
is named `rfc8785-jcs-v1` in the manifest. The document validator is named
`funnel-document-v2-validator-v1` there; any rule change requires a new
validator profile value and review of compatibility with the current schema.
Duplicate keys are rejected before JCS. JCS does not normalize Unicode; equal
visible text with different normalization remains distinct bytes and digest.
The validator and canonicalizer versions must pass RFC 8785 vectors plus
Rust/JavaScript parity fixtures before V2 writes are enabled.

## Compatibility and migration

- Every existing row with `schema_version = 1` continues through the unchanged
  V1 reader. Its first explicit V2 save creates a new immutable V2 revision;
  no in-place rewrite or implicit migration is permitted.
- A V1-to-V2 conversion allocates one entry-page ID and maps every legacy
  section to the same stable section ID and order. It either preserves typed
  content exactly or reports a deterministic conversion error; it never drops
  unsupported data or guesses an edge.
- A V2 row is rejected by a V1-only runtime. A V2-capable runtime must still
  read supported V1 rows. Unknown future versions fail closed.
- Runtime compatibility status, not a JSON field default, chooses a reader.
  Missing `schema_version` remains an error under C-01.

## Out of scope

- Runtime support before PR-024, hosted authentication, or tenant isolation.
- Form collection/submission, contact creation, stored consent choices,
  marketing dispatch, deletion, retention, or legal-compliance claims.
- R2 media upload/publication behavior, custom domains, SEO behavior, and
  arbitrary styling, markup, or script.
- A guarantee that every configured funnel and revision can reach its maximum
  size; aggregate D1 quotas and capacity behavior must be independently
  designed. Immutable revisions are never silently pruned.

## Acceptance tests required before implementation is considered verified

- Exact V1 reader compatibility for both legacy `format_version` 1 and 2;
  missing, mismatched, unknown, and unsupported versions fail closed.
- V2 exact row/document/manifest version agreement, stored funnel/revision
  identity binding, exact renderer dispatch, and rejection of unknown
  variants/properties.
- Stable-ID uniqueness and all typed references; duplicate page, section,
  block, form, and field IDs fail.
- Duplicate route slug, missing/duplicate entry, unreachable page, cyclic
  page graph, invalid target, unused/duplicate-per-page form placement,
  invalid URL spelling, and wrong action target fail. A `success_page` result
  contributes a potential reachability edge and can introduce a cycle; an
  `inline` result adds no edge.
- Malformed JSON, duplicate keys, invalid Unicode, excessive depth, unsafe
  numeric values, and unknown node tags fail without a panic.
- Byte boundaries at 1,048,575; 1,048,576; and 1,048,577 bytes, including
  multibyte UTF-8; manifest and complete-row headroom boundaries are tested
  independently.
- RFC 8785 vectors and Rust/JavaScript digest parity for escaped and
  non-ASCII keys, Unicode normalization differences, and number edges.
- Repeated image references contribute exactly one asset-manifest record;
  unreferenced assets are excluded, and missing or conflicting authoritative
  asset digests fail. PR-024 rejects image-bearing saves until PR-025 enables
  them with immutable metadata.
- Forms reject unknown fields, invalid purpose/copy versions, broken result
  targets, invalid help/option bounds, invalid per-kind validation, duplicate
  field names or IDs, and generic fields named `request_consent` or
  `marketing_opt_in`. Negative fixtures include a text field
  `{"id":"field-request-consent","name":"request_consent","label":"Message","kind":"text","required":false}`
  and a checkbox field
  `{"id":"field-marketing-opt-in","name":"marketing_opt_in","label":"Updates","kind":"checkbox","required":false}`;
  each is otherwise valid but rejected for using a built-in role key. Public
  form rendering stays disabled until
  PR-030 and the PR-029 restore-surviving deletion gate pass; PR-028 preview is
  inert and has no network effects.
- PR-028 preview renders the built-in request and optional marketing controls
  under stable role keys. PR-029/030 consent evidence binds each submitted
  choice to the exact form, revision, source page, and copy version; request
  handling must be affirmative, and marketing opt-in remains distinct.
- V1 migration is explicit and deterministic; old immutable revisions and
  their existing hashes remain unchanged.
