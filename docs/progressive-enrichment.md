# Progressive evidence enrichment

Status: proposed post-v0.2 implementation program

## Objective

Turn Ferrodoc from a conversion-oriented runtime into an incremental document-understanding substrate that can cheaply establish baseline evidence for very large corpora and then refine selected pages/regions as compute becomes available.

The public logical model remains the evidence-preserving `Document` IR. The physical execution/persistence model gains immutable evidence deltas and content-identifiable document states so a refinement does not require serializing another complete document snapshot.

## Design invariants

1. Refinement is append-only. Engines add hypotheses/evidence; they do not delete or overwrite prior evidence.
2. Native PDF text, OCR, layout, tables, formulas, figures and semantic refinements remain distinguishable by `SourceLayer` and deterministic provenance.
3. A materialized canonical DocumentIR is a checkpoint/view over a state, not the required output of every stage.
4. Expensive stages operate on document/page/region scope and declare the capabilities they add.
5. Cache identity excludes observations such as timestamps and host names.
6. Equivalent logical document state has the same identity regardless of stage execution order.
7. Resource accounting is first-class because corpus schedulers must compare information value against actual cost.
8. Ferrodoc does not embed electronics-specific semantics. Domain consumers request capabilities and interpret document evidence externally.

## New contracts

### EvidenceDelta/v1

Add a schema-owned immutable type in `ferrodoc-ir` or a narrowly adjacent persistence contract:

```rust
pub struct EvidenceDelta {
    pub schema: SchemaVersion,
    pub source: ArtifactDigest,
    pub stage: StageIdentity,
    pub producer: ProducerIdentity,
    pub scope: Scope,
    pub required_state: Option<DocumentStateId>,
    pub required_evidence: Vec<EvidenceId>,
    pub source_layers: Vec<SourceLayer>,
    pub regions: Vec<Region>,
    pub evidence: Vec<Evidence>,
    pub selection_hints: Vec<SelectionHint>,
    pub diagnostics: Vec<Diagnostic>,
    pub coverage: Vec<CapabilityCoverageDelta>,
}
```

Exact field names may differ, but the contract must distinguish deterministic semantic content from run observations/resource measurements.

`Scope` supports document, pages and regions. Geometry remains in source/page coordinate systems already defined by the IR.

Delta validation must reject:

- references to absent prerequisites;
- source identity mismatch;
- duplicate IDs with incompatible content;
- invalid/out-of-bounds geometry;
- cyclic reading-order additions;
- stage output claiming unsupported capabilities.

### DocumentStateManifest/v1

```rust
pub struct DocumentStateManifest {
    pub schema: SchemaVersion,
    pub source: ArtifactDigest,
    pub deltas: Vec<EvidenceDeltaId>,
    pub reconciliation_policy: ArtifactDigest,
    pub coverage: Vec<CapabilityCoverage>,
    pub parents: Vec<DocumentStateId>,
    pub checkpoint: Option<ArtifactDigest>,
}
```

The state ID is a domain-separated digest over canonicalized semantic fields. Delta order must not affect identity when the logical set is the same.

A state is resolvable to a normal `Document` by loading its deltas/checkpoint and running deterministic reconciliation.

### Capability coverage

Track coverage independently of selected evidence. At minimum:

```text
capability
scope
status: absent | candidate | partial | complete | failed
producer/stage identity
quality summary
unresolved diagnostics
```

Initial capability vocabulary should include:

```text
pdf_native_text
page_raster
ocr_text
coarse_layout
reading_order
precise_text_geometry
table_detection
table_structure
formula_detection
formula_recognition
figure_detection
figure_caption_link
pinout_visual
mechanical_drawing_visual
timing_diagram_visual
```

Keep producer-defined capability extension possible without breaking older readers.

## Execution API changes

The runtime planner should accept a goal rather than assuming whole-document conversion:

```rust
pub struct EnrichmentRequest {
    pub source: BlobId,
    pub input_state: Option<DocumentStateId>,
    pub goals: Vec<CapabilityGoal>,
    pub scope: ScopeHint,
    pub policy: ExecutionPolicy,
}
```

Planning returns either:

```text
AlreadySatisfied
NoAdmissiblePlan { reasons }
CandidatePlans { pareto[] }
```

A candidate plan lists stage invocations, dependencies, cache hits, expected capability gains, expected quality, resource estimates and monetary/network/privacy implications.

Execution emits one or more `EvidenceDelta` artifacts plus an observational trace. It does not have to materialize a complete `Document` unless requested.

## Built-in enrichment profiles

Profiles are conveniences expressed as capability goals.

### `survey`

- inspect PDF/container;
- native extraction;
- page dimensions/count;
- page-level text/image/vector density;
- scan/born-digital/hybrid hints;
- language/script hints;
- cheap document-family features;
- deterministic duplicate/near-duplicate features.

### `baseline`

- inexpensive OCR over all text-bearing pages/regions;
- keep OCR and native text separately;
- coarse layout;
- basic reading order;
- heading/paragraph/list candidates;
- line/span geometry;
- table/formula/figure candidates;
- confidence/disagreement diagnostics.

The baseline should be optimized for corpus throughput and recall. It is explicitly not the maximum-quality OCR configuration.

### `structure`

- table structure/cells;
- formula recognition;
- precise reading order;
- figure/caption association;
- hierarchy refinement.

### `precision`

- high-resolution OCR on selected scopes;
- alternate OCR engines/models;
- language-specific OCR;
- geometry refinement;
- disagreement resolution.

### `visual`

- figures requiring visual interpretation such as pinouts, mechanical drawings, timing diagrams, charts and application schematics.

### `deep`

- VLM/high-cost engines on explicit scopes only.

## Stage cache evolution

The current deterministic stage cache becomes the natural local cache for enrichment work. Extend keys so they include:

```text
source digest
input state/prerequisite evidence identity
stage/engine/model/config identity
scope identity
schema
seed where applicable
```

Cache outputs are immutable `EvidenceDelta` artifacts rather than opaque final-conversion fragments where possible.

A cache hit must be reusable by both embedded and process execution paths and must preserve exact producer/model identity.

## Materialization

Add a deterministic materializer:

```text
DocumentStateManifest + deltas + reconciliation policy
                     |
                     v
              canonical Document
                     |
           canonical JSON / renderers
```

Materialization should support checkpoint acceleration without changing logical output. Tests must prove that resolving from a checkpoint plus tail deltas yields identical canonical bytes to resolving all deltas from scratch.

Do not add a custom binary archive format until benchmarks show canonical JSON + Zstd/checkpoints are insufficient.

## Cost/quality observations

Every stage execution records observational measurements separately from semantic identity:

```text
stage/engine/model
scope size/pages/pixels/tokens
wall/cpu time
peak RAM
peak VRAM where measurable
bytes read/written
remote monetary cost
cache status
success/failure category
coverage before/after
```

These traces must be exportable as a corpus scheduler training/evaluation dataset.

## Post-v0.2 implementation phases

Each phase is one PR unless this document is explicitly amended.

### FP0 - Incremental state contracts

- `EvidenceDelta/v1` schema and canonical serialization;
- `DocumentStateManifest/v1` and `DocumentStateId`;
- capability coverage vocabulary;
- validation and migration rules;
- materialize a state containing native + mock OCR evidence;
- schema snapshots and conformance tests.

Acceptance: two independent deltas can be applied in either order and produce the same state identity/canonical materialized Document when semantically independent.

### FP1 - Capability-scoped runtime execution

- `EnrichmentRequest` and goal/scope planning;
- stage descriptors declare produced/required capabilities;
- page/region-scoped execution;
- planner responses for already-satisfied/no-plan/Pareto plans;
- deterministic delta outputs;
- process protocol extensions with bounded compatibility fixtures.

Acceptance: request table structure for two regions without executing unrelated OCR/layout stages on the rest of the document.

### FP2 - Corpus baseline profile

- `survey` and `baseline` profiles;
- full-document low-cost OCR orchestration;
- native/OCR disagreement retained;
- coarse layout/reading order/table-formula-figure candidates;
- baseline checkpoint materialization;
- throughput/bytes-per-page benchmark suite.

Acceptance: run a mixed born-digital/scanned/hybrid corpus end-to-end and report CPU seconds/page, peak memory, evidence bytes/page and useful text/layout coverage.

### FP3 - Progressive structure and precision

- table structure/cell engine integration;
- formula refinement path;
- higher-resolution/alternate OCR scopes;
- precise text geometry refinement;
- figure/caption association;
- diagnostic-driven follow-up goals.

Acceptance: improve selected benchmark regions without recomputing unchanged pages and retain both old/new hypotheses.

### FP4 - Durable artifact-backed cache integration

Define a storage-provider abstraction so Foundry can persist deltas/state/checkpoints in Artifactum without adding Artifactum as a hard dependency of Ferrodoc's semantic crates.

- import/export immutable deltas;
- shared cache identity across workers;
- checkpoint compaction policy hooks;
- safe missing/corrupt artifact handling;
- storage amplification benchmarks.

Acceptance: two workers processing the same deterministic stage converge on one semantic artifact identity and the second can reuse it.

### FP5 - Goal/value planner surface

Expose enough candidate-plan information for a corpus orchestrator to make global decisions:

- expected capability gain;
- benchmark-derived quality estimate;
- resource/cost estimates;
- cache-hit state;
- prerequisites;
- confidence/source of estimates.

Ferrodoc does not decide global corpus priority. It returns an explainable local Pareto frontier.

Acceptance: external caller can compare two admissible plans for the same goal without executing either.

### FP6 - Foundry integration validation

Maintain a cross-repo fixture/contract test proving:

- PDF -> survey -> baseline -> state artifact;
- targeted structure/precision delta;
- old evidence anchors remain resolvable;
- external datasheet semantic extraction pins state/evidence identities;
- materialized selected view improves after refinement without destroying prior evidence.

### FP7 - Deep/visual engines

Only after corpus scheduling and benchmark evidence identify useful gaps:

- document VLM engine packs;
- diagram/chart/pinout/mechanical-drawing engines;
- generic ONNX engine packs where justified;
- region-level router improvements.

Every deep engine must have a cheaper baseline/control and report quality improvement per unit cost.

## Benchmark gates

For each stage family report:

```text
quality metric(s)
CPU seconds/page or region
GPU seconds/page or region
peak RAM/VRAM
artifact bytes/page
cache hit latency
cold execution latency
coverage gain
failure rate by document family
```

For the baseline profile add corpus-oriented metrics:

```text
time-to-searchable-document
text recall / character error where truth exists
layout region recall
useful provenance geometry coverage
cost per 1k pages
```

A new expensive stage is not promoted into a default profile merely because it improves a single quality metric. It must improve the declared Pareto frontier for a real workload.

## Relationship to the existing v0.2 architecture

This program extends, rather than replaces, the existing design:

- the evidence graph invariant remains unchanged;
- `SelectedView` remains a deterministic reconciliation projection;
- embedded/process engine semantics remain transport-independent;
- hard resource admission and scheduler leases remain in the runtime;
- the current stage cache becomes the basis for immutable enrichment caching;
- canonical DocumentIR remains the stable complete logical representation;
- routing remains guarded by hard planner policy.

The principal change is that conversion is no longer the only orchestration unit. Ferrodoc can now answer incremental capability requests against an existing document state.
