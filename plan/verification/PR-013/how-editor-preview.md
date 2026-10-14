# PR-013 editor preview boundary

## Overview

The editor preview is a server-rendered HTML document inside a sandboxed `srcdoc` iframe. The dashboard owns the surrounding panels and controls; `eez-render` owns the content inside the frame. Responsive preview can therefore change the iframe's layout width entirely in dashboard presentation code.

## State and flow

`FunnelDetail` owns the `EditorState` signal for the current draft. Reorder, inspector edits, undo, and redo capture draft state and schedule a preview request. The response replaces `preview_html` only when its funnel, generation, base revision, order, and settings still match the current draft. Failed requests keep the last good HTML.

Viewport choice is presentation state. It must not affect draft order or settings, undo/redo history, dirty/save state, preview generations, preview request bodies, or the saved revision. It only selects the width of the existing iframe browsing context.

## Layout ownership

The dashboard's own breakpoints control the editor panels. The preview document separately adapts to its iframe viewport: the renderer uses `repeat(auto-fit, minmax(14rem, 1fr))` for features, maximum widths for section and hero content, and wrapping footer links. It does not need a renderer or contract change for this feature.

The existing iframe is `width: 100%` with a border. A fixed viewport must use a horizontally scrollable, width-constrained wrapper so a 1280 px selection remains a real 1280 px browsing context in a narrower preview panel. The iframe's border must not reduce the requested content viewport width. Keep `sandbox=""`, `srcdoc`, the restrictive renderer CSP, inert links, and blocked image requests unchanged.

## Verification seam

The hydrated Playwright editor path is the behavior seam: select 375, 768, and 1280 px; inspect the actual frame viewport and responsive rendered geometry; verify selected state is keyboard reachable; and prove the choice does not change dirty/history state or emit preview/save requests. Keep the existing isolation and axe checks. A deterministic layout snapshot is suitable because the harness has no screenshot golden infrastructure; the snapshot should record measured browser and iframe geometry, not values recomputed from the selected control alone.

## Relevant sources

- Editor preview component: `crates/dashboard-presentation/src/routes/editor.rs`, `FunnelDetail`.
- Dashboard workspace and iframe styles: `crates/dashboard-presentation/assets/dashboard.css`.
- Intrinsic responsive funnel layout: `crates/render/src/lib.rs`, `THEME_STYLESHEET`.
- Hydrated browser seam and security checks: `scripts/test-pr010-browser.mjs`.
- Security and preview invariants: `plan/decisions/D15_visual_editor.md` and frozen `plan/contracts/C-03_security.md`.
