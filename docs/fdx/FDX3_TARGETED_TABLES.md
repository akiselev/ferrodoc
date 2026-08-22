# FDX3 targeted table contract

FP3 lands a bounded table-structure slice over the FP0 state/delta contract and the FP1 scoped
execution boundary. It does not introduce a second document IR or claim general scanned-table
quality.

## Atomic input and output

A table invocation has exactly one `(page_id, region_id)` target. The immutable source PDF remains
the engine blob and pins source provenance. The runtime additionally serializes only text evidence
owned by that target region into the normalized `ferrodoc.source_text_evidence` parameter. Each
entry contains its stable evidence ID, exact UTF-8 text, and existing geometry/quality. The process
protocol therefore carries the same semantic request as embedded execution and never exposes a
host path. This source context participates in invocation/cache identity but is removed from
persisted producer configuration, avoiding a second copy of document text in every output record.

The reference `table.rulebased` engine recognizes deliberately small grammars: two or more
consistent rows and columns with nonempty `|`-separated cells, plus a bounded born-digital
`Name Description <identifier> <sentence>` fragment. The latter exists for the FLS5 retained-real-
document gate and reconstructs only the header plus one source row; it is not a general
two-column-table detector. The engine emits `EvidenceContent::Table` on a new region-owned layer.
Every cell records an exact UTF-8 byte span into the pre-existing text hypothesis. Table and cell
geometry inherit the source geometry and quality; the engine never manufactures cell boxes.
Byte-identical native/layout hypotheses produce one table in source order, retaining the more
conservative native geometry. Limits are 4 MiB of target text, 4,096 rows, 256 columns, and 65,536
cells; the description fragment is additionally capped at 4 KiB with a 64-byte identifier.

## Materialization and validation

The runtime rejects a table span unless it names text evidence in the exact page-qualified target
region. Referenced source IDs become `EvidenceDelta.required_evidence_ids`. Canonical
materialization then reuses the FP0 validation that checks UTF-8 boundaries, source content,
concatenated cell text, table extents, and geometry honesty. The old text hypothesis remains in the
region and unrelated pages receive no delta additions.

## Acceptance evidence

- `fixtures/table/pipe-table-v1.json` is a minimized grammar and byte-span oracle, not a quality
  corpus.
- `fixtures/table/name-description-fragment-v1.json` is the minimized second-grammar span oracle.
  `fixtures/table/rp2040-table1-oracle-v1.json` retains only source/page/evidence hashes and expected
  cells for the rights-reviewed external RP2040 PDF; no document bytes are checked in. Set
  `FERRODOC_TEST_RP2040_PDF` to that exact artifact to run the optional real-document gate.
- `ferrodoc-table-rulebased` runs the common deterministic engine conformance suite.
- Runtime integration proves exact target ownership, retained old hypotheses, stable deltas/state,
  evidence-grade validation, byte-identical unchanged-page JSON, and cold-miss/warm-hit semantic
  equivalence through the existing deterministic stage cache.
- Embedded/process parity is a required CI gate for the table executable.
- The portable job compiles and tests the engine/runtime slice on Linux, macOS, and Windows.

No checked-in representative datasheet or scanned-table truth corpus is policy-approved in this
phase. The retained RP2040 oracle proves one born-digital table fragment and cannot be interpreted
as corpus recall or association accuracy. Consequently ordering/package/pin/electrical-table recall,
CPU/GPU seconds per representative region, peak measured RAM/VRAM, cold/warm cache latency,
coverage gain, and failure rate by document family remain environmental benchmark gates. The
static engine estimate is a conservative scheduling envelope, not a measurement or quality score.
Formula recognition, figure/caption association, learned scanned-table recognition, and precise
cell geometry are explicitly outside this bounded slice.
