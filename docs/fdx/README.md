# Ferrodoc progressive document contracts

Ferrodoc's FP0/FDX0 boundary is the generic, electronics-agnostic substrate for progressive document enrichment. The normative implementation is in `ferrodoc-ir`; [`FDX0_EVIDENCE_GRADE_IR.md`](FDX0_EVIDENCE_GRADE_IR.md) records the persistent identity and materialization rules.

The phase boundary is deliberate:

- FP0/FDX0 defines immutable `EvidenceDelta` artifacts, content-identifiable `DocumentStateManifest` states, evidence-grade geometry/table spans, and page-qualified targets.
- FP1 executes capabilities against those scopes through the runtime contract in
  [`FDX1_SCOPED_EXECUTION.md`](FDX1_SCOPED_EXECUTION.md).
- FP2 will produce survey and baseline profiles from real PDFs.
- FP3 will add targeted structure/precision engines.
- FP4 will persist/cache deltas and checkpoints and benchmark replay.

FP0/FDX0 does not define electronics predicates, quantities, regimes, training policy, Artifactum storage, or a corpus scheduler. Those remain owned by Datasheet-cli and Foundry.

The conformance schemas and compact golden vectors are:

- `schemas/evidence-delta-v1.json`
- `schemas/document-state-manifest-v1.json`
- `fixtures/evidence-delta-v1.json`
- `fixtures/document-state-manifest-v1.json`
- `schemas/protocol-request-v1.json`
- `fixtures/protocol/v1/legacy-execute-request.bin`
- `fixtures/protocol/v1/scoped-execute-request.bin`

Regenerate them explicitly with:

```bash
cargo run --locked -p ferrodoc-ir --example export_snapshots
cargo run --locked -p ferrodoc-protocol --example export_fixtures
```
