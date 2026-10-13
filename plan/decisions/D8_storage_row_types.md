# D8 - Storage row types

Decision: D1 rows are decoded into typed storage records using the existing
workspace `serde` dependency. PR-003 does not add a new crate or introduce
domain types ahead of PR-004.

The repository owns database row shapes and validates the C-01 schema version
at its storage boundary. JSON structure and domain invariants remain owned by
the domain layer introduced in PR-004.
