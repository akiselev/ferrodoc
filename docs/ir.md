# Evidence IR

## Invariant

The IR is an evidence graph, not a flattened document. Native PDF extraction, OCR, layout analysis, and later refinements append distinct `Evidence` records associated with `SourceLayer` provenance. `SelectedView` references the evidence chosen by deterministic reconciliation and records a machine-readable reason. Competing evidence remains inspectable.

## Identity and geometry

Document, page, layer, artifact, region, and evidence IDs are domain-separated SHA-256 identities derived from deterministic, length-prefixed inputs. They do not include timestamps or random run IDs.

Every region carries a `PageRect` with a zero-based page index, validated rectangle, explicit coordinate space and unit, and source transform. Validation rejects regions outside page bounds, incompatible geometry, duplicate identities, missing layer/artifact/evidence references, and cyclic reading-order graphs.

## Payloads

The v1 graph distinguishes text, structured table cells, LaTeX formulas, image artifacts, and producer-defined unknown payloads. An unknown payload retains its media type and JSON value rather than being silently dropped.

## Versioning

Persistent documents contain `{major, minor}` schema versions. A different major version requires migration or a matching reader. Higher minor versions may add fields; serde readers ignore unknown fields within the same major version. Unknown enum-like producer data belongs in the explicit `Unknown` variants because adding a new closed enum tag is not backward compatible.

Schema snapshots are checked in as `schemas/document-ir-v1.json` and `schemas/model-manifest-v1.json`. `cargo run -p ferrodoc-ir --example export_snapshots` is the explicit regeneration command, and tests compare the public Rust schemas with both snapshots.

## Canonical JSON

`Document::to_canonical_json` validates the complete graph and serializes compact UTF-8 JSON. Struct field order is declared in code, maps use `BTreeMap`, and serde_json's exact float-round-trip mode is enabled. `fixtures/document-ir-v1.json` must deserialize, validate, and reserialize byte-for-byte. Observational metadata is a separate core type and is not present in this fixture.
