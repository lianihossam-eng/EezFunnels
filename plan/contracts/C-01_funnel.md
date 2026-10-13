# C-01 - Funnel Schema Contract

## Objective

Define the structure of a funnel and its compatibility rules.

## Invariants

- A funnel has four identifiers:
  - funnel_id: stable identity
  - revision_id: immutable snapshot
  - schema_version: structural contract version
  - published_revision_id: revision served publicly
- An edit produces a new revision_id. No silent mutation.
- Publishing produces a new published revision. Previous remains stored.
- Compatibility: renderer at schema_version = N reads N and older supported.
  deprecated triggers migration. unsupported rejected.
- Publication is atomic.
- Publication produces render_manifest (revision, theme version, renderer
  version, assets manifest hash, config hash).

## Negative scenarios

- Unknown section type -> validation/section_type_unknown
- Section without ID -> validation/section_missing_id
- Duplicate order -> validation/order_duplicate
- Missing schema_version -> validation/schema_version_missing
- Unsupported schema_version -> validation/schema_version_unsupported
- Concurrent conditional publication -> one succeeds, others conflict/publish_stale_revision
- Concurrent edit on same revision -> conflict, no silent overwrite

## Acceptance

- Snapshot: revision renders identically.
- Atomic publication: 100 concurrent publications coherent.
- Compatibility: supported versions render.
- Limits reject cleanly.

## Out of scope

- Serialization format.
- Storage.
- Exact limit values (see C-01_config).
