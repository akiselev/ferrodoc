use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::Instant,
};

use ferrodoc_bench::{
    BenchmarkReport, CandidateIdentity, CasePrediction, ComparisonPolicy, EvidenceValue,
    MetricConfig, PredictedRegion, PredictionDocument, PredictionOutcome, PredictionSet,
    ResourceReport, TimingReport, compare, evaluate,
};
use ferrodoc_core::{CURRENT_SCHEMA_VERSION, Sha256Digest};
use ferrodoc_foundry::{BlockKind, CorpusManifest, FormulaTruth};
use ferrodoc_ir::{Document, EvidenceContent, Region, RegionKind};
use serde::de::DeserializeOwned;

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run(arguments: Vec<std::ffi::OsString>) -> Result<(), String> {
    match arguments.as_slice() {
        [command, corpus_root, manifest, predictions, output] if command == "evaluate" => {
            let corpus_root = PathBuf::from(corpus_root);
            let manifest: CorpusManifest = read_json(Path::new(manifest))?;
            let predictions: PredictionSet = read_json(Path::new(predictions))?;
            let report = evaluate(
                &corpus_root,
                &manifest,
                &predictions,
                &MetricConfig::default(),
            )
            .map_err(|error| error.to_string())?;
            write_json(Path::new(output), &report)
        }
        [command, baseline, candidate, output] if command == "compare" => {
            let baseline: BenchmarkReport = read_json(Path::new(baseline))?;
            let candidate: BenchmarkReport = read_json(Path::new(candidate))?;
            let comparison = compare(&baseline, &candidate, ComparisonPolicy::default())
                .map_err(|error| error.to_string())?;
            write_json(Path::new(output), &comparison)
        }
        [command, report] if command == "verify-report" => {
            let report: BenchmarkReport = read_json(Path::new(report))?;
            report.validate().map_err(|error| error.to_string())
        }
        [command, corpus_root, manifest, executable, engine, model_dir, output]
            if command == "qualify-cli" =>
        {
            let corpus_root = PathBuf::from(corpus_root);
            let manifest: CorpusManifest = read_json(Path::new(manifest))?;
            let report = qualify_cli(
                &corpus_root,
                &manifest,
                Path::new(executable),
                &engine.to_string_lossy(),
                (model_dir != "-").then(|| Path::new(model_dir)),
            )?;
            write_json(Path::new(output), &report)
        }
        _ => Err(
            "usage: ferrodoc-bench evaluate <corpus-root> <manifest.json> <predictions.json> <report.json> | compare <baseline.json> <candidate.json> <comparison.json> | verify-report <report.json> | qualify-cli <corpus-root> <manifest.json> <ferrodoc-executable> <ocrs|tesseract> <model-dir|-> <report.json>"
                .into(),
        ),
    }
}

fn qualify_cli(
    corpus_root: &Path,
    manifest: &CorpusManifest,
    executable: &Path,
    engine: &str,
    model_dir: Option<&Path>,
) -> Result<BenchmarkReport, String> {
    if !executable.is_absolute() || !executable.is_file() {
        return Err("qualification requires an existing absolute Ferrodoc executable".into());
    }
    if !matches!(engine, "ocrs" | "tesseract") {
        return Err("qualification OCR engine must be ocrs or tesseract".into());
    }
    let executable_digest = Sha256Digest::of_file(executable).map_err(|error| error.to_string())?;
    let configuration_digest = Sha256Digest::of_bytes(
        &serde_json::to_vec(&(engine, model_dir.map(Path::to_path_buf), executable_digest))
            .map_err(|error| error.to_string())?,
    );
    let toolchain = Command::new(executable)
        .arg("--version")
        .output()
        .map_err(|error| format!("run Ferrodoc version: {error}"))?;
    if !toolchain.status.success() {
        return Err("Ferrodoc version command failed".into());
    }
    let toolchain = String::from_utf8_lossy(&toolchain.stdout)
        .trim()
        .to_string();
    let mut model_digest = None;
    let mut cases = Vec::with_capacity(manifest.cases.len());
    for case in &manifest.cases {
        let document = corpus_root.join(&case.document);
        let start = Instant::now();
        let mut command = Command::new(executable);
        command
            .args(["convert", "--format", "json", "--ocr-engine", engine])
            .arg(&document);
        if let Some(model_dir) = model_dir {
            command.arg("--ocrs-model-dir").arg(model_dir);
        }
        let output = command
            .output()
            .map_err(|error| format!("run Ferrodoc qualification case {}: {error}", case.id))?;
        let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
        let timing = Some(TimingReport {
            cold_wall_ms: EvidenceValue::Measured {
                value: wall_ms,
                method: "benchmark parent std::time::Instant around one CLI process".into(),
            },
            warm_wall_ms: EvidenceValue::Unknown {
                reason: "one isolated CLI process was executed; no warm sample".into(),
            },
            cold_cpu_ms: EvidenceValue::Unknown {
                reason: "child-process CPU attribution is unavailable".into(),
            },
            warm_cpu_ms: EvidenceValue::Unknown {
                reason: "no warm child-process sample".into(),
            },
        });
        let outcome = if output.status.success() {
            let document: Document = serde_json::from_slice(&output.stdout)
                .map_err(|error| format!("parse qualification IR for {}: {error}", case.id))?;
            model_digest = model_digest.or_else(|| document_model_digest(&document));
            PredictionOutcome::Success {
                prediction: prediction_from_ir(&document),
            }
        } else {
            PredictionOutcome::Failure {
                category: "conversion_failed".into(),
                message: format!("Ferrodoc CLI exited with {}", output.status),
            }
        };
        cases.push(CasePrediction {
            case_id: case.id.clone(),
            outcome,
            timing,
            resources: ResourceReport::default(),
        });
    }
    let predictions = PredictionSet {
        schema_version: CURRENT_SCHEMA_VERSION,
        corpus_digest: manifest.corpus_digest,
        candidate: CandidateIdentity {
            engine_id: format!("portfolio.{engine}"),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            model_digest,
            configuration_digest,
            toolchain,
        },
        cases,
    };
    evaluate(
        corpus_root,
        manifest,
        &predictions,
        &MetricConfig::default(),
    )
    .map_err(|error| error.to_string())
}

fn document_model_digest(document: &Document) -> Option<Sha256Digest> {
    document
        .pages
        .iter()
        .flat_map(|page| &page.regions)
        .flat_map(|region| &region.evidence)
        .find_map(|evidence| evidence.provenance.model_digest)
}

fn prediction_from_ir(document: &Document) -> PredictionDocument {
    let mut regions = Vec::new();
    let mut reading_order = Vec::new();
    let mut formulas = Vec::new();
    for page in &document.pages {
        for region in &page.regions {
            let id = region.id.to_string();
            let text = selected_text(region);
            if let Some(latex) = selected_formula(region) {
                formulas.push(FormulaTruth {
                    region_id: id.clone(),
                    latex,
                });
            }
            regions.push(PredictedRegion {
                id,
                kind: block_kind(region.kind),
                text,
                rect: ferrodoc_foundry::TruthRect {
                    x: region.geometry.rect.x(),
                    y: region.geometry.rect.y(),
                    width: region.geometry.rect.width(),
                    height: region.geometry.rect.height(),
                },
            });
        }
        reading_order.extend(
            page.reading_order
                .iter()
                .map(|edge| [edge.before.to_string(), edge.after.to_string()]),
        );
    }
    PredictionDocument {
        text: regions
            .iter()
            .map(|region| region.text.as_str())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        regions,
        reading_order,
        tables: Vec::new(),
        formulas,
    }
}

fn selected_text(region: &Region) -> String {
    let selected = region.selected.as_ref();
    region
        .evidence
        .iter()
        .filter(|evidence| {
            selected.is_none_or(|selected| selected.evidence_ids.contains(&evidence.id))
        })
        .find_map(|evidence| match &evidence.content {
            EvidenceContent::Text { text } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn selected_formula(region: &Region) -> Option<String> {
    region
        .evidence
        .iter()
        .find_map(|evidence| match &evidence.content {
            EvidenceContent::Formula { latex } => Some(latex.clone()),
            _ => None,
        })
}

fn block_kind(kind: RegionKind) -> BlockKind {
    match kind {
        RegionKind::Heading | RegionKind::Header => BlockKind::Heading,
        RegionKind::Table | RegionKind::TableCell => BlockKind::Table,
        RegionKind::Formula => BlockKind::Formula,
        _ => BlockKind::Paragraph,
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {path:?}: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {path:?}: {error}"))
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("create {parent:?}: {error}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("create temporary report in {parent:?}: {error}"))?;
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.as_file_mut().sync_all())
        .map_err(|error| format!("write temporary report: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("publish report {path:?}: {error}"))?;
    Ok(())
}
