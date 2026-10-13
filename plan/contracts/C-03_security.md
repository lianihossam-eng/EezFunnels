# C-03 - Security Invariants (v0.1.0)

## Objective

Properties true regardless of code path, attacker, or interface.

## Invariants

- Sandbox: no user input can inject script, style, or event handlers.
- Fail-closed: if authz/validation unavailable, deny.
- Idempotence: mutating commands accept idempotency key. Same key + same
  payload -> same effect for 24h. Same key + different payload ->
  conflict/idempotency_payload_mismatch.
- No secret leakage in logs, errors, bodies.
- No uncontrolled panic on production path from untrusted input. Fuzzing
  10 000 cases per boundary.
- /mcp requires Bearer token. Invalid -> 401.

## Negative scenarios

- Malformed JSON -> structured error.
- Authz unavailable -> denial.
- Replay same key + payload -> same response.
- Replay same key + different payload -> conflict.
- Injection -> rejected.
- Script tag in content -> stripped or escaped.
- Invalid Bearer -> 401.
- Fuzz 10 000 inputs -> no panic.

## Acceptance

- Each invariant has a property test.
- Fuzzing 10 000 inputs.
- No secret in errors (scan).
- Script injection fails.

## Out of scope

- Multi-tenant isolation.
- Password policy.
- Specific crypto.
