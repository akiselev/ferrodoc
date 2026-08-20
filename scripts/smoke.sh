#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

CARGO_NET_OFFLINE=true cargo test --locked -p ferrodoc-core --lib
echo "offline core smoke: ok"
