# Ferrodoc v0.2.0 (unreleased)

These notes describe the pending v0.2 release. The workspace version remains
`0.1.0` until the release operation bumps it; see `docs/release.md`.

Ferrodoc v0.2 is the first qualified offline vertical slice. It preserves native
PDF and OCR evidence independently, plans resource placement before execution,
and emits deterministic Markdown, HTML, or versioned evidence JSON.

Qualified engines:

- native PDF extraction through the bounded pure-Rust PDF layer;
- deterministic rule-based layout;
- pure-Rust OCRS CPU OCR with explicitly installed, digest-verified models;
- deterministic mock engine for conformance and fault injection;
- optional direct Tesseract C-API CPU OCR with runtime discovery;
- experimental administrator-controlled no-shell command escape hatch, not an official OCR integration.

The default build is offline after dependency acquisition, CPU-only, and has no system OCR-library or model-download requirement. Optional Tesseract and NVML support are feature-isolated.

v0.2 also includes bounded process transport, model/store and cache atomicity, hard RAM/VRAM/cost/deadline planning, deterministic foundry and real regression corpora, integrity-first metrics, fixed-corpus engine evidence, guarded routing calibration, and an immutable experiment ledger. The checked learned-router calibration is rejected because it does not beat deterministic held-out baselines; learned routing is not enabled.

Release hardening upgrades hostile-PDF parsing to `lopdf` 0.44.0, resolving RUSTSEC-2026-0187 and adding upstream decompression bounds. PDF acquisition and protocol encoding now enforce their limits before or during allocation.

Known limits include tiny benchmark coverage, unknown process-attributed RAM/VRAM for several paths, no OS-level child sandbox, no network model acquisition, and no specialized table/formula/handwriting/VLM engine. See `KNOWN_LIMITATIONS.md`, `docs/security.md`, and `THIRD_PARTY.md`.
