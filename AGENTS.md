# Ferrodoc agent guide

This file is the short operational contract for coding agents working on the repository.

## Invariants

1. `ferrodoc-core`, `ferrodoc-ir`, and `ferrodoc-protocol` stay runtime-agnostic. Do not add CUDA, ONNX, Candle, llama.cpp, HTTP-provider, PDF-parser, or OCR-library dependencies to them.
2. Heavy/model-specific integrations are dual-mode engine crates: a library implementing `Engine` plus a thin executable transport wrapper. Prefer a new `ferrodoc-engine-*` crate over a host feature flag.
3. Native PDF evidence is never destroyed by OCR. Engines append provenance-bearing hypotheses; reconciliation chooses a view.
4. Device/resource placement is a planner decision. Engines report estimates and capabilities; they do not silently seize all VRAM.
5. Every expensive stage must be deterministic enough to cache from `(input digest, model digest, engine version, parameters)`.
6. Do not optimize against the held-out benchmark set. Foundry generation seeds and benchmark manifests are part of experiment provenance.
7. Low-VRAM is a supported target, not a fallback. Keep `Profile::LowVram` under its declared hard budget or explicitly reject the candidate.
8. Engine stdout belongs exclusively to framed protocol traffic. Diagnostic logs go to stderr.

## Current orientation

- Implemented runtime-agnostic baseline types: `crates/ferrodoc-core`
- Recovery evidence: `docs/recovery-inventory.md`
- Implementation order and future paths: `PLAN.md`
- Short current state: `STATUS.md`

Paths named in `PLAN.md` are planned, not implemented, until they enter the workspace with real targets and tests. Do not add placeholder crates.

## Before changing routing

Run or create a fixed benchmark report before the change, make the change, rerun the exact same manifest, then compare. Record both quality and resource/latency outcomes. A change that improves one metric while losing another is not automatically a regression; keep the Pareto point in the experiment ledger.

## Before adding an engine

Implement the transport-independent `Engine` contract, declare honest capabilities and devices, make `health` useful, return conservative resource estimates, and keep model acquisition in the runtime model store. If the engine wraps a volatile native runtime, keep that runtime inside the engine crate and prefer process transport for isolation. Official engines use direct Rust APIs or a narrow FFI boundary rather than shelling out to a second OCR/model CLI.

## Expected validation

```bash
cargo metadata --locked --format-version 1 > /dev/null
./scripts/check-workspace.sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
./scripts/smoke.sh
```

After the PDF vertical slice exists, also test representative PDFs from `~/research/lightbulb` while keeping small purpose-built fixtures as the required CI inputs.
