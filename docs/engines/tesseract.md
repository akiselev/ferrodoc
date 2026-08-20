# Tesseract CPU engine

- Package/ID: `ferrodoc-engine-tesseract`, `ocr.tesseract`; enabled in the CLI with feature `tesseract` and selected by `--ocr-engine tesseract`.
- Dependencies: Tesseract 4 or newer discovered dynamically through its stable C API. The Rust build does not link a native library. Platform candidates are explicit; missing library, unsupported version, language initialization, and traineddata paths produce dependency diagnostics.
- Models: configured language traineddata is located through the initialized API and SHA-256 hashed before the engine becomes healthy.
- Capabilities: `ocr.page`, `ocr.region` on bounded RGBA8 input.
- Devices/network: CPU through `tesseract-c-api`, no network, deterministic, maximum concurrency 1.
- Estimate: conservative 1 GiB peak RAM, 384 MiB warm RAM, zero VRAM/cost; latency and calibrated quality remain unknown. Input is capped at 100 million pixels.
- Isolation: process transport is recommended because an in-flight native recognition call cannot cooperatively stop; embedded/process output parity is tested. A successfully loaded native library stays pinned for process lifetime because some OpenMP-enabled builds retain teardown references.
- Licenses: Ferrodoc MIT OR Apache-2.0; Tesseract Apache-2.0 and traineddata license/provenance are deployment responsibilities.
- Benchmark status: the optional CI job requires dependency health, inference, common conformance, process parity, exact scanned-fixture text, and two successful fixed real-corpus conversions. Reports include the actual traineddata digest and explicit unknown RAM/VRAM.
