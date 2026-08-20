#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

CARGO_NET_OFFLINE=true cargo test --workspace --locked
status=$(CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- status)
if [[ $status != "Ferrodoc Phase 1 foundation; PDF conversion is not implemented yet" ]]; then
  echo "error: unexpected CLI status: $status" >&2
  exit 1
fi
echo "offline workspace smoke: ok"
