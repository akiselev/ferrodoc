# Benchmarking and corpus governance

Ferrodoc benchmarks are integrity checks and experiment records, not a single leaderboard number. A report binds the exact corpus, candidate engine/version, optional model digest, normalized configuration digest, toolchain, metric version, and metric thresholds. Quality, failures, latency, memory, VRAM, and cost remain separate Pareto dimensions.

## Corpus types

`ferrodoc-foundry` deterministically generates PDFs and provenance-bearing truth from a versioned JSON specification. A specification declares its seed, every asset and license, redistribution status, degradations, partition, text, regions, reading order, tables, and formulas. The corpus manifest binds those inputs and every document/truth digest. Running the same generator version and specification must produce byte-identical directories.

The checked `benchmarks/foundry-smoke.json` suite exercises native, seeded raster, rotated, Unicode, reading-order, table, and formula paths. It is small enough for default CI.

Synthetic data is not the sole regression authority. `benchmarks/real-regression/manifest.json` binds the purpose-built born-digital and scanned PDF fixtures. Its truth is versioned under `benchmarks/real-regression/truth/`, and its source-generator digest and license are recorded as assets. This corpus is regenerated explicitly by:

```bash
cargo run --locked -p ferrodoc-pdf --example generate_fixtures
cargo run --locked -p ferrodoc-bench --example export_real_regression
```

## Held-out policy

Case identity is derived from semantic truth independently of partition and degradation, so the same case cannot occur in train, tuning, held-out, or regression partitions. Training and broad exploration use `train`; parameter or policy selection uses `tuning`; stable defect checks use `regression`.

Held-out truth is sealed from engine development, prompt/configuration selection, router training, and manual error-driven tuning. Only the designated evaluation workflow may expose held-out aggregate and category results. The evaluator self-test oracle refuses any manifest containing a held-out case. A held-out case that is inspected must be retired to regression or tuning and replaced before subsequent claims.

Foundry seeds and benchmark manifests are experiment provenance. Do not alter a fixed suite in place to improve a candidate. Create a new manifest identity and compare both results. Foundry or fixture assets must have an explicit source, license, redistribution status, and digest when bytes are not defined by the PDF standard.

## Evaluation integrity

`ferrodoc-bench` verifies the complete corpus before scoring. Empty suites, duplicate or extra cases, incompatible schema/corpus/metric identities, invalid truth, and mismatched comparison case sets are errors. Every manifest case appears in a report. An omitted or failed conversion counts as failure and contributes zero quality; a skip requires a visible exclusion and is never silently treated as success.

Metrics include Unicode NFKC-normalized CER/WER, reading-order edge F1, one-to-one region assignment, category-specific IoU thresholds, per-category region/text quality, spatially associated table structure and cell semantics, spatially associated normalized LaTeX token similarity, and exact checks on regression fixtures. Metric behavior changes require a new `METRIC_VERSION`; threshold changes remain explicit in each report and make reports comparison-incompatible.

Resource values use tagged evidence:

- `measured`: direct observation with method;
- `estimated`: a defensible estimate with method;
- `unknown`: no value is available, with reason.

Unknown RAM, VRAM, residency, load time, or cost never becomes zero. Timing distinguishes the first cold sample from repeated warm samples and retains count, min, mean, max, and population standard deviation. The measurement helper observes wall time, process CPU time, and process-lifetime peak RSS. Engines or runners attach optional device VRAM, model-load time, warm residency, and remote cost evidence when available.

Comparison is policy-specific Pareto analysis across quality, throughput, latency, RAM, VRAM, remote cost, and failure rate. Required unknown dimensions make dominance indeterminate. A candidate that omits a baseline success cannot dominate it. Per-case status and quality deltas plus repeated-sample variance notes remain in the comparison artifact.

## Commands

Generate and verify a synthetic suite:

```bash
corpus_dir=$(mktemp -d)/corpus
cargo run --locked -p ferrodoc-foundry -- generate benchmarks/foundry-smoke.json "$corpus_dir"
cargo run --locked -p ferrodoc-foundry -- verify "$corpus_dir" "$corpus_dir/manifest.json"
```

Evaluate candidate predictions, validate a report, and compare compatible reports:

```bash
cargo run --locked -p ferrodoc-bench -- evaluate CORPUS_ROOT MANIFEST.json PREDICTIONS.json REPORT.json
cargo run --locked -p ferrodoc-bench -- verify-report REPORT.json
cargo run --locked -p ferrodoc-bench -- compare BASELINE.json CANDIDATE.json COMPARISON.json
```

`oracle_predictions` copies visible truth only to test evaluator mathematics and case accounting. It is prohibited as engine evidence, performance evidence, or a benchmark result. Default CI runs `./scripts/benchmark-smoke.sh`, including byte reproducibility, synthetic and real suites, empty-work rejection, unknown-resource preservation, and report validation.

Checked JSON Schemas are regenerated by the `export_foundry_schemas` and `export_benchmark_schemas` examples. CI regenerates all corpus metadata and schemas and rejects any diff.
