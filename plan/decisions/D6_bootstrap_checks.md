# D6 Bootstrap checks

Status: accepted for PR-000 implementation.

Synthesis selects Cargo metadata candidate B after an independent cross-judge. The negative and live Cargo fixture tests come from candidate A; its human-readable tree parser was rejected as a weaker boundary. Design and review agents use the inherited model, so this comparison does not claim model diversity.

The eight frozen architecture units become empty buildable packages. Product packages declare their layer; xtask declares the tool role. The checker also verifies these declarations against the existing architecture so changing metadata cannot legalize an upward dependency. Any added unit requires an explicit architecture decision before validation succeeds.

The minimal xtask exposes check and layers. Check runs format, Clippy with warnings denied, workspace tests, and layer enforcement. CI, audit, deployment, and application behavior remain separate work.

Activate the already declared workspace serde_json dependency only in xtask. Cargo metadata is the supported JSON boundary for membership and all declared dependency kinds, including optional and target-specific dependencies. A standard-library string parser cannot safely implement JSON, and reproducing Cargo TOML rules would create bypasses. No application dependency is activated and the root dependency declarations remain unchanged. Cargo.lock freezes the resolved tool dependencies.

Canonical dependency paths identify workspace edges even when renamed. Workspace-named dependencies without an explicit path fail closed because registry patches can redirect them to local members. External packages with such names are also rejected as ambiguous until an explicit path or a separate architecture decision resolves their identity. All path dependencies that do not identify classified members are rejected, including paths outside the workspace.

The checker uses full Cargo metadata with --all-features and --locked. Every resolved local package (source null) must be a classified workspace member, regardless of whether its declaration looks like a registry dependency. This rejects excluded helpers redirected by registry patches, including optional and target-specific aliases; --no-deps omitted these helpers and allowed a forbidden workspace backdoor. Resolution uses the existing lockfile without updating it. Synthetic live fixtures generate their own lockfiles offline before invoking the same locked production reader. Declared workspace edges still cover every dependency kind. Standard transitive registry graphs are not layer-modeled; the local-package check closes the patch-helper bypass without introducing a TOML parser or another dependency.

All product-to-product edges must strictly descend the layers. Product-to-tool edges are forbidden. Tool callers are exempt. Dev, build, optional, and conditional declarations follow the same rule. Empty libraries and CLI bootstrap help assert no implemented product behavior. WASM compilation establishes dashboard packaging only.
