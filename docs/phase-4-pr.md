# Phase 4 validation evidence

Baseline: `58137d5`  
Branch: `phase-4/resource-runtime`

## Delivered runtime

- immutable SHA-256 model blobs and atomically published logical views;
- offline verification, explicit acceptance, source-root containment, leases, and live-manifest garbage collection;
- checked OCRS manifest with exact sizes/digests and visible CC-BY-SA-4.0 metadata;
- fixture-tested logical/physical CPU and RAM inventory with source/confidence, plus optional NVML inventory behind `nvml`;
- capability, backend/device, model, RAM, VRAM, privacy, offline, cost, and deadline admission with stable reason codes;
- all eight built-in profiles, including a hard 2 GiB `low-vram` ceiling and no fabricated fallback;
- worker, host-RAM, per-device, guarded-unknown, warm-residency, cancellation, and observation-aware scheduler leases;
- atomic stage caching keyed by input, model roles, engine ID/version, schema, stage, page, seed, and normalized parameters;
- real `models list|verify|pull|gc`, `plugins doctor`, resource-aware `plan`, and lease/cache/measurement `explain` CLI operations.

## Acceptance evidence

- corrupt, unaccepted, symlink-escaping, and partially supplied multi-file models never produce a visible logical view;
- the checked OCRS manifest installed from the verified local model pair, verified offline, and survived garbage collection with zero removed live blobs;
- unknown hard estimates reject without explicit guarded admission;
- low-VRAM candidates over 2 GiB reject before execution;
- deterministic scheduler tests prevent overlapping leases beyond device capacity, reserve a whole budget for guarded unknowns, propagate cancellation under backpressure, retain/evict warm models, and cancel on measured reservation overrun;
- cache tests reopen stable hits, detect corruption, reject uncacheable results, and invalidate input, model, engine ID/version, schema, stage, and parameter changes;
- CLI integration changes an actual layout stage from cache miss plus lease to verified hit without a lease, and rejects a 1 MiB RAM conversion before engine execution;
- `plugins doctor` distinguishes discovery, dependency, model, health, and inference checks;
- model-backed doctor inference passed for both layout and OCRS on the checked-in scanned fixture;
- model-backed OCR conversion changed from miss plus lease to hit without a lease on its second run.

Peak process attribution is not portable for the embedded path, so measurements remain explicitly absent there; conservative reservations are still admitted and enforced. The optional NVML feature compiles without making NVML or NVIDIA hardware a default dependency.

## Real-PDF evidence

The requested Lightbulb corpus was exercised again after resource enforcement:

- the six-page image-only nuclear-pumped-lasers paper planned every OCR stage as unavailable without models, with machine-readable `model_unavailable`; no fallback was invented;
- the four-page cavity-reactor born-digital paper executed four layout leases on the first cached run and four verified cache hits with zero leases on the second run.

The earlier bounded negative result for full debug OCR of the six-page historical scan remains unchanged; Phase 4 did not claim a quality or throughput improvement for that document.

## Final gate

The final gate runs locked metadata, workspace and boundary checks, formatting, all-target checking, 113 default tests, strict Clippy, the NVML feature check, `xtask doctor`, artifact regeneration, offline smoke, and a clean generated-artifact diff. Separate real-model checks cover manifest installation/verification/GC, model-backed plugin inference, and OCR cache miss/hit behavior.

Remote PR creation and repository settings were not performed. The phase is merged locally after these gates pass.
