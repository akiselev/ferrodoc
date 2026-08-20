# Architecture

## Product boundary

Ferrodoc is a document extraction compiler. It will acquire immutable input, collect deterministic and learned evidence, select work under policy and hardware constraints, reconcile without destroying hypotheses, and render a selected view. Phase 1 establishes the contracts only; PDF conversion begins in Phase 2.

## Package boundaries

```text
ferrodoc-core
├── ferrodoc-ir
│   ├── ferrodoc-engine-api
│   │   ├── ferrodoc-protocol
│   │   └── ferrodoc-runtime
│   └── ferrodoc-render
├── ferrodoc-pdf
└── ferrodoc CLI skeleton
```

- `ferrodoc-core` owns validated runtime-agnostic primitives: geometry, quantities, digests, stable IDs, scoped blobs, model manifests, resource estimates, device/backend/placement axes, schema versions, and provenance.
- `ferrodoc-ir` owns persistent document semantics and canonical evidence-graph JSON.
- `ferrodoc-engine-api` owns blocking engine semantics, descriptors, health, estimates, requests, responses, cancellation, deadlines, and errors.
- `ferrodoc-protocol` owns versioned process message schemas. It has no framing or process I/O in Phase 1.
- `ferrodoc-runtime` currently owns explicit embedded-engine registration. Planning, scheduling, process hosting, caching, and model storage are later modules of this crate.
- `ferrodoc-pdf` owns parser limits and immutable PDF acquisition identity. It deliberately has no parser dependency yet.
- `ferrodoc-render` owns deterministic output. Only canonical full-evidence JSON exists in Phase 1.
- `ferrodoc` is a truthful CLI skeleton exposing version and phase status only.

The default dependency graph contains no OCR, PDF parser, GPU, model runtime, HTTP client, async runtime, or native binary download feature. `scripts/check-boundaries.sh` enforces the runtime-agnostic package boundary in CI.

## Determinism and observations

`DeterministicProvenance` contains only schema, input, engine, model, normalized parameter, and stage identity. It can be hashed directly for cache and evidence identity. `Observation` is a separate type for request IDs, timestamps, host/device facts, durations, resource measurements, and diagnostic labels. Observations never enter canonical artifact identity.

## Trust boundaries

Engine request schemas carry `BlobId` plus a checked, nonempty `BlobRange`; they never carry a host path. The host retains responsibility for immutable registration, path/symlink containment, range enforcement, and optional digest verification. Those resolver guarantees and adversarial tests enter with process transport in Phase 3.

PDF input is hostile. `ferrodoc-pdf` applies byte limits before parsing and defines page, object, recursion, and raster limits for Phase 2 implementations.

## Execution model

The `Engine` trait is synchronous and `Send`. Blocking native libraries therefore do not force Tokio or another runtime into semantic crates. An orchestrator may call an engine through a dedicated worker or process while supplying cooperative cancellation, a deadline, scoped blob resolution, and structured tracing.

See [ADR 0001](adr/0001-consolidate-runtime.md) and [ADR 0002](adr/0002-transport-independent-engines.md).
