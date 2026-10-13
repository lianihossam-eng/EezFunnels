# D9 - Content-addressed theme tokens

Status: accepted for PR-005.

`ThemeTokens::new` validates and normalizes every token, then computes its version from the
canonical ordered map with SHA-256. `RenderManifest::theme_version` stores that `sha256-v1:` value.
The renderer compares the manifest value with the immutable token map before producing HTML, so a
manifest cannot select different token contents under the same version.

The hash input begins with a versioned domain separator. Each validated token name and value is
separated by NUL bytes, which the token grammar excludes. The `sha2` 0.10 dependency supplies the
SHA-256 implementation; the lockfile fixes the resolved package version. A future canonicalization
change must use a new version prefix.

CSS token values reject URL and `expression` function tokens rather than substrings. The bounded
character grammar excludes CSS escapes, comments, strings, declaration separators, and URL schemes.
This accepts names such as `burlywood` while rejecting resource-loading functions.

The test corpus uses a fixed-seed pseudo-random generator to build variable-length sequences from
32 HTML and 32 CSS attack fragments. It varies fragment order, sequence length, and the insertion
position of a unique marker. The test renders 10,000 distinct IDs and validates 10,000 distinct CSS
inputs, then asserts that every fragment, sequence length, and marker position was covered and that
more than 9,000 fragment sequences are structurally distinct in each set.
