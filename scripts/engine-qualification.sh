#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ $# -ne 2 ]]; then
  echo "usage: engine-qualification.sh <absolute-ferrodoc-binary> <output-directory>" >&2
  exit 2
fi

ferrodoc_binary=$1
qualification_output=$2
if [[ "$ferrodoc_binary" != /* || ! -x "$ferrodoc_binary" ]]; then
  echo "ferrodoc binary must be an existing absolute executable" >&2
  exit 2
fi
mkdir -p "$qualification_output"

cargo run --quiet --locked -p ferrodoc-bench -- qualify-cli \
  . benchmarks/real-regression/manifest.json "$ferrodoc_binary" ocrs - \
  "$qualification_output/native-without-models.json"
cargo run --quiet --locked -p ferrodoc-bench -- verify-report \
  "$qualification_output/native-without-models.json"
jq -e '
  .aggregate.total_cases == 2 and
  .aggregate.succeeded == 1 and
  .aggregate.failed == 1 and
  .aggregate.peak_ram.status == "unknown"
' "$qualification_output/native-without-models.json" >/dev/null

if [[ -n "${FERRODOC_TEST_OCRS_MODEL_DIR:-}" ]]; then
  cargo run --quiet --locked -p ferrodoc-bench -- qualify-cli \
    . benchmarks/real-regression/manifest.json "$ferrodoc_binary" ocrs \
    "$FERRODOC_TEST_OCRS_MODEL_DIR" "$qualification_output/ocrs.json"
  cargo run --quiet --locked -p ferrodoc-bench -- verify-report \
    "$qualification_output/ocrs.json"
  jq -e '
    .aggregate.total_cases == 2 and
    .aggregate.succeeded == 2 and
    .candidate.model_digest != null and
    .aggregate.peak_ram.status == "unknown"
  ' "$qualification_output/ocrs.json" >/dev/null
fi

if [[ "${FERRODOC_TEST_TESSERACT:-0}" == 1 ]]; then
  cargo run --quiet --locked -p ferrodoc-bench -- qualify-cli \
    . benchmarks/real-regression/manifest.json "$ferrodoc_binary" tesseract - \
    "$qualification_output/tesseract.json"
  cargo run --quiet --locked -p ferrodoc-bench -- verify-report \
    "$qualification_output/tesseract.json"
  jq -e '
    .aggregate.total_cases == 2 and
    .aggregate.succeeded == 2 and
    .candidate.model_digest != null and
    .aggregate.peak_ram.status == "unknown"
  ' "$qualification_output/tesseract.json" >/dev/null
fi

echo "fixed-corpus engine qualification: ok"
