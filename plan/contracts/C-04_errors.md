# C-04 - Error Contract

## Objective

Stable, transport-independent error taxonomy.

## Invariants

- Every error: code, category, message, correlation_id.
- Categories: validation, authorization, not_found, conflict, rate_limit,
  external, internal, unknown.
- Business code is transport-independent.
- A documented code never changes meaning.
- internal never exposes implementation details.
- unknown reserved for unclassified. Every production unknown alerts.

## Registry

| Code                                    | Category      | HTTP |
|:----------------------------------------|:--------------|:-----|
| validation/invalid_json                 | validation    | 400  |
| validation/missing_field                | validation    | 400  |
| validation/order_duplicate              | validation    | 400  |
| validation/schema_version_unsupported   | validation    | 400  |
| validation/section_type_unknown         | validation    | 400  |
| validation/section_missing_id           | validation    | 400  |
| validation/operation_not_supported      | validation    | 405  |
| authorization/token_invalid             | authorization | 401  |
| authorization/token_missing             | authorization | 401  |
| authorization/host_not_allowed          | authorization | 403  |
| not_found/funnel                        | not_found     | 404  |
| not_found/slug                          | not_found     | 404  |
| not_found/revision                      | not_found     | 404  |
| not_found/route                         | not_found     | 404  |
| conflict/publish_stale_revision         | conflict      | 409  |
| conflict/idempotency_payload_mismatch   | conflict      | 409  |
| rate_limit/exceeded                     | rate_limit    | 429  |
| external/timeout                        | external      | 504  |
| external/unavailable                    | external      | 503  |
| internal/unexpected                     | internal      | 500  |
| unknown/unclassified                    | unknown       | 500  |

## Acceptance

- Every code in at least one test.
- Same condition -> same code.
- internal no stack in response.
- unknown triggers alert.

## Out of scope

- Error library.
- Serialization format.
