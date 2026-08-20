# Native PDF extraction

- Integration: built directly into `ferrodoc-pdf`; no plugin process or model.
- Dependencies: pure-Rust `lopdf`; Hayro is used only when rasterization is required.
- Capabilities: bounded PDF inspection and native text acquisition. This is not advertised as an external `Engine` capability in v0.2.
- Devices/network: CPU, no network.
- Resources: bounded by `PdfLimits`; runtime planning accounts for later layout work. Fine-grained peak RSS remains unknown.
- Isolation: embedded by default; hostile-input byte/page/object/recursion/raster limits are enforced before downstream execution.
- Licenses: Ferrodoc MIT OR Apache-2.0; dependency licenses remain in Cargo metadata.
- Benchmark status: the fixed real corpus native baseline succeeds on the born-digital case and records the scanned case as a failure rather than fabricating OCR. Exact report identity is emitted by `scripts/engine-qualification.sh`.
