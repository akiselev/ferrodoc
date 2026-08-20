# Architecture

## Product boundary

Ferrodoc is a document extraction compiler. It acquires immutable input, collects deterministic and learned evidence, reconciles without destroying hypotheses, and renders a selected view. Phase 2 implements the embedded CPU vertical slice; policy-rich planning and isolated process execution remain later phases.

## Package boundaries

```text
ferrodoc-core
├── ferrodoc-ir
│   ├── ferrodoc-engine-api
│   │   ├── ferrodoc-protocol
│   │   ├── ferrodoc-runtime
│   │   ├── ferrodoc-layout-rulebased
│   │   └── ferrodoc-engine-ocrs
│   └── ferrodoc-render
├── ferrodoc-pdf
└── ferrodoc CLI
```

- `ferrodoc-core` owns validated runtime-agnostic primitives: geometry, quantities, digests, stable IDs, scoped blobs, model manifests, resource estimates, device/backend/placement axes, schema versions, and provenance.
- `ferrodoc-ir` owns persistent document semantics and canonical evidence-graph JSON.
- `ferrodoc-engine-api` owns blocking engine semantics, descriptors, health, estimates, requests, responses, cancellation, deadlines, and errors.
- `ferrodoc-protocol` owns versioned process message schemas and bounded length-prefixed CBOR framing.
- `ferrodoc-plugin-sdk` is the thin stdin/stdout server wrapper; protocol stdout never carries diagnostics.
- `ferrodoc-runtime` owns embedded registration, the bounded isolated process host, native-quality routing, evidence append, deterministic reconciliation, plans, and traces. Scheduling, caching, and model storage remain later modules.
- `ferrodoc-pdf` performs bounded inspection/native extraction with lopdf and deterministic pure-Rust rasterization with Hayro.
- `ferrodoc-layout-rulebased` and `ferrodoc-engine-ocrs` implement the common engine trait. OCRS model bytes are injected explicitly and never acquired by the engine.
- `ferrodoc-render` emits deterministic Markdown, semantic HTML, and canonical full-evidence JSON.
- `ferrodoc` exposes conversion, inspection, planning, trace explanation, and conservative hardware reporting.

The runtime-agnostic contract crates contain no OCR, PDF parser, GPU, model runtime, HTTP client, async runtime, or native binary download feature. The default application is pure Rust and has no build-time model or binary download. `scripts/check-boundaries.sh` enforces the contract boundary in CI.

## Determinism and observations

`DeterministicProvenance` contains only schema, input, engine, model, normalized parameter, and stage identity. It can be hashed directly for cache and evidence identity. `Observation` is a separate type for request IDs, timestamps, host/device facts, durations, resource measurements, and diagnostic labels. Observations never enter canonical artifact identity.

## Trust boundaries

Engine request schemas carry `BlobId` plus a checked, nonempty `BlobRange`; they never carry a host path. Embedded and process hosts verify token, range, media type, and digest before returning bytes. Process launch accepts explicit absolute executables or exact names under caller-provided trusted roots; it never searches the current directory.

PDF input is hostile. `ferrodoc-pdf` applies byte limits before parsing and defines page, object, recursion, and raster limits for Phase 2 implementations.

## Execution model

The `Engine` trait is synchronous and `Send`. Blocking native libraries therefore do not force Tokio or another runtime into semantic crates. An orchestrator may call an engine through a dedicated worker or process while supplying cooperative cancellation, a deadline, scoped blob resolution, and structured tracing.

See [ADR 0001](adr/0001-consolidate-runtime.md) and [ADR 0002](adr/0002-transport-independent-engines.md).
