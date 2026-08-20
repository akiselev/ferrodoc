# Ferrodoc

Ferrodoc is a pre-release Rust project for evidence-preserving document extraction. The intended system will retain native PDF text and OCR hypotheses separately, choose work under explicit hardware and policy constraints, and render deterministic outputs.

## Current status

Repository recovery and the Phase 1 foundations are implemented. The workspace contains validated core primitives, a versioned evidence IR, a transport-independent engine API, process-protocol schema types, and explicit runtime/PDF/render/CLI skeletons. There is not yet a PDF converter, OCR engine, process transport, model store, foundry, or benchmark runner.

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

The CLI currently exposes only truthful foundation status:

```bash
cargo run --locked -p ferrodoc -- --version
cargo run --locked -p ferrodoc -- status
```

PDF commands and the engine portfolio remain planned work.

## Design invariants

- Native PDF evidence will not be overwritten by OCR evidence.
- Runtime-agnostic contracts will not depend on model, OCR, GPU, HTTP, or PDF runtimes.
- Unknown resource use will remain explicit rather than being reported as zero.
- The default path will remain offline-capable and CPU-capable.
- Expensive work will be cacheable from deterministic content and configuration identity.
- Plugin stdout will be reserved for framed protocol traffic.

Ferrodoc is dual-licensed under MIT or Apache-2.0.
