# Ferrodoc

Ferrodoc is a pre-release Rust project for offline, evidence-preserving document extraction. Its CPU vertical slice retains native PDF text and OCR hypotheses separately and renders deterministic Markdown, HTML, or full evidence JSON.

## Current status

The verified v0.2 release candidate includes offline conversion, the qualified engine portfolio, integrity-first benchmarking, and guarded routing/research. Born-digital PDFs convert without models. Scanned and hybrid PDFs use the pure-Rust OCRS engine when an explicit verified model pair is supplied. Engines can run embedded or over the bounded process protocol; conversion applies explainable hard constraints, scheduler leases, and an optional deterministic stage cache.

Phase 6 qualifies the native PDF, rule-based layout, OCRS, deterministic mock, optional Tesseract C-API, and experimental no-shell command boundaries. The default `cpu-minimal` and `process-engines` features remain pure Rust and network-free. See the [qualified engine portfolio](docs/engines/README.md).

Phase 7 adds digest-bound routing examples, deterministic baselines, guarded learned recommendations, and an immutable experiment ledger. The fixed routing calibration is intentionally negative: its stump does not beat the deterministic held-out baseline and is therefore rejected rather than enabled. See [routing and research](docs/research.md).

The implementation sequence and acceptance gates are defined in [PLAN.md](PLAN.md). Current work is summarized in [STATUS.md](STATUS.md). CLI and protocol compatibility are documented in [docs/cli.md](docs/cli.md) and [docs/protocol.md](docs/protocol.md); security and release policy are in [docs/security.md](docs/security.md) and [docs/release.md](docs/release.md).

## Prerequisites

- Rustup; the repository pins Rust 1.95.0 and requests Rustfmt and Clippy.
- Network access is needed only for the first Cargo dependency fetch. Builds, tests, and the smoke check do not download models or native binaries.

## Verify the baseline

```bash
cargo metadata --locked --format-version 1 > /dev/null
./scripts/check-workspace.sh
./scripts/check-boundaries.sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
./scripts/smoke.sh
./scripts/benchmark-smoke.sh
./scripts/routing-smoke.sh
./scripts/readme-smoke.sh
```

## Quick start

```bash
cargo run --locked -p ferrodoc -- --version
cargo run --locked -p ferrodoc -- inspect fixtures/pdf/born-digital.pdf
cargo run --locked -p ferrodoc -- plan fixtures/pdf/born-digital.pdf
cargo run --locked -p ferrodoc -- convert fixtures/pdf/born-digital.pdf --output document.md
cargo run --locked -p ferrodoc -- explain fixtures/pdf/born-digital.pdf
cargo run --locked -p ferrodoc -- hardware
cargo run --locked -p ferrodoc -- plugins doctor
cargo run --locked -p ferrodoc -- models list --store .ferrodoc/models
cargo run --locked -p ferrodoc -- router inspect . benchmarks/routing/dataset.json
```

`convert` defaults to Markdown; `--format html` and `--format json` select semantic HTML or the complete evidence graph. Output files are committed with a temporary file and atomic rename. Malformed, encrypted, missing, unsupported, and unavailable-model failures use nonzero exit status and a JSON error envelope on stderr.

Ferrodoc never downloads models during build or conversion. The checked-in [OCRS manifest](models/ocrs-cpu.json) records exact sizes, SHA-256 digests, source, revision, license, and required acceptance. `models pull` installs an already acquired pair atomically from a local directory; see [models/README.md](models/README.md). Conversion can still load that verified logical directory with `--ocrs-model-dir DIR`.

`plan` accepts profiles plus hard `--max-ram`, `--max-vram`, `--max-cost-microusd`, and `--deadline-ms` constraints. Unknown values fail hard limits unless `--allow-unknown-estimates` explicitly requests guarded execution. `--cache-dir DIR` enables atomic stage caching from input, model, engine, schema, page, seed, and normalized-parameter identity.

The deterministic foundry, real regression corpus, evaluator contracts, metrics, measurement evidence, held-out rules, and Pareto comparison workflow are described in [Benchmarking and corpus governance](docs/benchmarking.md). The default benchmark smoke is offline and explicitly verifies that missing work scores as failure rather than success.

The router and experiment commands are offline. `router train` writes a model only after re-hashing every conversion trace and benchmark report; its qualification field remains `rejected` unless it beats all declared deterministic baselines on identical held-out cases. `research run` reads immutable reports, re-hashes protected truth and evaluator files before and after scoring, observes cumulative budgets, and atomically writes resumable ledger state.

Optional Tesseract is selected explicitly and discovered at runtime; the default binary never links a native OCR library:

```bash
cargo run --locked -p ferrodoc --features tesseract -- plugins doctor --inference
cargo run --locked -p ferrodoc --features tesseract -- convert scan.pdf --ocr-engine tesseract
```

The experimental command wrapper requires a trusted `FERRODOC_COMMAND_CONFIG`, an absolute canonical executable allowlist, typed arguments, and process transport. It is not selected by the CLI as an official OCR engine. Fixed-corpus portfolio reports are produced by `scripts/engine-qualification.sh` and retain failures plus explicit unknown resources.

## Design invariants

- Native PDF evidence is not overwritten by OCR evidence.
- Runtime-agnostic contracts do not depend on model, OCR, GPU, HTTP, or PDF runtimes.
- Unknown resource use is explicit rather than being reported as zero, and does not satisfy hard limits by default.
- The default born-digital path is offline-capable and CPU-capable.
- Deterministic expensive work is cacheable from complete semantic identity.
- Plugin stdout is reserved for framed protocol traffic.

Ferrodoc is dual-licensed under MIT or Apache-2.0.

## Roadmap

Post-v0.2 work may add specialized table, formula, handwriting, and document-VLM engines; richer reconciliation; larger independently governed benchmarks; region-level routing; and platform sandboxes. These are not qualified v0.2 capabilities. See [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) and the plan's post-v0.2 roadmap.
