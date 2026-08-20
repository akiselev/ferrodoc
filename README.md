# Ferrodoc

Ferrodoc is a pre-release Rust project for offline, evidence-preserving document extraction. Its CPU vertical slice retains native PDF text and OCR hypotheses separately and renders deterministic Markdown, HTML, or full evidence JSON.

## Current status

Repository recovery, foundations, and the Phase 2 vertical slice are implemented. Born-digital PDFs convert without models. Scanned and hybrid PDFs use the pure-Rust OCRS engine when an explicit verified model pair is supplied. Process isolation, the model store, resource planner, foundry, and benchmark runner enter in later phases.

The implementation sequence and acceptance gates are defined in [PLAN.md](PLAN.md). Current work is summarized in [STATUS.md](STATUS.md), and the discarded source payload is documented in [docs/recovery-inventory.md](docs/recovery-inventory.md).

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
```

## Quick start

```bash
cargo run --locked -p ferrodoc -- --version
cargo run --locked -p ferrodoc -- inspect fixtures/pdf/born-digital.pdf
cargo run --locked -p ferrodoc -- plan fixtures/pdf/born-digital.pdf
cargo run --locked -p ferrodoc -- convert fixtures/pdf/born-digital.pdf --output document.md
cargo run --locked -p ferrodoc -- explain fixtures/pdf/born-digital.pdf
cargo run --locked -p ferrodoc -- hardware
```

`convert` defaults to Markdown; `--format html` and `--format json` select semantic HTML or the complete evidence graph. Output files are committed with a temporary file and atomic rename. Malformed, encrypted, missing, unsupported, and unavailable-model failures use nonzero exit status and a JSON error envelope on stderr.

OCRS model acquisition is deliberately external until the content-addressed model store lands in Phase 4. To test scanned input, place the official OCRS `text-detection.rten` and `text-recognition.rten` files in one directory and pass `--ocrs-model-dir DIR`. Ferrodoc never downloads them during build or conversion. The optional model-backed CI job verifies their SHA-256 digests before use.

## Design invariants

- Native PDF evidence is not overwritten by OCR evidence.
- Runtime-agnostic contracts do not depend on model, OCR, GPU, HTTP, or PDF runtimes.
- Unknown resource use will remain explicit rather than being reported as zero.
- The default born-digital path is offline-capable and CPU-capable.
- Expensive work will be cacheable from deterministic content and configuration identity.
- Plugin stdout will be reserved for framed protocol traffic.

Ferrodoc is dual-licensed under MIT or Apache-2.0.
