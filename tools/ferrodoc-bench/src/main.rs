use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
};

use ferrodoc_bench::{
    BenchmarkReport, ComparisonPolicy, MetricConfig, PredictionSet, compare, evaluate,
};
use ferrodoc_foundry::CorpusManifest;
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
        _ => Err(
            "usage: ferrodoc-bench evaluate <corpus-root> <manifest.json> <predictions.json> <report.json> | compare <baseline.json> <candidate.json> <comparison.json> | verify-report <report.json>"
                .into(),
        ),
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
