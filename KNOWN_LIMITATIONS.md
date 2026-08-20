# Known limitations

- The qualified engine portfolio currently contains rule-based layout and pure-Rust CPU OCRS only. Tesseract and specialized table/formula engines are not yet qualified.
- `models pull` intentionally installs from a local acquisition directory. It does not contact a registry or download model bytes.
- Linux exposes CPU topology and RAM through `/proc`; unsupported platforms report explicit unknowns. NVIDIA inventory is compile-time optional through the `nvml` feature and returns no devices when NVML is unavailable.
- Embedded execution does not currently attribute process-level peak RAM/VRAM, so measurements remain unknown even though conservative reservations are enforced. Process-level observation is platform-dependent future work.
- Cache and model-store roots are local filesystem stores; multi-host locking and distributed eviction are out of scope for v0.2.
- The foundry and integrity-first evaluator are implemented, but checked CI uses an evaluator oracle only to validate scoring contracts; it is not engine quality evidence. Per-engine fixed-corpus qualification arrives in Phase 6.
- Device VRAM, model-load time, warm residency, and remote cost are supported evidence fields but remain explicit unknown unless the engine runner supplies observations. Statistical confidence intervals beyond visible repeated-sample variance are not yet modeled.
- The research ledger and automatic routing calibration arrive in Phase 7.
- The archived source import was truncated. Its complete original tree cannot be reconstructed from the committed fragments.
- Branch protection and required checks are repository settings and must be enabled after the Phase 0 change is merged.
