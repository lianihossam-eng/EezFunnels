# D12 - Dashboard rendering and hydration

Status: accepted for PR-008.

The shared Dioxus route tree and SSR renderer live in `eez-dashboard-presentation`, a rendering
package classified in L1. It produces escaped HTML from validated route paths and static UI content;
it has no dependency on commands, storage, or the dashboard application. The `eez-dashboard`
package remains the L4 browser/WASM entry point, and the L4 Worker depends directly on the L1
renderer rather than on the dashboard app.
The Worker renders only `/`, `/funnels`, and `/funnels/:id`; a narrow
`render_document(path)` entry point returns `None` for every unowned path so API, probe, MCP, and
not-found behavior stays in the existing Worker router. Funnel route IDs are limited to 64 ASCII
letters, digits, underscores, or hyphens. Dioxus owns HTML escaping for route content.

The browser bundle hydrates that same tree. The Worker emits Dioxus's serialized hydration data in
a JSON script element, and the bootstrap validates the base64 data before assigning the globals
read by `dioxus-web`. Development responses include Dioxus's debug hydration arrays; release
responses include only the serialized data. SSR places the rendered app tree inside an outer
`<div id="main">` mount container, matching `dioxus-web`'s `rootname("main")` behavior. The app
tree itself is the container's child, and page content uses a semantic `main` landmark.

Dioxus 0.7.10 is pinned with default features disabled and only `minimal`, `router`, and `document`
enabled in the workspace dependency. The L1 presentation package's `ssr` feature enables Worker
rendering; browser support is an explicit feature on the L4 dashboard entry and the L1 presentation
package, so the bundle does not activate server rendering. The selected feature sets build with the
workspace's Rust 1.85 toolchain. `web-sys/Location` is enabled for the browser router's use of the
current URL.

`cargo xtask dashboard-assets` builds the browser feature for `wasm32-unknown-unknown`, invokes the
pinned wasm-bindgen CLI 0.2.129 with a stable output name, then copies the checked-in bootstrap and
stylesheet from `crates/dashboard-presentation/assets` into `apps/dashboard/dist`. The installer
uses the auxiliary Rust 1.88.0 toolchain required by the CLI's locked build graph; the application
and Worker remain on Rust 1.85. Wrangler serves these generated assets through its `ASSETS` binding
with Worker-first routing, after the existing host allowlist. The local smoke exercises routes,
assets, fallback behavior, and host denial without contacting a deployed Worker.

This keeps rendering and hydration in one source tree while preserving the API's route ownership.
No funnel data API, CRUD behavior, or editor state is introduced; the shell uses an honest empty
state until those later phases provide data. PR-008's route, payload, bundle, and local Worker
verification results are fingerprinted in `plan/verification/PR-008/`.
