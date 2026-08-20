#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

benchmark_tmp=$(mktemp -d)
trap 'rm -rf -- "$benchmark_tmp"' EXIT

cargo run --quiet --locked -p ferrodoc-foundry -- \
  generate benchmarks/foundry-smoke.json "$benchmark_tmp/synthetic-a" >/dev/null
cargo run --quiet --locked -p ferrodoc-foundry -- \
  generate benchmarks/foundry-smoke.json "$benchmark_tmp/synthetic-b" >/dev/null
diff -qr "$benchmark_tmp/synthetic-a" "$benchmark_tmp/synthetic-b"
cargo run --quiet --locked -p ferrodoc-foundry -- verify \
  "$benchmark_tmp/synthetic-a" "$benchmark_tmp/synthetic-a/manifest.json"

cargo run --quiet --locked -p ferrodoc-bench --example oracle_predictions -- \
  "$benchmark_tmp/synthetic-a" "$benchmark_tmp/synthetic-a/manifest.json" \
  "$benchmark_tmp/oracle.json"
cargo run --quiet --locked -p ferrodoc-bench -- evaluate \
  "$benchmark_tmp/synthetic-a" "$benchmark_tmp/synthetic-a/manifest.json" \
  "$benchmark_tmp/oracle.json" "$benchmark_tmp/oracle-report.json"
cargo run --quiet --locked -p ferrodoc-bench -- verify-report \
  "$benchmark_tmp/oracle-report.json"
jq -e '.aggregate.total_cases == 3 and .aggregate.succeeded == 3 and .aggregate.quality == 1' \
  "$benchmark_tmp/oracle-report.json" >/dev/null

corpus_digest=$(jq -r .corpus_digest "$benchmark_tmp/synthetic-a/manifest.json")
jq -n --arg corpus_digest "$corpus_digest" '{
  schema_version: {major: 1, minor: 0},
  corpus_digest: $corpus_digest,
  candidate: {
    engine_id: "integrity.missing-work-self-test",
    engine_version: "1",
    model_digest: null,
    configuration_digest: "0000000000000000000000000000000000000000000000000000000000000000",
    toolchain: "none"
  },
  cases: []
}' > "$benchmark_tmp/missing.json"
cargo run --quiet --locked -p ferrodoc-bench -- evaluate \
  "$benchmark_tmp/synthetic-a" "$benchmark_tmp/synthetic-a/manifest.json" \
  "$benchmark_tmp/missing.json" "$benchmark_tmp/missing-report.json"
jq -e '.aggregate.total_cases == 3 and .aggregate.failed == 3 and .aggregate.quality == 0' \
  "$benchmark_tmp/missing-report.json" >/dev/null

cargo run --quiet --locked -p ferrodoc-bench -- compare \
  "$benchmark_tmp/oracle-report.json" "$benchmark_tmp/oracle-report.json" \
  "$benchmark_tmp/comparison.json"
jq -e '.dominance == "indeterminate" and ([.dimension_deltas[].status] | index("unknown") != null)' \
  "$benchmark_tmp/comparison.json" >/dev/null

cargo run --quiet --locked -p ferrodoc-bench --example oracle_predictions -- \
  . benchmarks/real-regression/manifest.json "$benchmark_tmp/real-oracle.json"
cargo run --quiet --locked -p ferrodoc-foundry -- verify \
  . benchmarks/real-regression/manifest.json
cargo run --quiet --locked -p ferrodoc-bench -- evaluate \
  . benchmarks/real-regression/manifest.json "$benchmark_tmp/real-oracle.json" \
  "$benchmark_tmp/real-report.json"
jq -e '.aggregate.total_cases == 2 and .aggregate.succeeded == 2 and .aggregate.quality == 1' \
  "$benchmark_tmp/real-report.json" >/dev/null

echo "offline benchmark smoke: ok"
