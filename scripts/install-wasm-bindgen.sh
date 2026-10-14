#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

toolchain=1.88.0
version=0.2.129
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
wasm_bindgen="$cargo_home/bin/wasm-bindgen"

if ! command -v rustup >/dev/null 2>&1; then
  printf 'rustup is required to install wasm-bindgen-cli %s with Rust %s\n' "$version" "$toolchain" >&2
  exit 1
fi

rustup toolchain install "$toolchain" --profile minimal

installed_version=""
if [ -x "$wasm_bindgen" ]; then
  installed_version="$("$wasm_bindgen" --version 2>/dev/null || true)"
fi

if [ "$installed_version" != "wasm-bindgen $version" ]; then
  cargo "+$toolchain" install wasm-bindgen-cli --version "$version" --locked --force --root "$cargo_home"
fi

if [ "$("$wasm_bindgen" --version)" != "wasm-bindgen $version" ]; then
  printf 'expected wasm-bindgen %s at %s/bin/wasm-bindgen\n' "$version" "$cargo_home" >&2
  exit 1
fi

printf '%s\n' "$wasm_bindgen $version"
