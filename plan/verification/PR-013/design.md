# PR-013 responsive preview design

## Problem

The editor already displays responsive server-rendered HTML in an empty-sandbox `srcdoc` iframe. PR-013 needs to control the browsing-context width at 375, 768, and 1280 CSS pixels without adding viewport choice to draft state, undo/redo, preview requests, or saves. The dashboard and renderer have separate responsive layouts, and the iframe's current border plus global `border-box` styling can make the actual viewport smaller than the selected width.

## Usage

`FunnelDetail` passes the current preview document and funnel name to a private responsive preview component:

```rust
ResponsivePreview {
    title: format!("Preview of {}", document.funnel.name),
    srcdoc: document.preview_html.clone(),
}
```

Inside the component, the closed viewport selection changes only the iframe width. The HTML continues to come from the existing guarded preview flow.

## Shape

`PreviewViewport` is a private closed enum with `Mobile`, `Tablet`, and `Desktop` variants mapped to 375, 768, and 1280 pixels. A single component-local signal owns the selection. The component renders a named button group with `aria-pressed` state, a keyboard-focusable horizontally scrollable canvas, and the existing titled `iframe` with an empty sandbox and `srcdoc`.

The iframe is borderless and receives the enum's exact width; the canvas owns border decoration and horizontal overflow. It has `min-width: 0` and `max-width: 100%` while the iframe has no max-width clamp or scaling transform, so a 1280 px selection remains 1280 CSS pixels even when the dashboard panel is narrower. A 1280 px selection may scroll inside the canvas without widening the page.

Viewport state does not enter `EditorState`, `EditorDraftSnapshot`, `EditorPreviewCapture`, local storage, URLs, or API parameters. A refreshed `srcdoc` preserves the selected viewport while the component remains mounted. The existing security boundary and renderer output remain unchanged.

## Synthesis decision

Both candidates and the cross-judge converged on a private component with local viewport state. Candidate A is the base because it keeps the current preview presentation policy behind a small route-private boundary. Candidate B contributes borderless exact sizing and a concrete layout snapshot. The cross-judge caught that a parent-page read of `contentWindow`/`contentDocument` is invalid for the empty-sandbox opaque-origin frame; browser tests will query through Playwright's frame execution context instead.

The candidates did not produce structurally distinct final shapes. Given the single presentation boundary and the explicit instruction to keep this PR narrow, implementation uses the shared component design rather than introducing a new module or editor-state field merely to create a different shape.

## Responsive snapshot contract

The hydrated browser test records for each choice: the selected accessible button, outer iframe client width, `window.innerWidth` from inside the frame, the funnel document width, active feature-grid column count and width, first feature-card width, and section order. The existing tripwire fixture contains two feature cards, so the active columns are 1 at 375 px and 2 at 768 and 1280 px; measured card widths demonstrate the reflow. The test also checks that the 1280 px canvas contains horizontal overflow while the outer dashboard page does not.

The repository uses a direct Playwright browser script without an image-comparison lifecycle. Structured measured layout snapshots provide a deterministic regression gate here; PNGs are not claimed as pixel-comparison snapshots.

## Tradeoffs accepted

- We accept horizontal scrolling within the preview canvas in exchange for an exact device-width browsing context inside the existing editor layout.
- We accept a private component in the existing route module in exchange for a clear owner without a new one-component module.
- We accept resetting selection when the component unmounts in exchange for keeping this display preference out of persistence and document state.

## Alternatives considered

- **Put viewport in `EditorState` and render controls inline.** Rejected because it exposes transient display state to history, reset, preview, and save flows; the public surface hides less policy.
- **Use CSS-only radio state.** Rejected because width choices must be coordinated between markup and CSS and the selected width has no typed Rust source of truth.
- **Resize the outer browser viewport.** Rejected because it couples dashboard breakpoints to preview breakpoints and cannot show a 1280 px page while keeping the editor usable in a narrow viewport.

## Implementation result

The component and browser regression implement these boundaries. All three measurements pass: the iframe browsing context is exactly 375, 768, or 1280 px; the funnel's feature grid reflows from one active column to two; selecting a viewport does not change editor history or issue preview/save requests; and the narrow outer dashboard page does not grow when the 1280 px preview scrolls inside its canvas. A preview refresh keeps the selected width while the component remains mounted. The existing empty iframe sandbox, title, and `srcdoc` behavior remain intact.
