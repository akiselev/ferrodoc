# FDX0 — Evidence-grade native geometry and table structure

Status: planned

Owner: Ferrodoc

Depends on: merged/green v0.2 baseline

Consumed by: Foundry FLS5/FLS6 and Datasheet-cli FDX1+

## Objective

Make Ferrodoc DocumentIR precise enough to serve as low-cost, auditable supervision for domain-specific document models without changing Ferrodoc into an electronics parser.

The phase has four goals:

1. recover and retain the finest defensible native PDF text geometry available;
2. make table cells evidence-bearing rather than text-only logical cells;
3. reconstruct common born-digital tables cheaply and deterministically before escalating to learned/vision engines;
4. support selective region/page refinement as a new immutable evidence generation rather than rerunning or mutating an entire document.

## Current baseline to preserve

The existing IR already has the correct high-level model:

```text
Document
  Page
    SourceLayer[]
    Region[]
      Evidence[]
      SelectedView?
    reading_order[]
```

and supports `RegionKind::Table`, `RegionKind::TableCell`, `EvidenceContent::Table`, provenance, geometry and competing native/OCR/layout evidence.

FDX0 extends precision and provenance. It does not replace the evidence graph with a flattened convenience model.

## Deliverable 1: explicit geometry quality

Add a generic, serialized geometry-quality type in the lowest appropriate Ferrodoc crate, conceptually:

```rust
pub enum GeometryQuality {
    Glyph,
    Word,
    Line,
    Region,
    PageOnly,
    Unknown,
}
```

Exact naming may change during implementation, but the semantics are required:

- `Glyph`: geometry corresponds to an individual glyph/character;
- `Word`: a word/token box assembled from defensible glyph/text state;
- `Line`: one text line/baseline region;
- `Region`: a coarser semantic/layout region;
- `PageOnly`: the content is known to belong to the page but not to a narrower defensible rectangle;
- `Unknown`: geometry precision cannot be characterized.

Do not infer precision from rectangle area. Geometry quality is producer evidence.

Propagate quality anywhere a downstream caller can otherwise confuse an approximate box for exact source geometry.

## Deliverable 2: positioned native PDF text

Replace the current page-sized single-span happy path wherever the underlying parser/interpreter can defensibly recover finer positions.

Target logical record:

```rust
pub struct NativeTextSpan {
    pub text: String,
    pub geometry: PageRect,
    pub geometry_quality: GeometryQuality,
    pub source_order: u32,
    // optional source-character mapping if implementable without unstable parser internals
}
```

### Implementation research order

Investigate the existing stack before adding another PDF runtime:

1. Hayro syntax/interpreter text-state APIs and callbacks;
2. lopdf content stream operations and text matrices;
3. whether a narrow internal content-stream walker can recover text positioning while reusing lopdf objects/fonts;
4. only then evaluate another Rust PDF extraction dependency.

Do not add MuPDF/Poppler merely for convenience. FDX0 should keep the default Rust-native/offline boundary unless measurements demonstrate an unacceptable limitation.

### Required PDF text semantics

Where recoverable, account for:

```text
BT/ET text objects
Tm / Td / TD / T* text positioning
Tj / TJ show-text operations
font size / horizontal scale
character and word spacing
text rise
page CTM and rotation/crop transforms
font encoding / ToUnicode mapping
```

Glyph-width calculation must be based on the actual font metrics/encoding available from the file; do not use monospaced approximations and label them `Glyph`/`Word` quality.

If decoding succeeds but geometry cannot be made precise, retain the text with `PageOnly` or another honest coarser quality.

### Text ordering

Keep PDF source-order evidence separate from downstream reading-order hypotheses. Native stream order is useful evidence but is not automatically semantic reading order.

## Deliverable 3: native text evidence granularity

DocumentIR should retain native text evidence at a useful granularity without exploding storage unnecessarily.

Preferred hierarchy:

```text
source-native glyph/word positions (internal or compact evidence metadata)
        |
        v
line/span evidence records
        |
        v
layout regions / table reconstruction
```

Benchmark at least word-level versus line-level persisted evidence before committing to per-glyph top-level `Evidence` records. Per-glyph data may be represented compactly inside an evidence payload/metadata structure if that preserves exact provenance with materially lower overhead.

The logical schema must still permit an evidence anchor to resolve a table cell or text range to the exact source spans that support it.

## Deliverable 4: evidence-bearing table cells

Extend the generic table representation so one logical cell can point to source evidence.

Conceptually:

```rust
pub struct EvidenceSpanRef {
    pub evidence_id: EvidenceId,
    pub text_start: Option<u32>,
    pub text_end: Option<u32>,
}

pub struct TableCell {
    pub row: u32,
    pub column: u32,
    pub row_span: u32,
    pub column_span: u32,
    pub text: String,
    pub geometry: Option<PageRect>,
    pub geometry_quality: GeometryQuality,
    pub source_spans: Vec<EvidenceSpanRef>,
}
```

Exact type placement may differ, but these invariants are mandatory:

1. a cell's source spans resolve inside the same pinned IR generation;
2. source references survive deterministic serialization;
3. cell geometry is not reported more precisely than its source evidence permits;
4. reconstructed cell text can be reproduced from/reconciled against referenced source text;
5. overlapping/merged cells retain all supporting source spans;
6. a table hypothesis may exist without a selected view.

## Deliverable 5: deterministic born-digital table recognizer

Add a model-free table recognizer using positioned text evidence.

It should target regular digital tables first, especially those with aligned textual columns. The recognizer is generic and must not hard-code electronics headings.

### Candidate algorithm

```text
positioned words/lines
  |
  +--> detect dense alignment bands / whitespace gutters
  +--> cluster baselines into row hypotheses
  +--> cluster x extents/anchors into column hypotheses
  +--> detect repeated column occupancy
  +--> infer header/body boundary candidates
  +--> infer row/column spans from geometric overlap/gaps
  +--> assign source spans to candidate cells
  +--> score structural consistency
  |
  +--> accept high-confidence grid
  +--> retain ambiguous hypotheses / abstain otherwise
```

Features may include:

```text
x-left/x-center/x-right alignment
baseline/y overlap
inter-word gaps relative to local font size
repeated x anchors across rows
row-height regularity
column occupancy consistency
horizontal/vertical ruling lines when available
font/style transitions if already exposed
page/region whitespace boundaries
```

Do not require visible ruling lines; many datasheet tables are whitespace-aligned.

### Abstention

The deterministic recognizer must expose why it refused a table. Examples:

```text
insufficient positioned evidence
multiple near-equal grids
excessive overlap
irregular reading order
page-only geometry
unsupported rotated text
```

A downstream planner can then request a stronger engine only for those regions.

## Deliverable 6: table capability engine

Expose the deterministic recognizer through the existing engine abstraction under `Capability::TableRecognize` rather than hard-wiring it into the PDF crate.

Preferred new engine:

```text
engines/ferrodoc-table-rulebased/
```

It must be:

- deterministic;
- CPU-only initially;
- network-free;
- explicit about unknown quality/resource evidence;
- executable embedded and, if the generic transport contract permits without special cases, over process isolation;
- independently benchmarkable against alternative table engines.

The PDF crate produces source evidence; the table engine interprets positioned evidence into structure.

## Deliverable 7: selective refinement API

Add a generic request/result contract for appending evidence to selected page/region targets.

Conceptually:

```rust
pub struct RefinementRequest {
    pub base_document_digest: Sha256Digest,
    pub targets: Vec<RefinementTarget>,
    pub capabilities: Vec<Capability>,
    pub policy: RefinementPolicy,
}

pub enum RefinementTarget {
    Page(PageId),
    Region(RegionId),
}
```

The exact transport may use existing engine/runtime request structures instead of these literal types.

Required behavior:

1. verify the pinned base IR/input identity;
2. execute only requested/admissible capabilities/targets;
3. append new source/evidence layers with full deterministic provenance;
4. reconcile/select only through an explicit new selected view/generation;
5. return a new canonical DocumentIR identity;
6. preserve the old generation unchanged;
7. reuse deterministic stage cache entries when semantic inputs match.

A caller must be able to refine one difficult table without re-OCRing unrelated pages.

## Deliverable 8: optional learned table-engine evaluation boundary

Do not make a learned table model part of the FDX0 acceptance gate unless deterministic extraction demonstrably fails on the proving corpus.

Prepare an evaluation path for external/learned engines using the existing engine API. Candidate research baselines include PubTables-1M/Table Transformer-style structure recognition.

Admission rules:

- fixed protected corpus;
- identical case accounting;
- deterministic baseline retained;
- separate structure accuracy, evidence alignment, latency and resource measurements;
- model digest included in provenance/cache identity;
- region-level routing preferred to full-document execution.

A learned table engine that improves recall but breaks evidence alignment is not an acceptable default for FDX.

## Deliverable 9: compact canonical serialization and compatibility

Any schema extension must retain deterministic canonical JSON and explicit schema-version compatibility.

If fine-grained geometry materially expands IR size, benchmark compression before inventing a second semantic representation. The Foundry FLS5 plan owns the eventual `FDocPack` physical-storage gate; FDX0 should not solve that by changing logical semantics.

## Corpus and fixtures

Add retained fixtures covering at least:

1. simple born-digital prose with word/line geometry;
2. two-column page;
3. regular table without ruling lines;
4. ruled table;
5. merged header cells;
6. multi-line cells;
7. rotated page;
8. page with native text that decodes but lacks precise geometry;
9. scanned/image-only table requiring OCR/refinement;
10. at least several real electronics datasheet pages retained under acceptable fixture rights.

The electronics fixtures validate generic geometry/table quality only; no electronics semantics enter Ferrodoc tests.

## Benchmarks

Measure:

```text
native extraction wall time
native extraction peak memory when practical
words/lines with exact vs coarse geometry
IR canonical JSON bytes
IR Zstd bytes
rule-table candidate count
accepted table count
cell text accuracy
cell source-alignment validity
row/column structure accuracy on labeled fixtures
abstention rate/reasons
selective-refinement latency vs whole-document rerun
```

For real documents, report results by document family rather than only a corpus-wide average.

## Tests

Required:

- text geometry remains inside transformed page bounds;
- page rotation/crop transform round-trips correctly;
- exact word/glyph geometry is never emitted from page-only evidence;
- source order is deterministic;
- canonical serialization of fine-grained evidence is stable;
- table cells reference existing evidence IDs/ranges;
- table cell text/source-spans are internally consistent;
- ambiguous table fixture causes deterministic abstention rather than arbitrary grid selection;
- same source evidence produces byte-identical deterministic rule-table output;
- refinement of one region leaves unrelated page evidence unchanged;
- old base IR remains valid/resolvable after refinement;
- cache identity changes on engine/model/schema/parameter changes;
- malformed source-span references fail IR validation.

## Acceptance criteria

FDX0 is complete when:

1. representative born-digital PDFs produce word/line or finer geometry where the source stack genuinely supports it;
2. files without precise positioning remain explicit `PageOnly`/coarse evidence rather than fake rectangles;
3. a retained real datasheet specification table becomes structured cells whose source spans resolve to exact native evidence;
4. the deterministic table recognizer has a measured protected-corpus baseline and abstains on ambiguous cases;
5. a caller can selectively refine one ambiguous region/page and receive a new immutable IR generation;
6. old evidence IDs/IR generations remain independently resolvable;
7. canonical IR/storage growth is measured and acceptable or recorded as an input to Foundry's FLS5 physical-storage gate;
8. the implementation introduces no electronics-specific semantics into Ferrodoc.

## Landed

Record exact commits, parser/text-position strategy, benchmark corpus, geometry coverage, table metrics and any deferred learned-engine decision after implementation.
