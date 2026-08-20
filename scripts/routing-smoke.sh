#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT

cargo run --quiet --locked -p ferrodoc -- router inspect . benchmarks/routing/dataset.json \
  >"$scratch/inspect.json"
jq -e '.records == 2 and .source_verification == "passed"' "$scratch/inspect.json" >/dev/null

cargo run --quiet --locked -p ferrodoc -- router train . benchmarks/routing/dataset.json \
  "$scratch/model.json" >"$scratch/train.json"
jq -e '.qualification.status == "rejected" and .confidence == 1' "$scratch/model.json" >/dev/null

cargo run --quiet --locked -p ferrodoc -- router evaluate . benchmarks/routing/dataset.json \
  "$scratch/model.json" >"$scratch/evaluate.json"
cargo run --quiet --locked -p ferrodoc -- router compare . benchmarks/routing/dataset.json \
  "$scratch/model.json" >"$scratch/compare.json"
jq -e '
  .identical_case_sets == true and
  .model_qualification.status == "rejected" and
  ([.baselines[][1].case_ids] | unique | length) == 1
' "$scratch/compare.json" >/dev/null

cargo run --quiet --locked -p ferrodoc-research --example build_experiment_fixture -- \
  "$repo_root" "$scratch/model.json" "$scratch/spec.json"
cargo run --quiet --locked -p ferrodoc -- research run . "$scratch/spec.json" \
  "$scratch/ledger.json" >"$scratch/run.json"
cargo run --quiet --locked -p ferrodoc -- research status "$scratch/ledger.json" \
  >"$scratch/status.json"
jq -e '
  .completed_evaluations == 2 and
  .pending_evaluations == 0 and
  (.pareto_frontier | sort) == ["native-baseline", "tesseract-route"] and
  [.trials[].status.status] == ["mutation_recorded", "evaluation_complete", "evaluation_complete"]
' "$scratch/status.json" >/dev/null

echo "offline routing and research smoke: ok"
