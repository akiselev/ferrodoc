# Ferrodoc

Ferrodoc is a Python-free, hardware-aware document extraction runtime and CLI written in Rust. It treats PDF-to-Markdown as a compilation problem: acquire native evidence first, cheaply analyze a page, route only difficult regions to OCR/VLM engines, reconcile competing evidence, then render Markdown/JSON/HTML.

The project is deliberately split into a small stable host and dual-mode Cargo plugins. Every official engine is an ordinary Rust library implementing the same `Engine` trait. The default CLI discovers thin `ferrodoc-*` executable wrappers on `$PATH` and speaks a versioned length-prefixed CBOR protocol for crash/dependency isolation; applications can instead link the same engines directly through `ferrodoc-batteries` with zero IPC. Rust dynamic-library ABI compatibility is not required and heavyweight inference stacks never enter the core dependency graph. Official engines do not shell out to secondary OCR/model CLIs.

## Status

Ferrodoc 0.2 implements the native-runtime architecture end to end: the IR, planner, resource scheduler, dual-mode plugin SDK/host, PDF ingestion, rendering, logical model views, pipeline, foundry, benchmark harness, tiny router training, experiment database/optimizer and official engine libraries are present. The local assembly environment cannot resolve crates.io, so the checked-in GitHub CI is the compile/test authority for dependency/API compatibility and runs `cargo check`, tests, Clippy and the end-to-end smoke path.

## Why this differs from Marker

Marker 2 already uses a strong hybrid strategy, including native PDF text, layout analysis and VLM fallback. Ferrodoc pushes the boundary further: the planner is region- and hardware-aware; every engine is replaceable; native/OCR/VLM hypotheses are retained as provenance-bearing evidence; reading order is a DAG; low-VRAM allocation is a hard planning constraint; and benchmark/autoresearch infrastructure is part of the product rather than an external script collection.

Primary implementation references:

- Marker: https://github.com/datalab-to/marker
- llama.cpp multimodal runtime: https://github.com/ggml-org/llama.cpp
- mistral.rs multimodal runtime: https://github.com/EricLBuehler/mistral.rs
- `ort`: https://github.com/pykeio/ort
- Burn: https://github.com/tracel-ai/burn
- `ocrs`: https://github.com/robertknight/ocrs
- OAR-OCR: https://github.com/GreatV/oar-ocr
- Hayro: https://github.com/LaurenzV/hayro
- PaddleOCR-VL 1.6 GGUF: https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6-GGUF
- Surya OCR 2 GGUF: https://huggingface.co/datalab-to/surya-ocr-2-gguf

## Architecture

```text
PDF / image
    |
    +--> deterministic acquisition (lopdf/PDFium/renderer)
    |       native text, page geometry, images/vectors
    |
    +--> cheap analysis (native quality + optional layout plugin)
    |       |
    |       +--> tiny learned router
    |       +--> heuristic fallback
    |
    +--> hardware-aware planner
    |       capability + quality + latency + RAM + VRAM + network cost
    |
    +--> selected engines (same Rust Engine trait)
    |       embedded OR process-isolated Cargo transport
    |       classical OCR / neural OCR / ONNX / native Rust / VLM / remote
    |
    +--> evidence-preserving IR
    |       competing hypotheses + provenance + reading-order DAG
    |
    +--> reconciliation
    |
    +--> Markdown / JSON / HTML
```

See `docs/ARCHITECTURE.md`, `docs/PLUGIN_PROTOCOL.md`, `docs/LOW_VRAM.md`, `docs/FOUNDRY.md`, and `docs/AUTORESEARCH.md`.

## Workspace

Core crates:

- `ferrodoc-core`: stable capability, geometry, resource, blob/model types.
- `ferrodoc-ir`: evidence-oriented document IR and reconciliation.
- `ferrodoc-protocol`: CBOR process protocol.
- `ferrodoc-plugin-sdk`: transport-neutral engine trait and thin stdio server adapter.
- `ferrodoc-plugin-host`: `$PATH` discovery plus shared process/embedded `EngineClient` lifecycle.
- `ferrodoc-pdf`: born-digital extraction plus render abstraction.
- `ferrodoc-planner`: quality/cost/resource selection.
- `ferrodoc-scheduler`: RAM/VRAM/CPU/remote admission control.
- `ferrodoc-model-store`: XDG content-addressed artifacts plus immutable logical model-directory views.
- `ferrodoc-router`: tiny trainable Rust MLP.
- `ferrodoc-pipeline`: adaptive page/region execution.
- `ferrodoc-foundry`: deterministic synthetic document generation and degradation.
- `ferrodoc-bench`: binary-verification benchmark harness.
- `ferrodoc-research`: SQLite experiment ledger + hybrid optimizer.
- `ferrodoc-batteries`: optional feature-gated direct embedding of official engines.
- `ferrodoc`: CLI.

Reference plugins:

| Plugin | Class | Runtime boundary | Intended use |
|---|---|---|---|
| `ferrodoc-engine-tesseract` | classical OCR | direct Tesseract C API | cheap CPU OCR without the Tesseract executable |
| `ferrodoc-engine-ocrs` | native Rust neural OCR | `ocrs` + RTen crates | pure-Rust CPU neural OCR |
| `ferrodoc-layout-rulebased` | deterministic/layout | built-in Rust image analysis | zero-model preflight |
| `ferrodoc-engine-oar-classic` | neural OCR | OAR-OCR + ONNX Runtime | PP-OCR on CPU/older CUDA GPUs |
| `ferrodoc-engine-ort` | ONNX | direct `ort` API | typed CPU/CUDA/TensorRT/OpenVINO ONNX models |
| `ferrodoc-engine-burn` | native Rust model | Burn/Flex | first-party learned router and imported/native Burn models |
| `ferrodoc-engine-oar` | local document VLM | OAR-OCR-VL + Candle | PaddleOCR-VL/GLM-OCR/Ovis/Monkey/Hunyuan/MinerU |
| `ferrodoc-engine-llamacpp` | local VLM | direct `libllama` + `libmtmd` | GGUF, Vulkan/CUDA/Metal, partial low-VRAM offload |
| `ferrodoc-engine-mistralrs` | local VLM | mistral.rs Rust SDK | Rust/Candle local multimodal runtime |
| `ferrodoc-engine-remote` | remote VLM | `reqwest` | OpenAI-compatible high-accuracy fallback |
| `ferrodoc-engine-mistral` | remote Document AI | `reqwest` | Mistral OCR endpoint |
| `ferrodoc-engine-command` | adapter | arbitrary executable | deliberate research/new-engine escape hatch |
| `ferrodoc-engine-mock` | test | built-in | protocol/integration tests |

## Bootstrap

```bash
rustup toolchain install 1.97.1
rustup override set 1.97.1
cargo build --workspace
cargo test --workspace

# Install the host and useful lightweight plugins.
cargo install --path crates/ferrodoc-cli
cargo install --path plugins/ferrodoc-layout-rulebased
cargo install --path plugins/ferrodoc-engine-tesseract
cargo install --path plugins/ferrodoc-engine-ocrs
cargo install --path plugins/ferrodoc-engine-oar-classic
cargo install --path plugins/ferrodoc-engine-oar
cargo install --path plugins/ferrodoc-engine-llamacpp
cargo install --path plugins/ferrodoc-engine-mistralrs
cargo install --path plugins/ferrodoc-engine-remote
```

Or run `scripts/install-dev.sh`. The workspace also defines `cargo xtask doctor`, `just check`, and `just smoke`; the smoke path builds the host + deterministic mock plugin, verifies process discovery, generates a tiny foundry corpus, and executes the benchmark pipeline.

PDF rendering defaults to Hayro in-process, so the normal PDF path needs neither Poppler nor MuPDF. PDFium is an optional compatibility renderer.

## First runs

```bash
ferrodoc hardware
ferrodoc plugins list
ferrodoc plugins doctor

ferrodoc convert paper.pdf -o paper.md
ferrodoc convert paper.pdf --profile low-vram --gpu-budget 1500MiB -o paper.md
ferrodoc convert paper.pdf --profile low-vram \
  --device layout.detect=cpu --device ocr.page=cuda:0 \
  --engine-param llamacpp.gpu_layers=12 \
  --engine-param llamacpp.mmproj_offload=false
ferrodoc explain paper.pdf --page 7 --profile low-vram
ferrodoc inspect paper.pdf > paper.ir.json
```

An engine can be forced per capability:

```bash
ferrodoc convert scan.pdf \
  --engine ocr.page=tesseract \
  --engine layout.detect=layout-rulebased
```

For a llama.cpp document VLM, install a model manifest and map it to the plugin:

```bash
ferrodoc models pull models/paddleocr-vl-1.6-gguf.toml
ferrodoc convert scan.pdf \
  --profile low-vram \
  --engine ocr.page=llamacpp \
  --model llamacpp=paddleocr-vl-1.6
```

The low-VRAM profile defaults to a 1536 MiB VRAM planning budget and caps that against live free NVIDIA memory queried through NVML. Engines are estimated separately on each advertised device, and failed/incompatible estimates are removed before planning, so CPU and GPU candidates compete on the same quality/cost/resource objective. The llama.cpp adapter automatically derives a conservative partial-offload layer count from the budget, while `--engine-param` can override `gpu_layers`, `mmproj_offload`, and `image_max_tokens` for calibration/autoresearch.

## Document foundry

```bash
ferrodoc foundry generate ./tmp/foundry --count 500 --seed 123
```

Each generated page has a pristine image, a degraded image, exact region truth, reading-order truth, formula/table truth and binary assertions. It uses an embedded bitmap font so corpus generation has no system-font dependency.

## Benchmarking

Install at least one OCR plugin, then:

```bash
ferrodoc bench run ./tmp/foundry/manifest.json \
  --profile cpu \
  --engine ocr.page=tesseract \
  --output ./tmp/tesseract.json

ferrodoc bench compare ./tmp/baseline.json ./tmp/tesseract.json
```

The benchmark data model records binary-test quality, wall time and pages/sec and is designed to grow with memory, VRAM, visual-token, API-cost and energy probes.

## Tiny learned router

The router is intentionally small enough to train without Python or a tensor framework. It is an 18-feature, one-hidden-layer MLP whose classes are native text, classical OCR, neural OCR, local VLM, remote VLM, specialized table and specialized formula.

```bash
ferrodoc router bootstrap ./tmp/router.jsonl --examples 10000
ferrodoc router train ./tmp/router.jsonl ./tmp/router.model.json --epochs 500
ferrodoc convert paper.pdf --router-model ./tmp/router.model.json
```

`router bootstrap` exists to exercise the complete training path. Real labels should come from benchmark sweeps: run every feasible engine/configuration on ground-truth documents, calculate quality/cost/resource outcomes, and assign the Pareto-optimal route.

## Autoresearch

```bash
# Create the fixed evaluator corpus once; do not regenerate it between trials.
ferrodoc foundry generate ./benchmarks/foundry --count 256 --seed 407912268
ferrodoc research run experiments/foundry-routing.toml
ferrodoc research best foundry-tesseract-routing
```

The optimizer sends candidate values as `FERRODOC_PARAM_*` environment variables and also expands `{PARAMETER}` placeholders in evaluator arguments. The included experiment therefore searches real Tesseract page-segmentation, OCR-engine, spacing, and box-output settings against a fixed foundry corpus. Numeric search uses a TPE-like elite sampler plus global exploration; structural/code candidates use the same SQLite ledger and Pareto objective via the library API.

The intended autonomous loop is:

```text
numeric parameters --------> TPE/CMA-ES-style optimizer --+
                                                         |
structural/code changes ---> coding/research agent -------+--> fixed benchmark --> ledger
                                                         |
prompts/preprocessing -----> either branch ---------------+
```

Do not let an agent select its own evaluation set. Keep a fixed held-out corpus and track quality, speed, RAM, VRAM and cost jointly.

## Adding an engine

An official-style plugin exports the engine from `lib.rs` and makes its binary a transport-only wrapper:

```rust
// lib.rs
pub struct MyEngine;
impl ferrodoc_plugin_sdk::Engine for MyEngine {
    fn descriptor(&self) -> PluginDescriptor { /* capabilities/devices */ }
    fn infer(&mut self, req: &InferenceRequest) -> anyhow::Result<InferenceOutput> { /* ... */ }
}

// main.rs
fn main() -> anyhow::Result<()> {
    ferrodoc_plugin_sdk::plugin_main(my_engine::MyEngine)
}
```

Name the binary `ferrodoc-engine-whatever` and the CLI discovers it. A Rust application can instead pass `MyEngine` to `Pipeline::add_embedded_engine`, avoiding IPC completely. Large page images are represented by `BlobRef` paths for isolated plugins rather than serialized through CBOR.

## Design rules

1. Never make `ferrodoc-core` depend on a model runtime.
2. Do deterministic/native extraction before raster inference.
3. Treat engines as evidence producers, not mutators of canonical truth.
4. Keep device placement and model residency decisions in the planner/scheduler.
5. Make every expensive inference cache-keyable by content/model/parameters.
6. Make every quality claim reproducible from a benchmark artifact.
7. Optimize Pareto frontiers, not a single “OCR score.”
8. Preserve intermediate evidence so routing/reconciliation experiments do not force model re-runs.
9. Official engines may use native libraries, but must not launch secondary OCR/model command-line tools.
10. Process isolation is a transport choice, not an engine API.
