# Qualified engine portfolio

Ferrodoc v0.2 keeps a small explicit portfolio. Every `Engine` implementation runs the shared conformance contract for descriptor validity, canonical capabilities, dependency health, sourced estimates, deterministic execution, cancellation, and structured unsupported-operation errors. Transport-capable official engines also have embedded/process parity checks.

The default `cpu-minimal` plus `process-engines` feature set is pure Rust, offline, and requires no system library. `tesseract` enables runtime discovery of the optional native C API. `nvml` enables the optional hardware probe. The experimental command engine is a separately configured escape hatch and is never selected by default.

- [Native PDF extraction](native-pdf.md)
- [Rule-based layout](layout-rulebased.md)
- [Bounded rule-based tables](table-rulebased.md)
- [OCRS CPU OCR](ocrs.md)
- [Tesseract CPU OCR](tesseract.md)
- [Deterministic mock](mock.md)
- [Experimental command escape hatch](command.md)

`scripts/engine-qualification.sh` produces ephemeral versioned benchmark reports on the fixed real regression corpus. Reports retain exact executable, corpus, model, configuration, and toolchain identities. Default CI expects one native success and an explicit scanned failure without models. The optional model/native jobs require both cases to convert and preserve RAM/VRAM as unknown when process attribution is unavailable.
