# FDX2: survey, baseline, and geometry

Status: implemented baseline contract; qualified OCR quality remains an environmental gate.

## Scope

FDX2 adds the cheap survey and full-document baseline described by Foundry's document-enrichment design. It does not add table reconstruction, formula recognition, semantic figure interpretation, or durable Artifactum persistence.

`PdfDocument::survey` parses bounded PDF structure and content streams without rasterization or OCR. The deterministic report records the exact PDF digest, bytes and object count; page dimensions; native character density; text-show, XObject, and painted-path counts; born-digital/scanned/hybrid/blank hints; script hints; exact native-text hashes; token SimHash features for near-duplicate candidate generation; coarse family features; and high-value page candidates. XObject invocations are deliberately not called image counts because a `Do` operator may invoke a form.

`Converter::baseline` runs this survey before expensive work and then renders and invokes the configured `ocr.page` engine for every page not deterministically proven blank. This includes born-digital pages. Native, layout, and OCR layers remain distinct, and deterministic diagnostics retain native/OCR disagreement instead of replacing either hypothesis.

The result contains:

- the survey;
- selected DocumentIR plus the non-canonical execution/resource trace;
- one immutable document-scoped `EvidenceDelta`;
- a content-identifiable `DocumentStateManifest`;
- canonical DocumentIR checkpoint bytes identified by the manifest's physical checkpoint reference.

Replaying the baseline delta from the empty document produces the exact checkpoint bytes. The checkpoint, execution trace, coverage summary, and state lineage do not affect logical state identity.

## Geometry contract

The current `lopdf` native-text API recovers page text but does not expose qualified glyph, word, line, or region boxes. Native PDF evidence therefore carries the page bounds with `geometry_quality = page_only`. Rule-based layout regions remain separate layout evidence with `region` quality. OCR geometry is accepted only at the precision and coordinate space declared by the selected OCR engine. Image/pixel OCR geometry is validated against declared raster artifact dimensions, while PDF/point geometry is validated against PDF page bounds. Consumers must not draw a precise native highlight from page-only evidence.

## Acceptance and goldens

The checked-in purpose-built PDFs exercise born-digital, image-only scan, and hybrid orchestration. The FP2 regression engine is deterministic and exists only to prove that every nonblank fixture is rasterized and sent through the OCR boundary; it is not OCR-quality evidence. It emits the same image/pixel geometry shape as the real OCR adapters so contract tests do not hide coordinate-space failures. Tests also prove native/OCR separation, page-only native geometry, stable surveys, planner-consumable `complete`/`candidate` coverage (empty discovery does not claim success), a physical checkpoint digest, and full-delta materialization equivalence.

The survey schema and born-digital golden are:

- `schemas/pdf-survey-v1.json`
- `fixtures/pdf-survey-born-digital-v1.json`

Regenerate them with:

```bash
cargo run --locked -p ferrodoc-pdf --example export_survey_snapshots
```

The existing benchmark layer supplies repeated cold/warm wall and process CPU measurements, Unix peak RSS with explicit unknown values on unsupported platforms, artifact/quality/coverage metrics, and CLI qualification against an exact corpus. A publishable mixed-corpus FP2 report still requires an explicitly installed OCRS model pair or qualified Tesseract installation plus permitted truth data. Without those dependencies, CER, useful-text/layout coverage, model-backed CPU seconds per page, and OCR failure rate are unknown rather than zero or fixture-derived. GPU time/VRAM likewise remain unknown for this CPU baseline.

Environmental qualification uses the existing corpus and benchmark commands documented in `docs/benchmarking.md` and `scripts/engine-qualification.sh`. No model file, third-party PDF, or machine-specific timing snapshot is checked in by FDX2.
