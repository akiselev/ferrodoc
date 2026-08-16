# Ferrodoc agent guide

This file is the short operational contract for coding agents working on the repository.

## Invariants

1. `ferrodoc-core`, `ferrodoc-ir`, and `ferrodoc-protocol` stay runtime-agnostic. Do not add CUDA, ONNX, Candle, llama.cpp, HTTP-provider, or OCR-library dependencies to them.
2. Heavy/model-specific integrations are dual-mode engine crates: a library implementing `Engine` plus a thin executable transport wrapper. Prefer a new `ferrodoc-engine-*` crate over a host feature flag.
3. Native PDF evidence is never destroyed by OCR. Engines append provenance-bearing hypotheses; reconciliation chooses a view.
4. Device/resource placement is a planner decision. Plugins report estimates and capabilities; they do not silently seize all VRAM.
5. Every expensive stage must be deterministic enough to cache from `(input digest, model digest, engine version, parameters)`.
6. Do not optimize against the held-out benchmark set. Foundry generation seeds and benchmark manifests are part of experiment provenance.
7. Low-VRAM is a supported target, not a fallback. Keep `Profile::LowVram` under its declared hard budget or explicitly reject the candidate.
8. Plugin stdout belongs exclusively to framed protocol traffic. Diagnostic logs go to stderr.

## Fast orientation

- Document semantics: `crates/ferrodoc-ir`
- Plugin ABI: `crates/ferrodoc-protocol`, `crates/ferrodoc-plugin-sdk`, `crates/ferrodoc-plugin-host`
- Routing and resource selection: `crates/ferrodoc-router`, `crates/ferrodoc-planner`, `crates/ferrodoc-scheduler`
- PDF acquisition/rasterization: `crates/ferrodoc-pdf`
- End-to-end orchestration: `crates/ferrodoc-pipeline`
- Synthetic truth: `crates/ferrodoc-foundry`
- Evaluation: `crates/ferrodoc-bench`
- Search/experiment ledger: `crates/ferrodoc-research`
- Model artifacts: `crates/ferrodoc-model-store`, `models/`
- User surface: `crates/ferrodoc-cli`

## Before changing routing

Run or create a fixed benchmark report before the change, make the change, rerun the exact same manifest, then compare. Record both quality and resource/latency outcomes. A change that improves one metric while losing another is not automatically a regression; keep the Pareto point in the experiment ledger.

## Before adding an engine

Implement `ferrodoc_plugin_sdk::Engine`, declare honest capabilities/devices, make `health` useful, return a conservative resource estimate, and keep model acquisition in `ferrodoc-model-store`. If the engine wraps a volatile native runtime, keep that runtime inside the engine crate and prefer the Cargo process transport for isolation. Do not shell out to a second OCR/model CLI from an official engine; direct Rust APIs or a narrow FFI boundary belong inside the plugin.

## Expected validation

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets
cargo run -p xtask -- doctor
```

Then generate a small foundry corpus and exercise at least one CPU OCR path plus any engine changed by the patch.
