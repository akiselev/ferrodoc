# Status

- Phases 0 through 8 are complete and merged to `master`.
- The v0.2 release operation is deferred: no tag exists and the workspace stays at `0.1.0` until release; `scripts/release-check.sh` derives the expected version from the workspace manifest. See `docs/release.md`.
- Competitive scope, measured gaps, and the M0-M9 sequence against pinned Marker v2 are recorded in `docs/marker-parity.md`.
- FP0/FDX0 progressive contracts are implemented in `ferrodoc-ir`: immutable evidence deltas, logical document states, page-qualified targets, evidence-grade cells, and checkpoint-equivalent canonical materialization. Execution and persistence remain FP1-FP4 work.
- Next: M0 comparison protocol (pinned Marker benchmark manifest, olmOCR adapter, cold/warm latency separation), then M1 character-aware native PDF evidence.
