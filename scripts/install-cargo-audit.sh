#!/usr/bin/env bash
set -euo pipefail

toolchain=1.88.0
version=0.22.2

if cargo audit --version 2>/dev/null | grep -Eq '(^| )0\.22\.2$'; then
  exit 0
fi

rustup toolchain install "$toolchain" --profile minimal
cargo "+$toolchain" install cargo-audit --version "$version" --locked --force
