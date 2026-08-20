# Deterministic mock engine

- Package/ID: `ferrodoc-engine-mock`, `test.mock`.
- Dependencies/models: pure Rust, model-free.
- Capabilities: fixture-only `ocr.page` echo semantics plus explicit fault injection.
- Devices/network: CPU, no network, deterministic, maximum concurrency 1.
- Estimate: conservative 1 MiB peak RAM, zero warm RAM/VRAM/cost, 1 ms nominal latency.
- Isolation: used in both embedded and process modes to test framing, stderr floods, malformed startup, cancellation, timeout, duplicate requests, and error mapping.
- Licenses: MIT OR Apache-2.0.
- Benchmark status: common conformance and process fault suites pass. It is plumbing evidence, not document-quality evidence, and is never a selectable production OCR candidate.
