# Phase 3 validation evidence

Baseline: `9e2d3f8`  
Branch: `phase-3/process-protocol`

## Delivered architecture

- version-negotiated, length-prefixed CBOR framing with a 16 MiB hard maximum;
- schema-versioned request/response snapshots and binary v1 conformance fixtures;
- `ferrodoc-plugin-sdk` server loop that reserves stdout for protocol traffic;
- parent-side `ProcessEngine` implementing the same semantic `Engine` trait;
- normalized scoped-blob registration without serialized host paths;
- explicit absolute/trusted-root discovery, cleared child environments, bounded stderr retention;
- bounded startup, request, cancellation, shutdown, and explicit restart policy;
- deterministic mock engine and thin mock, rule-based layout, and OCRS executable wrappers;
- explicit `embedded` or `process` execution mode in plans/traces.

The engine estimate method changed from `&self` to `&mut self` because a process transport must perform request/response I/O. Ferrodoc is pre-release and the semantic contract is otherwise unchanged.

## Security and failure evidence

The checked-in suites cover traversal and symlink escape, range overflow/out-of-scope access, blob digest and duplicate-token errors, oversized lengths before allocation, malformed/trailing CBOR, unknown messages, duplicate request IDs, garbage stdout, partial frames, stdout size attacks, bounded stderr flooding, startup and execution hangs, crashes, cancellation, graceful shutdown, version mismatch, process unavailability, and one permitted explicit restart.

No in-flight semantic request is retried automatically. Timeout or cancellation kills and waits for the child. The default restart bound is one and restart requires an explicit caller action.

## Parity evidence

- mock embedded/process descriptor, health, estimate, and execute responses matched;
- rule-based layout embedded/process responses matched through the real executable wrapper;
- OCRS embedded/process responses matched on the checked-in image-only fixture using the verified model pair; the final debug-profile parity rerun completed in 92.76 seconds;
- process trace capture observed `mode=process`; Phase 2 plans report `execution=embedded` per engine stage.

The OCRS model digests remain those recorded in `docs/phase-2-pr.md`; models were temporary and are not committed or acquired by a default build.

## Validation commands

The final gate passed locked metadata, workspace integrity and dependency-boundary checks, formatting, all-target checking, the full default test suite, strict Clippy, `xtask doctor`, all three artifact generators, offline smoke, and a clean generated-artifact diff. Separate commands passed mock fault/conformance tests, layout wrapper parity, and model-backed OCRS wrapper parity.

Remote PR creation and repository settings were not performed. The phase is merged locally after these gates pass.
