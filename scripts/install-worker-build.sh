#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
if [ -f .project-tools/env.sh ]; then
  source .project-tools/env.sh
fi
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
rustup toolchain install 1.88.0 --profile minimal
if [ ! -x "$cargo_home/bin/worker-build" ] || [ "$("$cargo_home/bin/worker-build" --version)" != "0.8.3" ]; then
  cargo +1.88.0 install --locked --force --version 0.8.3 worker-build --root "$cargo_home"
fi
"$cargo_home/bin/worker-build" --version
