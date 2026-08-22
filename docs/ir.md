# Evidence IR

## Invariant

The IR is an evidence graph, not a flattened document. Native PDF extraction, OCR, layout analysis, and later refinements append distinct `Evidence` records associated with `SourceLayer` provenance. `SelectedView` references the evidence chosen by deterministic reconciliation and records a machine-readable reason. Competing evidence remains inspectable.

## Identity and geometry

Document, page, layer, artifact, region, and evidence IDs are domain-separated SHA-256 identities derived from deterministic, length-prefixed inputs. They do not include timestamps or random run IDs.

Every region carries a `PageRect` with a zero-based page index, validated rectangle, explicit coordinate space and unit, and source transform. Evidence additionally declares honest `GeometryQuality`; page-only evidence must use the complete page bounds, and precise quality cannot omit geometry. Validation rejects regions or evidence outside page bounds, incompatible geometry, duplicate identities, missing layer/artifact/evidence references, and cyclic reading-order graphs.

## Payloads

The v1 graph distinguishes text, structured table cells, LaTeX formulas, image artifacts, and producer-defined unknown payloads. Table cells retain geometry quality and exact UTF-8 source spans whose reconstructed text must match the cell. An unknown payload retains its media type and JSON value rather than being silently dropped.

## Progressive states

FP0/FDX0 adds immutable `EvidenceDelta` and `DocumentStateManifest` contracts alongside the complete `Document` view. Deltas retain page/region ownership for layers and evidence and page ownership for render artifacts and reading-order edges. State identity is the canonical evidence-delta set plus source, IR schema, and reconciliation policy; parent lineage, coverage summaries, checkpoint choice, and physical representation are excluded.

`materialize_state` and `materialize_from_checkpoint` produce the same canonical DocumentIR for an equivalent logical state. Region refinement always uses `(page_id, region_id)` because region IDs are page-local. The full contract and phase boundary are documented in [`fdx/FDX0_EVIDENCE_GRADE_IR.md`](fdx/FDX0_EVIDENCE_GRADE_IR.md).

FP4 persists these same canonical contracts through a replaceable runtime storage-provider seam.
Physical delta, retained-manifest, and checkpoint realizations are validated separately from their
logical IDs; deterministic refinement reuse is pinned to the exact input state and producer
identity. See [`fdx/FDX4_DURABLE_STATE_REUSE.md`](fdx/FDX4_DURABLE_STATE_REUSE.md).

## Versioning

Persistent documents contain `{major, minor}` schema versions. A different major version requires migration or a matching reader. Higher minor versions may add fields; serde readers ignore unknown fields within the same major version. Unknown enum-like producer data belongs in the explicit `Unknown` variants because adding a new closed enum tag is not backward compatible.

Schema snapshots are checked in as `schemas/document-ir-v1.json`, `schemas/evidence-delta-v1.json`, `schemas/document-state-manifest-v1.json`, and `schemas/model-manifest-v1.json`. `cargo run -p ferrodoc-ir --example export_snapshots` is the explicit regeneration command, and tests compare the public Rust schemas with all snapshots.

## Canonical JSON

`Document::to_canonical_json` validates the complete graph and serializes compact UTF-8 JSON. Struct field order is declared in code, maps use `BTreeMap`, and serde_json's exact float-round-trip mode is enabled. `fixtures/document-ir-v1.json` must deserialize, validate, and reserialize byte-for-byte. Observational metadata is a separate core type and is not present in this fixture.
