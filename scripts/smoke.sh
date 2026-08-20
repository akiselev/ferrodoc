#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

CARGO_NET_OFFLINE=true cargo test --workspace --locked
status=$(CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- status)
if [[ $status != "Ferrodoc Phase 2 offline PDF vertical slice" ]]; then
  echo "error: unexpected CLI status: $status" >&2
  exit 1
fi
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- --version >/dev/null
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  inspect fixtures/pdf/born-digital.pdf >/dev/null
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  plan fixtures/pdf/born-digital.pdf >/dev/null
smoke_output=$(mktemp)
trap 'rm -f -- "$smoke_output"' EXIT
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  convert fixtures/pdf/born-digital.pdf --format markdown --output "$smoke_output"
cmp fixtures/golden/born-digital.md "$smoke_output"
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  plan fixtures/pdf/image-only.pdf | grep -q '"decision": "unavailable"'
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  explain fixtures/pdf/born-digital.pdf >/dev/null
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- hardware >/dev/null
echo "offline workspace smoke: ok"
