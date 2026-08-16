# Known limitations

- The local assembly container has no Rust toolchain and no outbound package resolution. GitHub CI is therefore the compile/test authority for this branch; the repository includes checks, tests, Clippy, and smoke coverage.
- Model manifests that track upstream `main` should be pinned to immutable revisions and hashes for reproducible production use.
- Hayro is the default pure-Rust PDF rasterizer. PDFium remains an optional compatibility fallback for PDFs Hayro does not yet render correctly.
- The generic ORT plugin deliberately uses an explicit tensor/output contract rather than guessing arbitrary ONNX preprocessing and tokenization. Reusable model families should receive typed adapters or use OAR's higher-level implementations.
- `ferrodoc-engine-oar` provides native Candle document VLM families; its custom CUDA kernels require SM80+. Older NVIDIA systems should use OAR classic/ORT, CPU VLM inference, or llama.cpp's supported backend instead.
- Tesseract no longer requires the `tesseract` executable, but the direct-FFI plugin still needs a loadable Tesseract/Leptonica shared library and tessdata. A vendored-source feature is a possible future packaging option.
- The foundry currently emphasizes deterministic raster truth and scan degradation. HTML/Chromium, Typst/LaTeX, office-document conversion, multilingual font packs, vector-PDF truth, real scan backgrounds, and licensed public corpora remain expansion areas.
- The research optimizer includes global random exploration plus elite/TPE-like sampling. Additional optimizers can implement the same research/ledger interface.
- Remote engines require credentials from environment/configuration and never place secrets in the model store.
- `ferrodoc-engine-command` and the research command evaluator intentionally remain subprocess escape hatches. They are not used by official native engine execution paths.
