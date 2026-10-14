# PR-013 throughput checkpoint

1. **Blocking first steps.** Confirm the preview state/API boundary and freeze the browser assertions before implementation; run the responsive browser regression once to prove it fails because the selector/resize behavior is absent.
2. **Independent workstreams.** Hydrated browser regression and implementation are distinct files, but the feature has one end-to-end behavior and both need the chosen UI shape. The lead adds the regression first; one implementation worker then changes only the editor component and dashboard stylesheet. Evidence, status, and verification logs remain with the lead.
3. **Shared mutable state.** The iframe width selector is transient component state owned by `FunnelDetail`; no second state copy or server/API writer is introduced. One worker owns the application source edits while the lead owns the test script and later review.
4. **Smallest safe decomposition.** Use one UI implementation worker because editor markup and its local styling are a single presentation slice. The lead keeps the browser harness, evidence, and validation changes separate, then checks the delegated diff against the hydrated behavior seam.
