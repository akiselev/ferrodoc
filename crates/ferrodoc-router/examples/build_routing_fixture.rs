use std::{fs, path::Path, process::Command};

use ferrodoc_bench::{BenchmarkReport, EvaluatedOutcome, EvidenceValue};
use ferrodoc_core::Sha256Digest;
use ferrodoc_foundry::{CorpusManifest, Partition};
use ferrodoc_router::{
    ArtifactSource, DATASET_VERSION, FEATURE_SCHEMA_VERSION, FeatureValue, RouteClass,
    RouteOutcome, RoutingDataset, RoutingFeatures, RoutingRecord,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 5 {
        return Err("usage: build_routing_fixture <repo-root> <ferrodoc> <native-report> <ocr-report> <output-dir>".into());
    }
    let root = Path::new(&arguments[0]);
    let executable = Path::new(&arguments[1]);
    let native_report_path = Path::new(&arguments[2]);
    let ocr_report_path = Path::new(&arguments[3]);
    let output = Path::new(&arguments[4]);
    let manifest: CorpusManifest = serde_json::from_slice(&fs::read(
        root.join("benchmarks/real-regression/manifest.json"),
    )?)?;
    let native_report: BenchmarkReport = serde_json::from_slice(&fs::read(native_report_path)?)?;
    let ocr_report: BenchmarkReport = serde_json::from_slice(&fs::read(ocr_report_path)?)?;
    native_report.validate()?;
    ocr_report.validate()?;
    if native_report.corpus_digest != manifest.corpus_digest
        || ocr_report.corpus_digest != manifest.corpus_digest
    {
        return Err("reports do not match the real regression corpus".into());
    }
    fs::create_dir_all(output.join("reports"))?;
    fs::create_dir_all(output.join("traces"))?;
    let native_bytes = serde_json::to_vec_pretty(&native_report)?;
    let ocr_bytes = serde_json::to_vec_pretty(&ocr_report)?;
    fs::write(output.join("reports/native.json"), &native_bytes)?;
    fs::write(output.join("reports/tesseract.json"), &ocr_bytes)?;
    let output_relative = output.strip_prefix(root)?;
    let native_source = ArtifactSource {
        path: output_relative
            .join("reports/native.json")
            .to_string_lossy()
            .into(),
        digest: Sha256Digest::of_bytes(&native_bytes),
    };
    let ocr_source = ArtifactSource {
        path: output_relative
            .join("reports/tesseract.json")
            .to_string_lossy()
            .into(),
        digest: Sha256Digest::of_bytes(&ocr_bytes),
    };
    let mut records = Vec::new();
    for (index, case) in manifest.cases.iter().enumerate() {
        let trace_output = Command::new(executable)
            .current_dir(root)
            .args(["explain", &case.document, "--ocr-engine", "tesseract"])
            .output()?;
        if !trace_output.status.success() {
            return Err(format!(
                "trace conversion failed for {}: {}",
                case.id,
                String::from_utf8_lossy(&trace_output.stderr)
            )
            .into());
        }
        let trace: serde_json::Value = serde_json::from_slice(&trace_output.stdout)?;
        let trace_bytes = serde_json::to_vec_pretty(&trace)?;
        let trace_name = format!("traces/{}.json", case.id);
        fs::write(output.join(&trace_name), &trace_bytes)?;
        let (page_count, native_characters) = trace_features(&trace)?;
        records.push(RoutingRecord {
            case_id: case.id.clone(),
            document_digest: case.document_digest,
            family_id: case.case_identity.to_string(),
            partition: if index == 0 {
                Partition::Train
            } else {
                Partition::HeldOut
            },
            conversion_trace: ArtifactSource {
                path: output_relative.join(&trace_name).to_string_lossy().into(),
                digest: Sha256Digest::of_bytes(&trace_bytes),
            },
            features: RoutingFeatures {
                page_count: FeatureValue::Observed { value: page_count },
                native_characters: FeatureValue::Observed {
                    value: native_characters,
                },
                image_coverage: FeatureValue::Missing {
                    reason: "the current conversion trace does not observe image coverage".into(),
                },
                scanned_likelihood: FeatureValue::Observed {
                    value: if native_characters == 0 { 1.0 } else { 0.0 },
                },
            },
            outcomes: vec![
                outcome(&native_report, &case.id, RouteClass::Native, &native_source)?,
                outcome(&ocr_report, &case.id, RouteClass::Ocr, &ocr_source)?,
            ],
        });
    }
    let dataset = RoutingDataset {
        dataset_version: DATASET_VERSION.into(),
        feature_schema_version: FEATURE_SCHEMA_VERSION.into(),
        corpus_digest: manifest.corpus_digest,
        records,
    };
    dataset.verify_sources(root)?;
    let mut dataset_bytes = serde_json::to_vec_pretty(&dataset)?;
    dataset_bytes.push(b'\n');
    fs::write(output.join("dataset.json"), dataset_bytes)?;
    Ok(())
}

fn trace_features(trace: &serde_json::Value) -> Result<(u32, u64), Box<dyn std::error::Error>> {
    let events = trace["events"]
        .as_array()
        .ok_or("trace events are absent")?;
    let mut pages = None;
    let mut characters = 0_u64;
    for event in events {
        let code = event["code"].as_str().unwrap_or_default();
        let detail = event["detail"].as_str().unwrap_or_default();
        if code == "document.inspected" {
            pages = detail
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok());
        } else if code == "native.quality" {
            characters = characters.saturating_add(
                detail
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("native character count is absent")?,
            );
        }
    }
    Ok((pages.ok_or("page count is absent")?, characters))
}

fn outcome(
    report: &BenchmarkReport,
    case_id: &str,
    route: RouteClass,
    source: &ArtifactSource,
) -> Result<RouteOutcome, Box<dyn std::error::Error>> {
    let case = report
        .cases
        .iter()
        .find(|case| case.case_id == case_id)
        .ok_or("case absent from report")?;
    let (quality, failed) = match &case.outcome {
        EvaluatedOutcome::Success { metrics } => (metrics.quality.get(), false),
        EvaluatedOutcome::Failure { .. } | EvaluatedOutcome::Skipped { .. } => (0.0, true),
    };
    let cold_wall_ms = case
        .timing
        .as_ref()
        .and_then(|timing| match &timing.cold_wall_ms {
            EvidenceValue::Measured { value, .. } => Some(*value),
            EvidenceValue::Estimated { .. } | EvidenceValue::Unknown { .. } => None,
        });
    Ok(RouteOutcome {
        engine_id: report.candidate.engine_id.clone(),
        route,
        benchmark_report: source.clone(),
        case_id: case_id.into(),
        quality,
        failed,
        cold_wall_ms,
    })
}
