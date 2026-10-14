# D19 - Closed command registry

Status: accepted for PR-015.

## Problem

`Session::execute` is the shared dispatcher used by the Worker and planned CLI/MCP adapters, but it currently embeds the command inventory, handlers, and required idempotency-key policy in a string `match`. The production plan says there are 15 commands, yet the repository defines nine executable IDs and gives no IDs or behavior for the additional `section.*` operations. `theme.set` is a pure revision transform, while section edits are saved as part of `funnel.revision.save`.

## Caller usage

The Worker declares whether a request runs as the implicit owner or as a public reader, then asks the same session to execute a stable command:

```rust
Session::new(&repository)
    .execute(CommandActor::LocalUser, "funnel.create", parameters, Some(key))
    .await?;

Session::new(&repository)
    .execute(CommandActor::Public, "funnel.public.read", parameters, None)
    .await?;
```

The CLI and MCP can discover the same supported command IDs through `command_registry()` instead of maintaining their own lists.

## Shape

Use one static `CommandDefinition` table. Each entry owns the ID, short description, actor policy, idempotency policy, and a private typed handler selector. `Session::execute` looks up an entry, checks that the supplied actor is allowed, enforces its idempotency policy, then dispatches through the entry's handler. The registry exposes only stable metadata; handler details remain private.

The actor is a closed `CommandActor::{LocalUser, Public}`. Owner operations require `LocalUser`; `funnel.public.read` uses `Public` because the Worker exposes it on the unauthenticated public funnel route. Actor mismatch and unknown IDs use the existing C-04 `validation/operation_not_supported` code. Mutation entries require an idempotency key; read and preview entries do not. The existing typed command parsers, receipt handling, repository transactions, and error mapping remain the owners of their current invariants.

This makes the registry the only inventory and key-policy source. It does not expose storage or HTTP types, change C-01/C-04, add new commands, or split revision saves into section-level writes.

## Synthesis decision

Two candidate shapes were considered. A metadata table beside the current `match` would keep handlers easy to read but hand-sync IDs and idempotency policies. A registry of boxed async closures would combine lookup and dispatch but erase useful handler types and make the table harder to audit. The chosen typed handler selector keeps one inventory, avoids async trait/object complexity, and makes all external IDs visible in one place.

The registry contains the nine executable IDs already present in `Session::execute`: `funnel.list`, `funnel.public.read`, `funnel.publish`, `funnel.unpublish`, `funnel.editor.read`, `funnel.editor.preview`, `funnel.revision.save`, `funnel.create`, and `funnel.delete`. The plan's additional commands remain undefined; placeholder registrations would claim behavior the product does not implement.

## Tradeoffs accepted

- We accept a typed handler match after registry lookup in exchange for keeping handler signatures explicit and free of boxed futures.
- We accept nine registered commands in this phase in exchange for preserving one canonical revision-save path until section-level command semantics are specified.
- We accept a fixed actor/access policy in exchange for modeling the single-user product without introducing authentication.

## Alternatives considered

- Keep the current string match and publish a separate metadata array: this exposes a hand-synced command list and can drift from executable handlers.
- Register the planned 15 IDs with unimplemented handlers: this would make discovery lie and give CLI/MCP callers nonfunctional commands.

## Open questions and risks

The production plan should later name the six additional IDs and their request/result semantics if 15 executable commands remain a release requirement. Until then, the registry reports only real operations.

## Future additions

Add a command only after its ID, actor, parameter shape, result, and idempotency behavior are defined. Register its handler and dispatch tests in the same change.
