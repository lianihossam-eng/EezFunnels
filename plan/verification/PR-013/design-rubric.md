# PR-013 responsive preview design rubric

## Artifact

A minimal dashboard-presentation shape for a 375/768/1280 px responsive preview switcher, iframe resizing, and hydrated responsive layout snapshots.

## Rubric

1. The selected number is the iframe's actual CSS browsing-context width at all three options, including 1280 px in a narrower panel.
2. Viewport state is local and transient; it cannot enter draft/history/save/preview state or cause API requests.
3. Existing sandbox, `srcdoc`, CSP, preview generation guards, and renderer ownership stay unchanged.
4. Selection has a clear keyboard-accessible name and selected state without adding custom keyboard behavior that native controls already provide.
5. Browser coverage snapshots measured responsive geometry for all three widths through the hydrated editor path and checks editor state/request invariants.

## Candidate screen

Check each design for split state ownership, duplicated viewport-width lists, presentation state leaking into preview command data, decorative-only scaling that leaves the browsing context unchanged, border width reducing the actual viewport, or new access to iframe internals.
