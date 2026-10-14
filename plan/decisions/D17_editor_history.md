# D17: Editor undo and redo history

**Status:** accepted for PR-012
**Date:** 2026-10-14

Undo and redo belong to the presentation-layer editor draft. They record JSON diffs over the user's editable intent: section order and sparse `settings_changes`. The saved baseline, section selection, preview HTML, request generations, errors, announcements, and server-derived `effective_value` are not history entries. D15's order-permutation, preview-generation, save-capture, retry, stale-revision, idempotency, and editor-head CAS rules remain in force. D16's closed server-owned setting schema and V1/V2 persistence behavior remain unchanged.

The editor retains the latest 100 accepted edits. Undo and redo move a cursor through those diffs; a new edit after undo drops the redo tail. A diff applies only when the current JSON values match its expected side. If that invariant fails, history resets rather than applying a partial or stale patch. Setting history distinguishes an absent sparse override from an explicit JSON `null`.

Each accepted setting mutation and completed pointer or move-button reorder is one history entry. A keyboard reorder is one entry when dropped; Escape restores its starting order and records no entry. Undo and redo restore the local draft and request a preview through the existing generation-guarded path. The live region announces the undo or redo action, names the moved section and resulting position for order diffs, and announces the field label for setting diffs. Effective values remain preview-derived and are not serialized into history or save payloads.

Ctrl+Z undoes and Ctrl+Y redoes when the corresponding transition is available. The shortcuts are handled at the editor boundary so they work from both the inspector and section controls; unrelated modifier combinations and unhandled shortcuts continue through normal browser behavior. The toolbar exposes matching Undo/Redo buttons and `aria-keyshortcuts` metadata.

History is unavailable while loading, saving, retrying an uncertain save, resolving a stale revision, or performing a keyboard reorder. A successful save adopts the returned immutable revision as the new baseline and clears history; a route load also starts with empty history. Definitive validation errors keep the existing draft and its history. Undo/redo do not change any API request, contract, migration, receipt, or CAS behavior.

## Verification

- Presentation tests cover 100 edits, undo 50, redo 50; nullable and missing sparse values; branch truncation; keyboard reorder grouping/cancel; save reset/lock; and retryable/stale locks.
- Browser coverage verifies Ctrl+Z/Ctrl+Y through the hydrated Worker, preview requests, visible settings, and dirty/save state.
- `cargo xtask check`, `cargo xtask ci`, the local migration harness, and the PR-012 browser suite are recorded in `plan/verification/PR-012/`.
