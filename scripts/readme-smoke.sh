#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT

cargo metadata --locked --format-version 1 >/dev/null
./scripts/check-workspace.sh
./scripts/check-boundaries.sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
./scripts/smoke.sh
./scripts/benchmark-smoke.sh
./scripts/routing-smoke.sh

cargo run --quiet --locked -p ferrodoc -- --version
cargo run --quiet --locked -p ferrodoc -- inspect fixtures/pdf/born-digital.pdf >"$scratch/inspect.json"
cargo run --quiet --locked -p ferrodoc -- plan fixtures/pdf/born-digital.pdf >"$scratch/plan.json"
cargo run --quiet --locked -p ferrodoc -- convert fixtures/pdf/born-digital.pdf --output "$scratch/document.md"
cargo run --quiet --locked -p ferrodoc -- explain fixtures/pdf/born-digital.pdf >"$scratch/explain.json"
cargo run --quiet --locked -p ferrodoc -- hardware >"$scratch/hardware.json"
cargo run --quiet --locked -p ferrodoc -- plugins doctor >"$scratch/doctor.json"
cargo run --quiet --locked -p ferrodoc -- models list --store "$scratch/models" >"$scratch/models.json"
cargo run --quiet --locked -p ferrodoc -- router inspect . benchmarks/routing/dataset.json >"$scratch/router.json"

cmp fixtures/golden/born-digital.md "$scratch/document.md"
jq -e '.source_verification == "passed"' "$scratch/router.json" >/dev/null
echo "README command smoke: ok"
