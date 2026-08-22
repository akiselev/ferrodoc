# Marker parity assessment

This document records what “Marker parity” means for Ferrodoc, the evidence used
to assess it, and the implementation order needed to reach it. Marker is a useful
competitive reference, not Ferrodoc's architecture or correctness oracle.

## Reference snapshot

- Repository: [datalab-to/marker](https://github.com/datalab-to/marker)
- Reviewed commit: [`e1a6226adfaab4cd573cfa96e12d60905ee38036`](https://github.com/datalab-to/marker/commit/e1a6226adfaab4cd573cfa96e12d60905ee38036), 2026-08-07
- Reviewed release: [v2.0.0](https://github.com/datalab-to/marker/releases/tag/v2.0.0), 2026-07-20
- Local reference clone: `./marker`, excluded from Ferrodoc by `.gitignore`

Marker's code is Apache-2.0. Its model weights use a modified OpenRAIL-M
license with additional commercial terms, so code parity does not imply that
Ferrodoc can redistribute or select those weights by default. Any adopted model
must still pass Ferrodoc's source, digest, license, and runtime-policy gates.

## Executive finding

Ferrodoc already has the stronger trust and orchestration foundation: immutable
inputs and model identities, append-only evidence, transport-independent engines,
hard resource admission, scheduler leases, deterministic caching, and protected
benchmark provenance. Marker is substantially ahead in document intelligence and
product surface.

The largest quality gap is not “better OCR.” It is the combined digital-document
path:

1. character-aware native PDF extraction;
2. learned layout and reading order;
3. page and block quality classification;
4. table, formula, image, list, and reference reconstruction;
5. selective VLM repair only where native evidence is inadequate;
6. structural rendering to Markdown, HTML, JSON, and RAG chunks.

A single CUDA OCR engine would leave most of the observed gap intact. The first
high-return work is native geometry plus learned layout and deterministic
structure processing. A document VLM follows as a selectively routed specialist.

## What Marker v2 provides

Marker's main pipeline is providers -> builders -> processors -> renderers ->
converters. Its PDF path combines:

- PDFium-backed native characters, font/style flags, geometry, links, references,
  forms, page images, and heuristics for bad embedded text;
- a small RF-DETR layout model in fast mode;
- a Surya v2 document VLM for balanced-mode layout, OCR, reading order, table
  recognition, and selective repair;
- low-resolution layout rendering and demand-driven higher-resolution rendering
  for OCR, equations, and tables;
- processors for line merging, headings, lists, code, blockquotes, tables, forms,
  equations, handwriting, headers/footers, marginalia, references, footnotes,
  TOC construction, and optional LLM refinement;
- Markdown, HTML, hierarchical JSON, flattened chunks, OCR JSON, extracted images,
  metadata, and table-of-contents output;
- PDF and image input by default, plus DOCX, PPTX, XLSX, HTML, and EPUB through
  optional dependencies;
- single-file and batch CLIs, page ranges, pagination, worker sharing, resume,
  sharding, a Python API, a small HTTP server, and a Streamlit UI.

The current [Marker documentation](https://github.com/datalab-to/marker#usage)
describes two useful operating points:

- `fast`: native PDF text plus lightweight learned layout, with selective VLM use;
- `balanced`: VLM layout and aggressive page repair, intended for a GPU.

Those are useful product profiles, but Ferrodoc should express them as planner
outcomes from explicit estimates and policy rather than implicit device defaults.

## Upstream benchmark evidence

Marker reports results on the third-party olmOCR benchmark: 1,403 single-page
PDFs and roughly 8,400 tests across eight categories. Its [published v2 table](https://github.com/datalab-to/marker#benchmarks)
reports:

| Marker mode | Overall | Digital subset | Reported B200 throughput |
|---|---:|---:|---:|
| balanced | 76.0 | 83.5 | 2.9 pages/s |
| fast | 66.6 | 71.6 | 7.4 pages/s |
| fast, OCR disabled | 43.6 | 55.8 | 23.7 pages/s |

These are upstream claims, not Ferrodoc certification. The throughput numbers are
concurrent B200 measurements and do not predict latency on this workstation. The
[Marker benchmark harness](https://github.com/datalab-to/marker/blob/master/benchmarks/README.md)
also applies output normalization before evaluation. Ferrodoc must run both tools
against an identical pinned corpus, evaluator, mode, hardware inventory, and
complete-case manifest before making parity claims.

## Local same-document smoke comparison

On 2026-08-21, Marker at the pinned commit was installed into `marker/.venv` with
the frozen lockfile and run in CPU `fast --disable_ocr` mode. This deliberately
tests the digital-text/layout path without starting Surya. Outputs are temporary
under `/tmp/marker-parity-output`; they are evidence for planning, not a checked-in
benchmark.

| Document | Ferrodoc output | Marker output | Ferrodoc wall | Marker wall / reported conversion |
|---|---:|---:|---:|---:|
| 3-page résumé | 4,531 B, 15 lines, 2 headings | 4,059 B, 66 lines, 16 headings | 2.745 s | 143.227 s cold / 31.158 s |
| 30-page HMCAD1511 datasheet | 83,505 B, 6,812 lines, 2,222 headings | 114,869 B, 1,190 lines, 95 headings | 7.364 s | 41.932 s / 28.853 s |
| 10-page Parasolid manual chapter | 20,344 B, 288 lines, 3 headings | 18,992 B, 194 lines, 20 headings | 4.146 s | 22.315 s / 6.869 s |

Observed Marker advantages:

- the datasheet contains recognizable sections, bullet lists, Markdown tables,
  and extracted figure/diagram assets instead of thousands of false headings;
- résumé sections and roles are much more usable;
- the manual has useful heading hierarchy, links, a reconstructed table, and an
  extracted diagram;
- paragraphs and reading order are generally more coherent.

Observed Marker defects:

- résumé bullets are still collapsed into very long paragraphs and some role or
  employer associations are lost;
- words such as “infrastructure-as-code” are incorrectly joined, while other text
  retains source typos or spacing defects;
- the manual collapses several lists, retains a noisy running header, and creates
  malformed links near page boundaries;
- datasheet tables are structurally useful but contain shifted, missing, or merged
  cells and repeated legal/footer text.

Marker therefore supplies a strong comparison point and defect corpus, not truth.

## CUDA and low-VRAM implications

The workstation has a 4 GiB GTX 1050 Ti Max-Q and approximately 2.4 GiB currently
free. Ferrodoc's `Profile::LowVram` declares a hard 2 GiB budget.

Surya v2's published model artifacts are already approximately 1.37 GB for the
safetensors model, or approximately 1.47 GB for the GGUF model plus multimodal
projection, before KV cache, image embeddings, allocator workspace, and server
overhead. Marker's NVIDIA path uses vLLM with a default 0.85 GPU memory utilization
and an 18k model context. That configuration should be rejected on this machine
unless a measured probe proves it fits; it is not a credible LowVram default.
See the official [Surya model](https://huggingface.co/datalab-to/surya-ocr-2),
[GGUF artifacts](https://huggingface.co/datalab-to/surya-ocr-2-gguf), and
[Surya implementation](https://github.com/datalab-to/surya).

The plausible low-VRAM route is a quantized llama.cpp/libmtmd engine with one
slot, bounded context and image resolution, configurable partial GPU offload,
and a conservative measured estimate. Partial offload consumes both host RAM and
VRAM, while the current `EngineCandidate` represents one device. Before claiming
this capability, Ferrodoc needs an allocation model that can reserve multiple
resources for one candidate without inventing a fake “hybrid” device.

Acceptance for this workstation should be:

- the planner rejects every candidate whose conservative peak exceeds 2 GiB VRAM;
- the selected engine reserves host RAM and VRAM before model load;
- NVML sampling verifies peak attributable VRAM and aborts on lease overrun;
- model, projection, quantization, GPU-layer count, context, image size, and token
  budget all enter estimate and cache identity;
- CPU-only, partial-offload, and any full-GPU candidate run the same fixed cases;
- quality, cold latency, warm latency, host RAM, VRAM, and failures remain separate
  metrics, retaining every Pareto point;
- low-VRAM is an independently qualified operating point, not an OOM retry path.

## Parity definition

“Parity” has four gates, in order:

1. **PDF quality parity:** on an identical public and local regression corpus,
   Ferrodoc is non-dominated by pinned Marker modes in quality, failures, latency,
   RAM, and VRAM. Category scores must be reported separately.
2. **Semantic parity:** tables, formulas, images, headings, lists, code, references,
   footnotes, reading order, and provenance are available in the IR and useful in
   rendered output. Presence of an enum or empty engine does not count.
3. **Output and workflow parity:** Markdown, HTML, tree JSON, flat chunks, images,
   metadata/TOC, page selection, batch resume, and sharding work end to end.
4. **Input parity:** images and the optional office/web formats work through
   bounded provider integrations. This is lower priority than correct PDFs.

Ferrodoc does not need to copy Marker internals or regress its own guarantees to
pass. In particular, parity excludes silent model downloads, unconstrained GPU
seizure, destructive replacement of native evidence, opaque resource estimates,
and a monolithic default dependency graph.

## Implementation sequence

### M0 — close v0.2 and freeze the comparison protocol

Finish Phase 8 release gates first. Pin the Marker commit, model digests, evaluator,
PDF digests, configuration, hardware inventory, and exact commands in a benchmark
manifest. Add an adapter for the public olmOCR benchmark without copying held-out
truth into ordinary development fixtures. Record cold and warm latency separately.

Exit: the same manifest produces complete Ferrodoc and Marker reports, and the
experiment ledger rejects any changed input, evaluator, model, or configuration.

### M1 — character-aware native PDF evidence

Preserve per-character and per-span geometry, font/style flags, links, rotation,
images, and page transforms. Improve word joining, dehyphenation, ligatures, and
space inference. Evaluate the current parser first; if PDFium is needed, isolate a
narrow native boundary rather than adding it to runtime-agnostic crates.

Exit: digital text is traceable to source characters; the three local PDFs no
longer exhibit Ferrodoc's current giant-paragraph, split-word, and heading-flood
failures before OCR is considered.

### M2 — learned layout and reading order engine

Add a dual-mode `ferrodoc-engine-layout-*` crate around a pinned detector and
reading-order model. It must implement `Engine`, declare honest CPU/CUDA candidates,
accept runtime-provided model blobs, expose useful health, and support embedded and
framed process execution. ONNX Runtime or a narrow native runtime belongs inside
the engine crate.

Exit: coherent multi-column order and calibrated regions for headings, text,
lists, tables, figures, equations, code, headers/footers, and marginalia; the
datasheet false-heading count becomes a regression assertion.

### M3 — quality classification and selective routing

Add deterministic pre-execution features for native-text quality, scan likelihood,
image coverage, layout confidence, table confidence, and garbled regions. Render a
low-resolution page for layout and higher-resolution pages or regions only on
demand. Route at page and region granularity while retaining native hypotheses.

Exit: clean digital pages do not invoke OCR/VLM; scanned pages and locally damaged
regions do; every selection records its reason and all expensive work is cacheable.

### M4 — structural reconciliation and processors

Implement line/span merging, paragraph and column formation, heading hierarchy,
lists, code and blockquotes, repeated-header/footer suppression, marginalia,
captions, references, footnotes, page breaks, and TOC generation as explicit
evidence/reconciliation stages.

Exit: structure is represented semantically in IR and survives Markdown and JSON;
processor golden tests cover the concrete local Marker defects listed above.

### M5 — tables, formulas, and image artifacts

Reconstruct digital tables from character geometry, then add specialist engine
capabilities for scanned/low-confidence tables and formulas. Preserve LaTeX,
structured cells, source geometry, image artifacts, captions, and confidence.
Forms, handwriting, charts, and image descriptions enter later through the same
contract rather than an all-purpose opaque string response.

Exit: independent table-cell and normalized-LaTeX metrics pass spatial association;
the datasheet tables and manual figure are useful end to end.

### M6 — selectively routed document VLM

Add a `ferrodoc-engine-surya` or model-neutral document-VLM engine using a direct
Rust API or narrow llama.cpp/libmtmd FFI, plus a thin protocol executable. Prefer a
shared process/server for volatile GPU state, but do not shell out to another model
CLI. Declare layout, OCR, table, formula, and quality capabilities independently.

Exit: balanced-quality scanned/math/table cases improve over M1-M5 without running
the VLM on clean regions; model acquisition remains in the model store and every
semantic parameter participates in cache identity.

### M7 — first-class multi-resource and low-VRAM placement

Extend candidate estimates and scheduler leases to express one execution using
host RAM plus a specific CUDA device. Implement llama.cpp partial-offload candidates
with explicit GPU-layer, context, image, slot, and token limits. Add guarded probes,
warm-residency accounting, and live enforcement.

Exit: a fixed workload completes under the declared 2 GiB VRAM hard limit on this
machine, or the candidate is explicitly rejected. No claim is made from an idle
allocation snapshot alone.

### M8 — renderers and operating surface

Add HTML, hierarchical JSON, flattened chunk, extracted-image bundle, metadata,
and TOC renderers. Add page ranges, deterministic batch naming, resume, complete
case accounting, and sharding. Add an embedding facade or local API only after the
underlying contracts stabilize.

Exit: downstream users can obtain the Marker-equivalent representations without
losing Ferrodoc provenance, and interrupted batches resume without silently
skipping failures.

### M9 — additional input providers and optional refinement

Add image input first. Add DOCX, PPTX, XLSX, HTML, and EPUB only as bounded provider
integrations with explicit dependency, parser, and sandbox policies. Optional local
or hosted LLM refinement is a separate engine/stage with privacy, network, model,
prompt, cost, and cache identity—not a hidden renderer behavior.

Exit: each format and refinement provider has real fixtures, conformance evidence,
resource accounting, and an explicit support status.

## Priority recommendation

After v0.2, execute M0-M4 before committing to a large VLM integration. That path
targets the clearest local failures, materially improves born-digital documents,
and creates the regions and quality signals needed to use an expensive VLM
selectively. Develop M6 and M7 together: an engine that cannot fit the supported
LowVram profile is a valid high-resource candidate, but it is not the low-VRAM
solution.

