# Rule-based layout engine

- Package/ID: `ferrodoc-layout-rulebased`, `layout.rulebased`.
- Dependencies/models: pure Rust, model-free.
- Capabilities: `layout.detect`, `reading-order.detect`.
- Devices/network: CPU through the `rules` backend, no network, deterministic, maximum concurrency 64.
- Estimate: conservative 16 MiB peak RAM, 1 MiB warm RAM, zero VRAM/cost, 10 ms nominal latency. Execution rejects text beyond 16 MiB and unreasonable page dimensions.
- Isolation: embedded is recommended; the thin framed process wrapper is tested for identical output.
- Licenses: MIT OR Apache-2.0.
- Benchmark status: shared conformance passes; the native fixed-corpus portfolio uses this engine for born-digital region construction. Its imperfect geometry score remains visible rather than being presented as an OCR-quality claim.
