# D14 - Bounded workspace funnel list

Status: accepted for PR-009.

The workspace stores at most 100 funnels. This matches the v0.1.0 `max_funnels` config limit and bounds the current dashboard's all-at-once list response and card rendering. `funnel.list` orders by creation time and ID and applies `LIMIT 100`.

Creation claims its C-03 idempotency receipt only while the workspace count is below 100. Receipt claim, funnel row, and initial revision stay in one D1 batch. A create attempted at capacity returns `conflict/funnel_limit_reached` (HTTP 409), and writes neither a funnel nor a receipt. Existing receipt replay is checked first, so a retry of a successful create still returns its stored result at capacity. A database trigger enforces the same limit for other funnel insert paths.

Deleting a funnel frees one workspace slot. Pagination remains out of scope until a later product decision changes the all-at-once dashboard contract.

The local Wrangler smoke seeds 99 rows, creates the 100th through the API, and verifies same-key replay returns that result. A distinct create at capacity returns HTTP 409 with no funnel or receipt; a direct 101st insert is rejected by the trigger. A test-only local overflow row proves the list still returns at most 100. After removing it, deleting a funnel and creating another succeeds, proving deletion releases capacity.
