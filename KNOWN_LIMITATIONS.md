# Known limitations

- The qualified engine portfolio currently contains rule-based layout and pure-Rust CPU OCRS only. Tesseract and specialized table/formula engines are not yet qualified.
- `models pull` intentionally installs from a local acquisition directory. It does not contact a registry or download model bytes.
- Linux exposes CPU topology and RAM through `/proc`; unsupported platforms report explicit unknowns. NVIDIA inventory is compile-time optional through the `nvml` feature and returns no devices when NVML is unavailable.
- Embedded execution does not currently attribute process-level peak RAM/VRAM, so measurements remain unknown even though conservative reservations are enforced. Process-level observation is platform-dependent future work.
- Cache and model-store roots are local filesystem stores; multi-host locking and distributed eviction are out of scope for v0.2.
- The foundry, trustworthy benchmark runner, research ledger, and automatic routing calibration arrive in Phases 5 through 7.
- The archived source import was truncated. Its complete original tree cannot be reconstructed from the committed fragments.
- Branch protection and required checks are repository settings and must be enabled after the Phase 0 change is merged.
