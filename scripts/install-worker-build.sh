#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
source .project-tools/env.sh
rustup toolchain install 1.88.0 --profile minimal
if [ ! -x "$CARGO_HOME/bin/worker-build" ] || [ "$("$CARGO_HOME/bin/worker-build" --version)" != "0.8.3" ]; then
  cargo +1.88.0 install --locked --force --version 0.8.3 worker-build --root "$CARGO_HOME"
fi
worker-build --version
