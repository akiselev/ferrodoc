# Ferrodoc progressive document contracts

Ferrodoc's FP0/FDX0 boundary is the generic, electronics-agnostic substrate for progressive document enrichment. The normative implementation is in `ferrodoc-ir`; [`FDX0_EVIDENCE_GRADE_IR.md`](FDX0_EVIDENCE_GRADE_IR.md) records the persistent identity and materialization rules.

The phase boundary is deliberate:

- FP0/FDX0 defines immutable `EvidenceDelta` artifacts, content-identifiable `DocumentStateManifest` states, evidence-grade geometry/table spans, and page-qualified targets.
- FP1 will execute capabilities against those scopes.
- FP2 will produce survey and baseline profiles from real PDFs.
- FP3 will add targeted structure/precision engines.
- FP4 will persist/cache deltas and checkpoints and benchmark replay.

FP0/FDX0 does not define electronics predicates, quantities, regimes, training policy, Artifactum storage, or a corpus scheduler. Those remain owned by Datasheet-cli and Foundry.

The conformance schemas and compact golden vectors are:

- `schemas/evidence-delta-v1.json`
- `schemas/document-state-manifest-v1.json`
- `fixtures/evidence-delta-v1.json`
- `fixtures/document-state-manifest-v1.json`

Regenerate them explicitly with:

```bash
cargo run --locked -p ferrodoc-ir --example export_snapshots
```
