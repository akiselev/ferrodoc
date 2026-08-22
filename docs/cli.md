# CLI compatibility policy

Ferrodoc v0.2 stabilizes command names, precedence, exit status, and persistent JSON majors for the qualified path.

## Configuration precedence

For conversion options, an explicit command-line argument wins over its corresponding environment variable, which wins over the compiled default. The supported environment variables are `FERRODOC_NATIVE_CHARACTER_THRESHOLD`, `FERRODOC_OCR_DPI`, `FERRODOC_OCR_ENGINE`, `FERRODOC_OCRS_MODEL_DIR`, `FERRODOC_CACHE_DIR`, and `FERRODOC_MODEL_STORE`. Hard RAM/VRAM/cost/deadline limits and profiles are command-line only in v0.2. `--document-profile baseline` renders and OCRs every page that the cheap survey did not deterministically prove blank, including pages with high-quality native text; it therefore requires a healthy OCR engine and explicit model assets. OCRS is the default engine; Tesseract is accepted only by a binary built with its feature. An OCRS model directory is rejected with Tesseract.

Unknown options, invalid Unicode environment values, invalid ranges, and incompatible combinations fail rather than being ignored.

## Exit status and streams

- `0`: command completed successfully;
- `2`: argument, configuration, input, model, engine, runtime, rendering, serialization, or output failure.

Machine-readable failures are one JSON object on stderr using `ferrodoc-cli-error/1`; diagnostics do not appear on stdout. Successful JSON commands write one complete object on stdout. `convert` writes the requested document format and uses an atomic replacement when `--output` is supplied.

The checked schemas are `schemas/cli-error-v1.json` and `schemas/cli-plan-v1.json`. Evidence JSON uses the checked document-IR schema. Hardware, plugin doctor, router, research, model, benchmark, and corpus persistent objects use their owning crate's versioned schema or tagged evidence contracts.

## Deprecation

During v0.2, additive JSON fields and new optional flags may be introduced in a minor release. Removing or renaming a command/field, changing precedence, changing an exit status, or changing the meaning of an existing field requires a new schema/protocol major and a release-note migration entry. Deprecated commands or fields receive at least one minor release of warning before removal unless retaining them would violate security or integrity.
