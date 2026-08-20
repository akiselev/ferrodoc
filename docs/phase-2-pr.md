# Phase 2 validation evidence

Baseline: `90f7266`  
Branch: `phase-2/minimal-conversion`

## Delivered architecture

- bounded lopdf inspection and native extraction plus deterministic Hayro rasterization;
- deterministic born-digital, image-only, hybrid, malformed, encrypted, and rotated/cropped fixtures;
- model-free rule-based layout and direct-Rust OCRS/RTen CPU engines behind `Engine`;
- embedded per-page quality routing, append-only evidence, reconciliation, plans, and traces;
- deterministic Markdown, semantic HTML, and canonical evidence JSON;
- split CLI argument, configuration, command, and atomic-output modules.

OCRS receives explicit model bytes and performs no acquisition. The optional CI job downloads the upstream example model pair separately and verifies both digests before the model-backed test.

## Development corpus evidence

The corpus inventory under `/home/dev/research/lightbulb` contained 425 PDFs. Ferrodoc itself inspected and rasterized two representative inputs:

- born-digital OSTI 4130843: 132,706 bytes, four pages, 2,255 native characters on page 1, deterministic 744 by 995 RGBA raster at 96 DPI;
- image-only OSTI 7094593: 187,901 bytes, six pages, zero native characters on every page, deterministic 819 by 1,052 page-1 raster at 96 DPI.

The checked-in image-only fixture recovered exactly `SCANNED FERRODOC PAGE` and `Optical text survives the CPU path.` through OCRS. The hybrid model-backed test confirmed independent `native_pdf`, `layout`, and `ocr` layers. The two-fixture CLI test completed in 98.47 seconds in the debug profile.

The first page of image-only OSTI 7094593 did not finish OCR within a three-minute debug-build probe and was terminated. This is retained as performance evidence, not represented as a passing real-corpus OCR result; measurement and optimization belong in Phases 4 and 5.

The first end-to-end OSTI 4130843 conversion exposed floating-point accumulation in layout bands: the last region could extend fractionally beyond a non-divisible page height. The implementation now derives the last band from the exact remaining height, has a regression test, and successfully converts all four pages (5,028 bytes before paragraph-line joining and 4,780 bytes after it).

Model digests used for development and optional CI:

- detection: `f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca`;
- recognition: `e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e`.

## Acceptance notes

- Default builds and conversion perform no network, model, GPU, native-library, or system-Tesseract acquisition.
- Born-digital pages reject OCR when native quality meets the configured threshold.
- Scan and hybrid conversion are covered by an explicit model-backed CI job.
- Native, layout, and OCR hypotheses remain provenance-bearing separate layers.
- Repeated born-digital conversion yields byte-identical selected IR and rendered output.
- CLI failures are structured for missing, malformed, encrypted, unsupported/limited, configuration, and unavailable-model cases.
- `plan` inspects the actual input and reports selected, rejected, or unavailable OCR per page.
- README quick-start commands are exercised by the offline smoke script.

Remote PR creation and repository settings were not performed; this phase is merged locally after all gates pass.
