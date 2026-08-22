# FDX datasheet-extraction program: Ferrodoc responsibilities

Status: planned post-v0.2 cross-repository program

FDX is a cross-repository program for turning evidence-preserving document reconstruction into inexpensive electronics-specific extraction and training data. Ferrodoc owns only the generic document/evidence substrate. It must not acquire electronics-specific predicates, part-number semantics, distributor APIs, Foundry lake concepts, or model-training policy.

Companion plans:

- Foundry: `docs/datasheet-extraction/README.md`
- Datasheet-cli: `docs/fdx/README.md`

## Phase ownership

| Phase | Owner | Purpose |
|---|---|---|
| [FDX0](FDX0_EVIDENCE_GRADE_IR.md) | Ferrodoc | precise native evidence, evidence-bearing table cells, deterministic born-digital tables, selective refinement |
| FDX1 | Datasheet-cli | workspace/domain contracts and `DatasheetSketch` |
| FDX2 | Datasheet-cli | deterministic electronics compiler and baseline |
| FDX3 | Datasheet-cli + Foundry persistence | weak-supervision compiler and source-taxonomy mappings |
| FDX4 | Datasheet-cli | template propagation, synthetic corpus, protected evaluation |
| FDX5 | Datasheet-cli | compact IR-native model, RTen inference, active teacher |
| FDX6 | Datasheet-cli + Foundry FLS6 | production cascade and retraining/evaluation loop |

## Boundary

Ferrodoc should make the following possible without understanding what a voltage, current, pin, package or electrical regime means:

```text
PDF bytes
  |
  v
native text / raster / OCR evidence
  |
  v
layout regions + reading order
  |
  v
structured table hypotheses
  |
  v
validated DocumentIR
  |
  +--> exact evidence/source geometry
  +--> deterministic semantic rendering
  +--> selective page/region refinement
```

The downstream electronics compiler consumes this IR and is responsible for:

```text
section semantics
quantity parsing/normalization policy
predicate retrieval/linking
specification regime
part/package/variant applicability
claim assembly
training labels/models
```

## Why FDX0 is required

The current v0.2 CPU vertical slice intentionally favors a trustworthy generic baseline over rich layout semantics. Native PDF extraction may expose only page-level geometry for recovered text, and the rule-based layout engine is a basic heading/paragraph segmenter. That is sufficient for v0.2 plumbing but insufficient as precise row/cell training evidence.

FDX0 therefore improves generic evidence fidelity before downstream training treats the IR as supervision.

## Non-goals

FDX0 must not:

- add TI/DigiKey/Mouser/manufacturer API clients;
- add an electrical predicate ontology;
- parse units into electronics-specific property meaning;
- implement `ClaimBundle` or Foundry assertions;
- embed a general LLM/VLM in the ordinary document path;
- guarantee perfect tables for every PDF;
- fabricate precise geometry when the source/parser cannot justify it;
- mutate old DocumentIR generations in place.

## Compatibility with Ferrodoc principles

FDX0 continues the existing invariants:

- competing evidence remains append-only;
- native PDF and OCR evidence remain distinguishable;
- deterministic work is content-addressable/cacheable;
- unknown/unsupported precision is explicit;
- expensive engines remain planner-controlled and optional;
- default born-digital conversion remains network-free and CPU-capable;
- learned/stronger table engines must beat deterministic baselines on protected corpora before becoming default.

## Exit condition

Ferrodoc's FDX work is complete when a downstream consumer can take a born-digital electronics datasheet and obtain cell/line/word evidence that is precise where justified, explicitly imprecise otherwise, and selectively refine only ambiguous regions without losing old evidence identity.
