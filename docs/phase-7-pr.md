# Phase 7 validation evidence

Baseline: `f02c265`
Branch: `phase-7/routing-and-research`

## Goal and architecture changes

Phase 7 adds a trustworthy optimization loop without allowing learned preferences to bypass policy or turn a negative calibration into a product claim.

- `ferrodoc-router` defines versioned, pre-execution features with explicit missing values; digest-bound conversion-trace and benchmark-report lineage; document-family partition validation; always-native, threshold, page-type, and profile baselines; and a small auditable decision stump.
- Training consumes only `Partition::Train`. Admission compares the learned candidate with every deterministic baseline on identical `Partition::HeldOut` case IDs under a declared quality/failure/latency objective.
- `guarded_decision` receives the planner-approved engine set. Unqualified, incompatible, low-confidence, missing-feature, or hard-policy-rejected model output falls back deterministically and cannot introduce a rejected candidate.
- `ferrodoc-research` records immutable spec/code/model/corpus/evaluator digests, exact command vectors and environment facts, separate mutation/evaluation trial types, raw report paths/digests, cumulative budgets, resumable state, and a multidimensional Pareto frontier.
- The research runner executes no mutation or arbitrary evaluation command. It validates existing benchmark reports internally and re-hashes protected truth and metric code before and after evaluation. Ledger writes are atomic.
- The user CLI exposes `router inspect|train|evaluate|compare` and `research run|status`. Four new JSON Schema snapshots and an offline routing/research smoke are checked in CI.

Added packages are `crates/ferrodoc-router` and `crates/ferrodoc-research`. Added fixed artifacts are two real conversion traces, two real benchmark reports, one routing dataset, one immutable experiment spec, four schemas, generators, and `scripts/routing-smoke.sh`. No package or persistent schema was removed or renamed.

## Fixed calibration and experiment identities

- real regression corpus: `9ca75658e0db622584320467b7cc312cff9f4409c2909d62c0a78df7fcd4f6d5`;
- routing dataset: `83b6e8495e7b19f985b8ef70b15cb444f2c4d112052c6e58618a885c3bcb3a62`;
- experiment spec: `007b00d54a7bbf4bd6461b9d1cb16e3956d3f42f7ba9718dab145288ef8eb6ea`;
- native/no-model report: `f73806768e5cbb03d1c63e827b92842321c9bd8da746682029ea3f0f3d55e6da`;
- Tesseract report: `e362884aadaa0b8b10663429f01a5257f3ddfb627b94edde001afa6c2303fae3`;
- calibrated router model: `53b1b9cefdc4581024e304edf58de41f81ffd3182e2c05f7b2174ea2456ec7a3`.

The native candidate completed 1/2 cases with aggregate quality 0.1667 and measured total cold wall 35.86 ms. Tesseract completed 2/2 with aggregate quality 0.3333 and measured total cold wall 9,263.42 ms. Both retain unknown RAM/VRAM observations, so the experiment ledger retains both Pareto points rather than fabricating a resource winner.

The one-case training split selected a stump that used native extraction. On the image-only holdout it produced quality 0, failure rate 1.0, and objective -1.0000. The threshold, page-type, and balanced-profile baselines selected OCR, producing quality 0.3333, failure rate 0, and objective 0.3260 on the identical case ID. The model is therefore serialized as `rejected`. This is a calibration/falsification result, not a learned-routing success.

## Acceptance criteria

| Criterion | Evidence |
|---|---|
| Examples trace to real conversion and benchmark records | `router inspect` re-hashes two actual `ferrodoc explain` traces and two complete CLI qualification reports, then verifies copied case quality/failure/latency against source records. |
| Held-out documents and related variants are excluded from training | Training filters only `train`; qualification filters only `held_out`; validation rejects a repeated family identity across partitions. Unit regression covers the rejection. |
| Learned and deterministic policies use identical cases | `compare_plans` rejects differing case sets. The checked comparison reports `identical_case_sets: true` for four baselines and the learned candidate. |
| A model cannot override hard policy | Unit regression recommends OCR while only native is planner-approved and observes deterministic native fallback. The API can choose only from the supplied accepted set. |
| Missing/low-confidence output falls back deterministically | Missing-feature unit regression selects the first accepted fallback. Qualification, schema, confidence floor, feature presence, and hard-policy membership are all checked before model use. |
| Experiments record commands, inputs, identities, configuration, and results | The v1 spec binds code/model/corpus/evaluator, exact argv/environment/cwd identity, raw reports, protected artifacts, policy, and budgets; the ledger records each result and external report digest. |
| Runner cannot mutate evaluator or held-out truth | It executes no command, prohibits report/protected-path aliasing, and verifies protected truth/metric digests before and after evaluation. Mutation trials are recorded separately. |
| Pareto retains quality/resource tradeoffs | Comparison uses the Phase 5 multidimensional policy. Unknown RAM makes dominance indeterminate, so both native and Tesseract trial IDs remain on the frontier. |

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
cargo test --workspace --locked                         # 148 tests passed
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p ferrodoc-ir --example export_snapshots
cargo run --locked -p ferrodoc-pdf --example generate_fixtures
cargo run --locked -p ferrodoc-protocol --example export_fixtures
cargo run --locked -p ferrodoc-foundry --example export_foundry_schemas
cargo run --locked -p ferrodoc-bench --example export_benchmark_schemas
cargo run --locked -p ferrodoc-bench --example export_real_regression
cargo run --locked -p ferrodoc-router --example export_router_schemas
cargo run --locked -p ferrodoc-research --example export_research_schemas
./scripts/smoke.sh                                      # offline workspace smoke: ok
./scripts/benchmark-smoke.sh                            # offline benchmark smoke: ok
./scripts/routing-smoke.sh                              # offline routing and research smoke: ok
./scripts/engine-qualification.sh "$PWD/target/debug/ferrodoc" /tmp/ferrodoc-phase7-qualification-final
git diff --exit-code
```

No required Phase 7 validation was left unexecuted.

## Real-PDF evidence

The requested Lightbulb corpus was exercised with the six-page, image-only `1978-progress-in-nuclear-pumped-lasers-osti-7094593.pdf` (SHA-256 `b9a3dde9ae9adf6d99bf8096ea266d61b9a5bb7e9a9dd81b378c66059d910e06`). A low-VRAM Tesseract `explain` run completed in approximately 49 seconds. Every page reported zero native characters and an executed OCR stage. The hard-policy planner selected CPU Tesseract with a conservative 1 GiB RAM estimate and zero VRAM, demonstrating the deterministic scan route and planner gate on a non-fixture document.

This is execution/routing evidence, not ground-truthed quality evidence.

## Migration, risk, rollback, and deviations

The router and research schemas are new v1 contracts. They are additive to the default CLI and do not alter deterministic conversion policy because the checked model is rejected. Consumers must retain source files at their digest-bound relative paths, supply missing-feature reasons, and create a new experiment identity when reports or evaluator bytes change.

Primary risks are tiny-sample overinterpretation, lineage drift, related-variant leakage, scalar-objective misuse, and treating digest checks as an OS sandbox. Negative model admission, family partition validation, exact source verification, separate metric dimensions/Pareto retention, and the no-command evaluator constrain those risks. Rollback is removal of the two isolated packages, artifacts, and CLI commands; the Phase 6 deterministic runtime remains intact.

There are no deviations from the Phase 7 plan. The plan permits learned routing only when it beats or extends deterministic baselines; this calibration did not, so no learned runtime route was enabled. Remote PR creation and repository settings were not performed. The phase is merged locally after this evidence commit.
