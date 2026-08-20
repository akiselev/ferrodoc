use std::{collections::BTreeMap, fs, path::Path};

use ferrodoc_bench::{BenchmarkReport, ComparisonPolicy};
use ferrodoc_core::Sha256Digest;
use ferrodoc_research::{
    CommandProvenance, ExperimentBudget, ExperimentSpec, ProtectedArtifact, SPEC_VERSION, TrialSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 3 {
        return Err(
            "usage: build_experiment_fixture <repo-root> <router-model> <output-spec>".into(),
        );
    }
    let root = Path::new(&arguments[0]);
    let model = Path::new(&arguments[1]);
    let output = Path::new(&arguments[2]);
    let native_path = root.join("benchmarks/routing/reports/native.json");
    let tesseract_path = root.join("benchmarks/routing/reports/tesseract.json");
    let native: BenchmarkReport = serde_json::from_slice(&fs::read(&native_path)?)?;
    let tesseract: BenchmarkReport = serde_json::from_slice(&fs::read(&tesseract_path)?)?;
    native.validate()?;
    tesseract.validate()?;
    if native.corpus_digest != tesseract.corpus_digest {
        return Err("experiment reports use different corpora".into());
    }
    let evaluator_path = root.join("tools/ferrodoc-bench/src/lib.rs");
    let truth_path = root.join("benchmarks/real-regression/manifest.json");
    let evaluator_digest = Sha256Digest::of_file(&evaluator_path)?;
    let command = |argv: Vec<String>| CommandProvenance {
        argv,
        environment: BTreeMap::from([
            ("RUST_TOOLCHAIN".into(), "1.95.0".into()),
            ("NETWORK_POLICY".into(), "offline".into()),
        ]),
        working_directory_digest: Sha256Digest::of_file(root.join("Cargo.lock"))
            .expect("workspace lockfile"),
    };
    let spec = ExperimentSpec {
        spec_version: SPEC_VERSION.into(),
        experiment_id: "phase-7-fixed-routing-comparison".into(),
        code_digest: Sha256Digest::of_file(root.join("Cargo.lock"))?,
        model_digest: Some(Sha256Digest::of_file(model)?),
        corpus_digest: native.corpus_digest,
        evaluator_digest,
        comparison_policy: ComparisonPolicy::default(),
        budget: ExperimentBudget {
            maximum_evaluations: 2,
            maximum_runner_wall_ms: 30_000,
        },
        protected: vec![
            ProtectedArtifact {
                path: "benchmarks/real-regression/manifest.json".into(),
                digest: Sha256Digest::of_file(&truth_path)?,
                role: "benchmark_truth".into(),
            },
            ProtectedArtifact {
                path: "tools/ferrodoc-bench/src/lib.rs".into(),
                digest: evaluator_digest,
                role: "metric_code".into(),
            },
        ],
        trials: vec![
            TrialSpec::Mutation {
                id: "router-stump-training".into(),
                command: command(vec![
                    "ferrodoc".into(),
                    "router".into(),
                    "train".into(),
                    ".".into(),
                    "benchmarks/routing/dataset.json".into(),
                    "router-model.json".into(),
                ]),
                output_digest: Some(Sha256Digest::of_file(model)?),
            },
            TrialSpec::Evaluation {
                id: "native-baseline".into(),
                command: command(vec![
                    "ferrodoc-bench".into(),
                    "verify-report".into(),
                    "benchmarks/routing/reports/native.json".into(),
                ]),
                raw_report: "benchmarks/routing/reports/native.json".into(),
                raw_report_digest: Sha256Digest::of_file(&native_path)?,
            },
            TrialSpec::Evaluation {
                id: "tesseract-route".into(),
                command: command(vec![
                    "ferrodoc-bench".into(),
                    "verify-report".into(),
                    "benchmarks/routing/reports/tesseract.json".into(),
                ]),
                raw_report: "benchmarks/routing/reports/tesseract.json".into(),
                raw_report_digest: Sha256Digest::of_file(&tesseract_path)?,
            },
        ],
    };
    spec.validate()?;
    let mut bytes = serde_json::to_vec_pretty(&spec)?;
    bytes.push(b'\n');
    fs::write(output, bytes)?;
    Ok(())
}
