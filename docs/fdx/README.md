# Ferrodoc progressive document contracts

Ferrodoc's FP0/FDX0 boundary is the generic, electronics-agnostic substrate for progressive document enrichment. The normative implementation is in `ferrodoc-ir`; [`FDX0_EVIDENCE_GRADE_IR.md`](FDX0_EVIDENCE_GRADE_IR.md) records the persistent identity and materialization rules.

The phase boundary is deliberate:

- FP0/FDX0 defines immutable `EvidenceDelta` artifacts, content-identifiable `DocumentStateManifest` states, evidence-grade geometry/table spans, and page-qualified targets.
- FP1 executes capabilities against those scopes through the runtime contract in
  [`FDX1_SCOPED_EXECUTION.md`](FDX1_SCOPED_EXECUTION.md).
- FP2 produces cheap surveys and full-document baseline states with honest native geometry in
  [`FDX2_BASELINE_GEOMETRY.md`](FDX2_BASELINE_GEOMETRY.md).
- FP3 adds the bounded targeted-table structure slice in
  [`FDX3_TARGETED_TABLES.md`](FDX3_TARGETED_TABLES.md); formula, figure, and finer-geometry
  specialists remain later work.
- [`FDX4_DURABLE_STATE_REUSE.md`](FDX4_DURABLE_STATE_REUSE.md) defines the implemented runtime
  provider seam, immutable delta/state/checkpoint persistence, state-aware cross-worker reuse,
  checkpoint policy hooks, canonical replay equivalence, and exact storage accounting.
- [`FDX5_EXPLAINABLE_PARETO.md`](FDX5_EXPLAINABLE_PARETO.md) defines hard-admitted local
  alternatives, fixed-point value/quality/cost uncertainty, cache and prerequisite explanations,
  targeted-versus-whole-document escalation, and conservative Pareto retention.

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
cargo run --locked -p ferrodoc-pdf --example export_survey_snapshots
```
