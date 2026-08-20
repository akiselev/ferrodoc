# Phase 5 validation evidence

Baseline: `b9e1bcf`
Branch: `phase-5/trustworthy-benchmarks`

## Goal and architecture changes

Phase 5 establishes a benchmark loop that cannot reward missing work or collapse absent measurements to zero.

- `ferrodoc-foundry` generates byte-reproducible native, rotated, and seeded raster-only PDFs with versioned text, region, reading-order, table, formula, asset/license, degradation, and provenance truth.
- Corpus manifests bind the generator, specification, seed, assets, semantic case identities, partitions, documents, truth, and complete corpus digest. Offline verification rejects malformed paths, digest changes, empty/invalid truth, duplicate cases, and cross-partition semantic overlap.
- A purpose-built real regression manifest binds the repository born-digital and scanned fixtures, their truth, and fixture-generator provenance, so synthetic cases are not the sole authority.
- `ferrodoc-bench` records exact candidate engine/version, optional model digest, configuration digest, toolchain, metric version/configuration, every case outcome, and tagged measured/estimated/unknown evidence.
- Metrics cover Unicode-normalized CER/WER, reading-order edges, one-to-one regions, category-specific geometry and text quality, spatially associated table semantics, spatially associated LaTeX token similarity, and exact regression checks.
- Measurement covers cold/warm wall and CPU time, repeated-sample statistics, and process peak RSS. VRAM, model load, warm residency, and remote cost accept supplied evidence and otherwise remain explicit unknown.
- Comparison retains seven separate Pareto dimensions, policy/tolerances, per-case deltas, and repeated-sample variance notes. Missing baseline successes cannot dominate.

Added packages are `tools/ferrodoc-foundry` and `tools/ferrodoc-bench`. Added artifacts are six JSON Schema snapshots, the synthetic specification, the real regression manifest/truth, the offline benchmark smoke, governance documentation, and Phase 5 CI regeneration. No package or persistent schema was removed or renamed.

## Corpus identities

- synthetic smoke corpus: `6691b806f52cfd81c3f532530612cab698d6286dbd533398d55d36802eae5b0c` (3 cases, seed 20260820, `ferrodoc-foundry/1`);
- real regression corpus: `9ca75658e0db622584320467b7cc312cff9f4409c2909d62c0a78df7fcd4f6d5` (2 cases, `ferrodoc-real-regression/1`).

The oracle prediction generator is evaluator self-test machinery only. It refuses held-out cases and is not engine quality, latency, or resource evidence.

## Acceptance criteria

| Criterion | Evidence |
|---|---|
| Empty work cannot pass or score perfectly | Unit and offline smoke convert an empty prediction set into all failures and aggregate quality 0. |
| Missing candidate cases cannot dominate | Comparison test uses identical corpus cases and confirms a missing baseline success is never candidate-dominant. |
| One prediction cannot satisfy multiple regions | One-to-one augmenting assignment regression test matches only one of two coincident truth regions. |
| Tables and formulas require spatial and semantic agreement | Tests detach table/formula IDs and observe zero semantics; perfect regression requires exact associated content and geometry. |
| Unknown RAM/VRAM remains unknown | Tagged evidence serialization test and smoke comparison preserve `unknown`; no zero substitution occurs. |
| Incompatible reports reject | Comparison rejects corpus, metric-version, metric-configuration, and case-set mismatches. |
| Synthetic and real suites run in CI | `scripts/benchmark-smoke.sh` runs three deterministic synthetic cases and two purpose-built real cases in the default core job. |
| Fixed runs are reproducible | Byte-identical dual foundry generation, artifact/schema regeneration with clean diff, manifest/candidate/config/toolchain identities, and offline digest verification pass. |

## Validation

The final locked gate passed:

```text
cargo metadata --locked --format-version 1
./scripts/check-workspace.sh
./scripts/check-boundaries.sh
cargo run --locked -p xtask -- doctor
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo check --locked -p ferrodoc-runtime --features nvml
cargo test --workspace --locked                       # 125 tests passed
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p ferrodoc-ir --example export_snapshots
cargo run --locked -p ferrodoc-pdf --example generate_fixtures
cargo run --locked -p ferrodoc-protocol --example export_fixtures
cargo run --locked -p ferrodoc-foundry --example export_foundry_schemas
cargo run --locked -p ferrodoc-bench --example export_benchmark_schemas
cargo run --locked -p ferrodoc-bench --example export_real_regression
./scripts/smoke.sh                                  # offline workspace smoke: ok
./scripts/benchmark-smoke.sh                        # offline benchmark smoke: ok
git diff --exit-code
```

No required Phase 5 validation was left unexecuted.

## Real-PDF evidence

The requested Lightbulb corpus was exercised during the phase using `1975-relationship-of-observed-flow-patterns-to-gas-core-reactor-criticality-osti-4130843.pdf`:

- bounded inspection found 4 pages and native evidence on all 4;
- low-VRAM planning selected 9 required stages and rejected 4 unnecessary OCR stages;
- Markdown conversion completed with 4,780 bytes across 86 lines.

This is a pipeline regression check, not ground-truthed benchmark quality evidence.

## Migration, risk, rollback, and deviations

The additions are new v1 schemas and packages, so there is no migration from an earlier benchmark format. Consumers must provide exact corpus and candidate identities; omissions that were previously unrepresented now become explicit failures or unknown evidence.

Main risks are metric/version drift, accidental truth leakage, fixture provenance changes, and overinterpreting synthetic or oracle results. Checked schemas, corpus digests, CI regeneration, held-out refusal/governance, and the real fixture subset constrain those risks. Rollback is removal of the two tool packages and their isolated CI/artifact additions; runtime conversion crates are not coupled to them.

There are no deviations from the Phase 5 plan. Remote PR creation and repository settings were not performed. The phase is merged locally after this evidence commit.
