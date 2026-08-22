# FDX0 evidence-grade IR and FP0 state contract

Status: implemented contract

## Durable model

`EvidenceDelta` is an immutable append-only artifact. It binds the exact source PDF and IR schema, deterministic producer/build/model/configuration identity, stage, processed scope, optional input-state precondition, required evidence, additions, reconciliation hints, diagnostics, and coverage observations. Resource measurements and run timestamps live outside the delta identity.

`EvidenceDelta::artifact_digest` identifies those exact retained bytes. `EvidenceDeltaId` is the logical evidence projection used by states: source/schema, stage, producer, additions, and selection hints. Execution scope, input-state preconditions, prerequisite declarations, diagnostics, coverage summaries, and physical render-blob locations remain in the immutable artifact but cannot rename identical logical evidence reached through another construction path.

Additions retain explicit ownership:

- `PageDelta` owns render artifacts and reading-order edges;
- `OwnedSourceLayer` distinguishes page-owned from `(page_id, region_id)`-owned layers;
- `RegionEvidenceAddition` owns evidence through a page-qualified containing page;
- new regions are nested under their containing page.

A delta cannot delete or replace existing pages, layers, artifacts, regions, evidence, or edges. Duplicate identities and invalid cross-references fail during canonical materialization.

## State identity

`DocumentStateManifest` retains a canonical set of `EvidenceDeltaId` values and derives:

```text
DocumentStateId = H(
  "ferrodoc-document-state/1",
  source_pdf_sha256,
  ir_schema,
  sorted unique evidence_delta_ids,
  reconciliation_policy_id
)
```

Coverage summaries, parent/merge states, checkpoint references, artifact locations, encoding, and compression are retained metadata but excluded from this identity projection. Consequently, execution order, lineage choice, checkpoint placement, and physical realization cannot rename an equivalent logical evidence state.

## Materialization

`materialize_state` applies a complete delta set to an initial canonical `Document`. `materialize_from_checkpoint` accepts the checkpoint's retained state manifest, verifies its source/schema and evidence-set prefix, binds non-empty checkpoints to the manifest's canonical DocumentIR digest, and applies only tail deltas while proving that checkpoint-prefix plus tail identities equal the requested manifest's complete evidence set. Both paths canonicalize page, layer, artifact, region, evidence, selection, and reading-edge order before validating and serializing DocumentIR.

Conformance tests prove:

- independent delta order yields identical state identity and canonical DocumentIR;
- full-delta replay equals checkpoint-plus-tail replay byte-for-byte;
- old evidence IDs remain resolvable in later states;
- lineage, coverage, and checkpoint realization do not affect `DocumentStateId`;
- malformed and future major schema tags fail closed.

## Page-qualified refinement

`RegionRefinementTarget` always binds `base_document_state_id`, `page_id`, `region_id`, and a nonempty capability set. Validation first resolves the page, then resolves the page-local region. A matching `RegionId` elsewhere in the document cannot satisfy the target.

`RegionId` therefore remains page-local. Durable callers must carry `PageRegionRef` rather than treating a bare region ID as a document-wide address.

## Evidence-grade geometry and tables

Every `Evidence` records explicit `GeometryQuality`: `glyph`, `word`, `line`, `region`, `page_only`, or `unknown`. Precise quality requires geometry; `page_only` geometry must equal the complete page bounds. Validation rejects evidence outside the page.

Each `TableCell` records row/column spans, reconstructed text, optional geometry, geometry quality, and one or more exact UTF-8 `TextSourceSpan` references. Validation requires:

- nonzero spans within table dimensions;
- every source ID resolves to text evidence on the same page;
- every byte range is ordered and lies on UTF-8 boundaries;
- concatenated source spans exactly equal reconstructed cell text.

Page-sized or unlocated text therefore cannot masquerade as precise cell evidence. Electronics-specific interpretation remains outside Ferrodoc.

## Versioning and boundaries

The persistent tags are `ferrodoc-evidence-delta/1` and `ferrodoc-document-state/1`. A different major tag is rejected rather than guessed. Geometry quality and cell source spans are additive DocumentIR fields: older v1 documents deserialize them as `unknown`/empty and remain readable, while FP0 state materialization applies the stricter evidence-grade validator and rejects cells without resolvable spans. The stable complete interchange/checkpoint representation remains canonical DocumentIR JSON; FP0 introduces no custom binary container.

This contract does not claim a targeted engine, durable artifact store, state-aware runtime cache, real-PDF survey baseline, or corpus performance result. Those are FP1-FP4 deliverables built against this contract.
