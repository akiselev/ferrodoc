# OCRS CPU engine

- Package/ID: `ferrodoc-engine-ocrs`, `ocr.ocrs`.
- Dependencies: direct Rust `ocrs` and RTen APIs; no nested CLI and no network acquisition.
- Models: explicit verified detection/recognition RTen pair from `models/ocrs-cpu.json`; both bytes contribute to one model digest.
- Capabilities: `ocr.page`, `ocr.region` on bounded RGBA8 input.
- Devices/network: CPU through `rten`, no network, deterministic, maximum concurrency 1.
- Estimate: conservative 768 MiB peak RAM, 256 MiB warm RAM, zero VRAM/cost; latency and calibrated quality remain unknown. Images above 50 million pixels and inconsistent buffers reject before inference.
- Isolation: Cargo process transport is recommended for hard cancellation and process attribution; embedded/process parity is tested with the exact model pair.
- Licenses: Ferrodoc MIT OR Apache-2.0; OCRS/RTen dependency licenses from Cargo metadata; model license/provenance is recorded in the model manifest.
- Benchmark status: the model-backed CI job runs common conformance, scanned/hybrid conversion, process parity, and both fixed real cases. Without models, the same corpus records the scanned case as failure.
