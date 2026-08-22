# Status

- Phases 0 through 8 are complete and merged to `master`.
- The v0.2 release operation is deferred: no tag exists and the workspace stays at `0.1.0` until release; `scripts/release-check.sh` derives the expected version from the workspace manifest. See `docs/release.md`.
- Competitive scope, measured gaps, and the M0-M9 sequence against pinned Marker v2 are recorded in `docs/marker-parity.md`.
- FP0/FDX0 progressive contracts are implemented in `ferrodoc-ir`: immutable evidence deltas, logical document states, page-qualified targets, evidence-grade cells, and checkpoint-equivalent canonical materialization.
- FP1 scoped execution is implemented in `ferrodoc-runtime`: state-aware capability goals, declared stage prerequisites, document/page/page-qualified-region planning, already-satisfied/no-plan/bounded-candidate outcomes, deterministic delta materialization, and backward-compatible process request fixtures.
- FP2 survey/baseline is implemented across `ferrodoc-pdf` and `ferrodoc-runtime`: cheap structural survey and duplicate features, OCR of every nonblank page under the baseline profile, separate native/OCR evidence with disagreement diagnostics, honest page-only native geometry, and a canonical baseline checkpoint/state. Model-backed OCR quality and platform-specific benchmark observations remain explicit environmental gates; durable persistence and targeted refinement remain FP3-FP4 work.
- Next: M0 comparison protocol (pinned Marker benchmark manifest, olmOCR adapter, cold/warm latency separation), then M1 character-aware native PDF evidence.
