# PR-015 throughput checkpoint

- **Blocking first steps:** map the current `Session::execute` routes, their idempotency-key rules, and the C-04 error mapping before changing dispatch.
- **Independent workstreams:** n/a; the registry, dispatcher, API caller, and dispatch tests share one command inventory and must change together.
- **Shared mutable state:** `CommandRegistration` is the single source for supported IDs, actor access, idempotency policy, help text, and handler selection; API adapters pass the actor but do not keep a second command list.
- **Smallest safe decomposition:** one implementation owner for the closed registry and its callers; splitting these writes would risk a partial command table.

## PR-015 scope decision

The production plan says “15 commands” but does not list those IDs or define the extra section operations. The current executable dispatcher has nine stable IDs. `theme.set` is a pure revision transformation, and section edits are persisted through `funnel.revision.save`; neither is currently an independent `Session::execute` operation. The registry will cover the nine executable commands without advertising placeholders or introducing competing write paths.
