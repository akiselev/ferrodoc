# Ferrodoc

Ferrodoc is a pre-release Rust project for evidence-preserving document extraction. The intended system will retain native PDF text and OCR hypotheses separately, choose work under explicit hardware and policy constraints, and render deterministic outputs.

## Current status

Repository recovery is complete enough to provide a truthful build baseline. The workspace currently contains only `ferrodoc-core`, the recovered runtime-agnostic types from the original import. There is no usable CLI, PDF converter, OCR engine, plugin transport, model store, foundry, or benchmark runner yet.

The implementation sequence and acceptance gates are defined in [PLAN.md](PLAN.md). Current work is summarized in [STATUS.md](STATUS.md), and the discarded source payload is documented in [docs/recovery-inventory.md](docs/recovery-inventory.md).

## Prerequisites

- Rustup; the repository pins Rust 1.95.0 and requests Rustfmt and Clippy.
- Network access is needed only for the first Cargo dependency fetch. Builds, tests, and the smoke check do not download models or native binaries.

## Verify the baseline

```bash
cargo metadata --locked --format-version 1 > /dev/null
./scripts/check-workspace.sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
./scripts/smoke.sh
```

These are the only supported operational commands at this phase. The future CLI and engine portfolio remain planned work.

## Design invariants

- Native PDF evidence will not be overwritten by OCR evidence.
- Runtime-agnostic contracts will not depend on model, OCR, GPU, HTTP, or PDF runtimes.
- Unknown resource use will remain explicit rather than being reported as zero.
- The default path will remain offline-capable and CPU-capable.
- Expensive work will be cacheable from deterministic content and configuration identity.
- Plugin stdout will be reserved for framed protocol traffic.

Ferrodoc is dual-licensed under MIT or Apache-2.0.
