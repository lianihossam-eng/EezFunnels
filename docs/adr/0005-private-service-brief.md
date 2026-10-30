# Keep the service brief separate and current-state only

Status: accepted for the public improvement series.

## Problem

The public funnel document is an immutable, versioned snapshot used for rendering, publication, drafts, and reports. The owner also needs private, potentially incomplete service-planning notes that can change independently, while avoiding lost updates and accidental inclusion in public projections.

## Usage (caller's view)

The dashboard reads a funnel's private brief and saves it with the modification version it loaded. A save against the current version advances that version; a retry of the same desired contents succeeds without another change if those contents are still current. A different newer brief yields a conflict that preserves the owner's local edits until they explicitly load or replace the current value.

## Shape

`eez-domain` owns the closed service-intention type and bounded, optionally incomplete brief. `eez-storage` owns a dedicated D1 repository and table keyed by funnel, with a format version and a separate modification version. An absent row means a version-zero, empty brief. A conditional write is atomic; it does not create a public revision, update the editor head, or retain prior brief text.

`eez-commands` provides typed read and save operations directly to the owner HTTP adapter without adding them to the CLI or MCP registry. `apps/api` validates the request, checks a supplied origin, maps version conflicts to 409, and marks every response private and non-cacheable. The dashboard has its own state and route, separate from the public document editor and local public drafts. This keeps a small read/save interface while the storage adapter contains the SQL and conflict rules.

## Synthesis decision

The architect arena's candidate B is the base: current state plus compare-and-swap, with an explicit undecided intention and no private history. The cross-judge scored A/B/C respectively 5/5/5 for privacy, 4/4/5 for concurrency, 5/4/5 for uncertain retries, 4/5/4 for interface/layer fit, and 4/5/4 for complexity and verification. Candidate B's write must allow both first insert at version zero and updates at a positive expected version in the same atomic statement; both paths receive concurrency tests. From candidates A and C, the HTTP adapter is isolated from `apps/api/src/lib.rs`, and origin plus no-store behavior covers errors as well as successful responses. Their digest receipts are not adopted: PUB-005 needs safe target-state retries, not historical operation acknowledgement, and storing a digest of private notes adds retained state without removing a required failure mode.

## Tradeoffs accepted

- The table keeps only the latest brief; past versions cannot be recovered. This avoids retaining superseded private notes and avoids inventing private-history retention semantics.
- An uncertain retry returns a conflict if a different brief has since replaced its target. The dashboard keeps local edits and offers explicit load-latest or save-over-latest actions instead of overwriting newer data.
- The intention can remain unset while the owner is drafting. This avoids silently treating contact as the owner's chosen path; publication readiness is handled separately by PUB-006.
- Owner access still depends on the installation's configured network perimeter. This feature does not introduce application accounts or roles.

## Alternatives considered

An append-only brief revision log would retain every confidential edit and require history, deletion, and retention behavior outside this task. It offers more interface surface than the brief currently needs. A digest-backed receipt per write would acknowledge old operations exactly, but adds a second mutation lifecycle and retains operation digests; a current-state comparison already makes the target PUT safe to retry and keeps a superseded retry from replacing newer contents.

## Open questions and risks

If private brief history becomes a product requirement, what retention, export, deletion, and restoration contract should govern superseded notes before a history table is added?

## Next implementation step

Add the validated domain model and private D1 table, then implement the compare-and-swap repository before wiring the owner API and dashboard route.
