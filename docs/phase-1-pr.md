# Phase 1: foundations, IR, and workspace consolidation

## Goal and baseline

Establish stable semantic contracts and the reduced target workspace from Phase 0 merge `8e6e44f23f15840679efe44f46a41085ca0249a0`.

## Summary

- Replaced the recovered core monolith with validated modules for geometry, canonical names, exact quantities, typed SHA-256 digests, stable IDs, explicit estimates, device/backend/placement axes, scoped blobs, model manifests, schema compatibility, and separated deterministic/observational provenance.
- Added the versioned evidence IR with pages, transforms, layers, artifacts, regions, reading-order DAGs, typed payloads, append-only evidence, and reason-coded selected views.
- Added the synchronous transport-independent engine API and structured error taxonomy.
- Added protocol schema types without process I/O and explicit runtime, PDF, render, and CLI skeletons.
- Added model-manifest and IR JSON Schema snapshots plus a canonical IR golden fixture.
- Added property/fuzz-style tests and enforced runtime-agnostic dependency boundaries in CI.
- Documented architecture, IR versioning, package consolidation, and transport-independent engines.

## Packages

Added `ferrodoc-ir`, `ferrodoc-engine-api`, `ferrodoc-protocol`, `ferrodoc-runtime`, `ferrodoc-pdf`, `ferrodoc-render`, and the `ferrodoc` CLI package. `ferrodoc-core` remains and was intentionally broken at its pre-release API boundary. No package was renamed in place and no placeholder package was added.

## Deviations

None. The process protocol has semantic messages, a version range, fixed preamble, and maximum frame declaration, but framing and process I/O remain in Phase 3 as planned. PDF parsing and Markdown/HTML rendering remain in Phase 2.

## Migration, risk, and rollback

The old public core API was not valid enough to preserve. Callers must use checked constructors and handle `Result`, explicit `Estimate::Unknown`, algorithm-specific digests, `DeviceId`/`BackendId`/`PlacementPolicy`, and `ScopedBlob` rather than paths. Geometry uses `f64` with serde_json exact float round trips and carries coordinate space and units.

The principal risk is schema churn while v0.2 remains pre-release. Major/minor compatibility is explicit, schema snapshots are checked, and the canonical fixture detects byte drift. Rollback is a normal revert of the ordered Phase 1 commits.

## Acceptance evidence

| Criterion | Evidence |
|---|---|
| Phase 1 crates compile without heavy integrations | Locked workspace check and tests pass; `scripts/check-boundaries.sh` rejects OCR, PDF parser, GPU/model, HTTP, and async-runtime dependencies from semantic crates. |
| Core, IR, and engine API remain runtime-agnostic | Their manifests contain only serde/schema/hash/error dependencies and internal semantic crates; boundary script passes. |
| Geometry operations pass property tests | Properties cover IoU symmetry, clipping containment, expansion containment, translation invariance, and serialization; fixed tests cover invalid floats, negative dimensions, touching edges, and extreme margins. |
| Canonical textual representations agree | Capability, profile, and region-kind tests compare `Display`, `FromStr`, serde, and schema values; device/backend/media types have validated canonical string serialization. |
| Invalid and overflowing quantities are rejected | Exact fixed-point parsing distinguishes SI/IEC, rejects signs, NaN/infinity text, overflow, excessive/impossible precision, and checked arithmetic failure. |
| Unknown resources cannot become zero | Every `ResourceEstimate` field defaults to tagged `Estimate::Unknown`; serialization regression test rejects implicit zero. |
| Engine schemas contain no host paths | `EngineRequest` carries `ScopedBlob`; a schema regression test rejects path fields and requires `BlobId`/`BlobRange`. |
| Golden IR is byte deterministic | `fixtures/document-ir-v1.json` deserializes, validates, and reserializes byte-for-byte; serde_json exact float round trips are enabled. |
| Persistent schemas are versioned and snapshotted | `schemas/document-ir-v1.json` and `schemas/model-manifest-v1.json` match generated Rust schemas in tests. |
| Public types and invariants are documented | Crate/module/type/field documentation plus `docs/architecture.md`, `docs/ir.md`, and ADRs describe contracts and planned boundaries. |

## Validation

All commands passed locally on 2026-08-19 with Rust 1.95.0:

```text
cargo metadata --locked --format-version 1 > /dev/null
./scripts/check-workspace.sh
./scripts/check-boundaries.sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
./scripts/smoke.sh
git diff --exit-code
```

The suite contains 47 unit/integration/property tests plus doc tests. The smoke command reruns the complete workspace with `CARGO_NET_OFFLINE=true` and verifies the truthful CLI status.

GitHub-hosted CI was not executed locally because remote publication is not authorized. The local merge is the implementation phase boundary; remote review and branch-protection settings remain release operations.

## Follow-up

Phase 2 adds real PDF inspection/rasterization, rule-based layout, CPU OCRS, orchestration, Markdown rendering, CLI conversion commands, deterministic fixtures, and development tests against representative PDFs from `~/research/lightbulb`.
