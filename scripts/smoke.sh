#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

CARGO_NET_OFFLINE=true cargo test --workspace --locked
reported_status=$(CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- status)
if [[ $reported_status != "Ferrodoc Phase 4 resource-aware runtime" ]]; then
  echo "error: unexpected CLI status: $reported_status" >&2
  exit 1
fi
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- --version >/dev/null
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  inspect fixtures/pdf/born-digital.pdf >/dev/null
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  plan fixtures/pdf/born-digital.pdf >/dev/null
smoke_output=$(mktemp)
smoke_state=$(mktemp -d)
trap 'rm -f -- "$smoke_output"; rm -rf -- "$smoke_state"' EXIT
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  convert fixtures/pdf/born-digital.pdf --format markdown --output "$smoke_output"
cmp fixtures/golden/born-digital.md "$smoke_output"
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  plan fixtures/pdf/image-only.pdf | grep -q '"decision": "unavailable"'
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  explain fixtures/pdf/born-digital.pdf >/dev/null
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- hardware >/dev/null
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  plugins doctor >/dev/null
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  models list --store "$smoke_state/models" >/dev/null
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  explain fixtures/pdf/born-digital.pdf --cache-dir "$smoke_state/cache" \
  | grep -q '"decision": "miss"'
CARGO_NET_OFFLINE=true cargo run --quiet --locked -p ferrodoc -- \
  explain fixtures/pdf/born-digital.pdf --cache-dir "$smoke_state/cache" \
  | grep -q '"decision": "hit"'
echo "offline workspace smoke: ok"
