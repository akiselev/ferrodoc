//! Immutable, resumable experiment ledger over already-produced benchmark reports.
//!
//! The runner deliberately does not execute mutation or evaluation commands. It
//! records exact commands as provenance, validates immutable reports internally,
//! and re-hashes protected truth/evaluator artifacts before and after evaluation.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    time::Instant,
};

use ferrodoc_bench::{BenchmarkReport, ComparisonPolicy, Dominance, compare};
use ferrodoc_core::Sha256Digest;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SPEC_VERSION: &str = "ferrodoc-experiment-spec/1";
pub const LEDGER_VERSION: &str = "ferrodoc-experiment-ledger/1";

/// File that must remain byte-identical throughout an evaluation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProtectedArtifact {
    pub path: String,
    pub digest: Sha256Digest,
    pub role: String,
}

/// Explicit finite admission budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExperimentBudget {
    pub maximum_evaluations: u32,
    pub maximum_runner_wall_ms: u64,
}

/// Exact command and selected environment facts recorded by the producer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommandProvenance {
    pub argv: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub working_directory_digest: Sha256Digest,
}

/// Mutations are recorded but never executed by the evaluation runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrialSpec {
    Mutation {
        id: String,
        command: CommandProvenance,
        output_digest: Option<Sha256Digest>,
    },
    Evaluation {
        id: String,
        command: CommandProvenance,
        raw_report: String,
        raw_report_digest: Sha256Digest,
    },
}

impl TrialSpec {
    fn id(&self) -> &str {
        match self {
            Self::Mutation { id, .. } | Self::Evaluation { id, .. } => id,
        }
    }
}

/// Immutable experiment definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExperimentSpec {
    pub spec_version: String,
    pub experiment_id: String,
    pub code_digest: Sha256Digest,
    pub model_digest: Option<Sha256Digest>,
    pub corpus_digest: Sha256Digest,
    pub evaluator_digest: Sha256Digest,
    pub comparison_policy: ComparisonPolicy,
    pub budget: ExperimentBudget,
    pub protected: Vec<ProtectedArtifact>,
    pub trials: Vec<TrialSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TrialStatus {
    Pending,
    MutationRecorded {
        output_digest: Option<Sha256Digest>,
    },
    EvaluationComplete {
        engine_id: String,
        report_digest: Sha256Digest,
    },
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TrialRecord {
    pub id: String,
    pub status: TrialStatus,
}

/// Resumable state. Raw reports stay external and digest-bound.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExperimentLedger {
    pub ledger_version: String,
    pub experiment_id: String,
    pub spec_digest: Sha256Digest,
    pub code_digest: Sha256Digest,
    pub model_digest: Option<Sha256Digest>,
    pub corpus_digest: Sha256Digest,
    pub evaluator_digest: Sha256Digest,
    pub trials: Vec<TrialRecord>,
    /// Trial IDs on the retained Pareto frontier. Tradeoffs and unknowns remain.
    pub pareto_frontier: Vec<String>,
    pub completed_evaluations: u32,
    pub pending_evaluations: u32,
    pub runner_wall_ms: u64,
}

#[derive(Debug, Error)]
pub enum ResearchError {
    #[error("experiment integrity error: {0}")]
    Integrity(String),
    #[error("experiment file operation at {path:?}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("experiment JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("experiment digest error: {0}")]
    Core(#[from] ferrodoc_core::CoreError),
}

impl ExperimentSpec {
    pub fn validate(&self) -> Result<(), ResearchError> {
        if self.spec_version != SPEC_VERSION || self.experiment_id.is_empty() {
            return Err(ResearchError::Integrity(
                "unsupported spec version or empty experiment ID".into(),
            ));
        }
        if self.budget.maximum_evaluations == 0 || self.budget.maximum_runner_wall_ms == 0 {
            return Err(ResearchError::Integrity(
                "experiment budget must be positive".into(),
            ));
        }
        if self.protected.is_empty() {
            return Err(ResearchError::Integrity(
                "truth and evaluator protection list cannot be empty".into(),
            ));
        }
        let mut protected_paths = BTreeSet::new();
        for artifact in &self.protected {
            validate_relative(&artifact.path)?;
            if artifact.role.is_empty() || !protected_paths.insert(&artifact.path) {
                return Err(ResearchError::Integrity(
                    "protected artifacts need unique paths and nonempty roles".into(),
                ));
            }
        }
        let has_truth = self
            .protected
            .iter()
            .any(|item| item.role == "benchmark_truth");
        let has_evaluator = self
            .protected
            .iter()
            .any(|item| item.role == "metric_code" || item.role == "evaluator_binary");
        if !has_truth || !has_evaluator {
            return Err(ResearchError::Integrity(
                "protected artifacts must include benchmark_truth and metric_code/evaluator_binary"
                    .into(),
            ));
        }
        let mut ids = BTreeSet::new();
        for trial in &self.trials {
            if trial.id().is_empty() || !ids.insert(trial.id()) {
                return Err(ResearchError::Integrity(
                    "trial IDs must be nonempty and unique".into(),
                ));
            }
            let command = match trial {
                TrialSpec::Mutation { command, .. } | TrialSpec::Evaluation { command, .. } => {
                    command
                }
            };
            if command.argv.is_empty() || command.argv.iter().any(|argument| argument.is_empty()) {
                return Err(ResearchError::Integrity(
                    "exact command argv must be nonempty".into(),
                ));
            }
            if let TrialSpec::Evaluation { raw_report, .. } = trial {
                validate_relative(raw_report)?;
                if protected_paths.contains(raw_report) {
                    return Err(ResearchError::Integrity(
                        "evaluation output cannot alias protected truth or metric code".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Evaluate pending reports within budget and atomically persist resumable state.
pub fn run(
    root: &Path,
    spec_path: &Path,
    ledger_path: &Path,
) -> Result<ExperimentLedger, ResearchError> {
    let started = Instant::now();
    let spec_bytes = read(spec_path)?;
    let spec_digest = Sha256Digest::of_bytes(&spec_bytes);
    let spec: ExperimentSpec = serde_json::from_slice(&spec_bytes)?;
    spec.validate()?;
    verify_protected(root, &spec)?;
    let mut ledger = if ledger_path.exists() {
        let ledger: ExperimentLedger = serde_json::from_slice(&read(ledger_path)?)?;
        if ledger.ledger_version != LEDGER_VERSION
            || ledger.experiment_id != spec.experiment_id
            || ledger.spec_digest != spec_digest
            || ledger.code_digest != spec.code_digest
            || ledger.model_digest != spec.model_digest
            || ledger.corpus_digest != spec.corpus_digest
            || ledger.evaluator_digest != spec.evaluator_digest
        {
            return Err(ResearchError::Integrity(
                "existing ledger does not match immutable experiment identities".into(),
            ));
        }
        ledger
    } else {
        ExperimentLedger {
            ledger_version: LEDGER_VERSION.into(),
            experiment_id: spec.experiment_id.clone(),
            spec_digest,
            code_digest: spec.code_digest,
            model_digest: spec.model_digest,
            corpus_digest: spec.corpus_digest,
            evaluator_digest: spec.evaluator_digest,
            trials: spec
                .trials
                .iter()
                .map(|trial| TrialRecord {
                    id: trial.id().into(),
                    status: TrialStatus::Pending,
                })
                .collect(),
            pareto_frontier: Vec::new(),
            completed_evaluations: 0,
            pending_evaluations: 0,
            runner_wall_ms: 0,
        }
    };
    let prior_runner_wall_ms = ledger.runner_wall_ms;
    let mut admitted = ledger
        .trials
        .iter()
        .filter(|record| {
            matches!(
                record.status,
                TrialStatus::EvaluationComplete { .. } | TrialStatus::Failed { .. }
            )
        })
        .count() as u32;
    for trial in &spec.trials {
        let record = ledger
            .trials
            .iter_mut()
            .find(|record| record.id == trial.id())
            .ok_or_else(|| ResearchError::Integrity("ledger trial set changed".into()))?;
        if record.status != TrialStatus::Pending {
            continue;
        }
        match trial {
            TrialSpec::Mutation { output_digest, .. } => {
                record.status = TrialStatus::MutationRecorded {
                    output_digest: *output_digest,
                };
            }
            TrialSpec::Evaluation {
                raw_report,
                raw_report_digest,
                ..
            } => {
                if admitted >= spec.budget.maximum_evaluations
                    || u128::from(prior_runner_wall_ms) + started.elapsed().as_millis()
                        >= u128::from(spec.budget.maximum_runner_wall_ms)
                {
                    continue;
                }
                admitted += 1;
                match load_report(root, raw_report, *raw_report_digest, spec.corpus_digest) {
                    Ok(report) => {
                        record.status = TrialStatus::EvaluationComplete {
                            engine_id: report.candidate.engine_id,
                            report_digest: *raw_report_digest,
                        };
                    }
                    Err(error) => {
                        record.status = TrialStatus::Failed {
                            reason: error.to_string(),
                        };
                    }
                }
            }
        }
    }
    verify_protected(root, &spec)?;
    ledger.pareto_frontier = pareto_frontier(root, &spec, &ledger)?;
    ledger.completed_evaluations = ledger
        .trials
        .iter()
        .filter(|trial| matches!(trial.status, TrialStatus::EvaluationComplete { .. }))
        .count() as u32;
    ledger.pending_evaluations = ledger
        .trials
        .iter()
        .filter(|trial| trial.status == TrialStatus::Pending)
        .count() as u32;
    ledger.runner_wall_ms = prior_runner_wall_ms
        .saturating_add(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    write_atomic(ledger_path, &serde_json::to_vec_pretty(&ledger)?)?;
    Ok(ledger)
}

fn load_report(
    root: &Path,
    relative: &str,
    expected: Sha256Digest,
    corpus: Sha256Digest,
) -> Result<BenchmarkReport, ResearchError> {
    let path = root.join(relative);
    let bytes = read(&path)?;
    if Sha256Digest::of_bytes(&bytes) != expected {
        return Err(ResearchError::Integrity(format!(
            "raw report digest changed for {relative:?}"
        )));
    }
    let report: BenchmarkReport = serde_json::from_slice(&bytes)?;
    report
        .validate()
        .map_err(|error| ResearchError::Integrity(error.to_string()))?;
    if report.corpus_digest != corpus {
        return Err(ResearchError::Integrity(
            "raw report corpus differs from experiment corpus".into(),
        ));
    }
    Ok(report)
}

fn pareto_frontier(
    root: &Path,
    spec: &ExperimentSpec,
    ledger: &ExperimentLedger,
) -> Result<Vec<String>, ResearchError> {
    let complete: Vec<_> = spec
        .trials
        .iter()
        .filter_map(|trial| match trial {
            TrialSpec::Evaluation {
                id,
                raw_report,
                raw_report_digest,
                ..
            } if ledger.trials.iter().any(|record| {
                record.id == *id && matches!(record.status, TrialStatus::EvaluationComplete { .. })
            }) =>
            {
                Some((id, raw_report, raw_report_digest))
            }
            _ => None,
        })
        .map(|(id, path, digest)| {
            load_report(root, path, *digest, spec.corpus_digest).map(|report| (id, report))
        })
        .collect::<Result<_, _>>()?;
    let mut retained = Vec::new();
    'candidate: for (candidate_id, candidate) in &complete {
        for (other_id, other) in &complete {
            if candidate_id == other_id {
                continue;
            }
            let comparison = compare(candidate, other, spec.comparison_policy.clone())
                .map_err(|error| ResearchError::Integrity(error.to_string()))?;
            if comparison.dominance == Dominance::CandidateDominates {
                continue 'candidate;
            }
        }
        retained.push((*candidate_id).clone());
    }
    retained.sort();
    Ok(retained)
}

fn verify_protected(root: &Path, spec: &ExperimentSpec) -> Result<(), ResearchError> {
    for artifact in &spec.protected {
        let path = root.join(&artifact.path);
        let digest = Sha256Digest::of_file(&path)?;
        if digest != artifact.digest {
            return Err(ResearchError::Integrity(format!(
                "protected {} artifact {:?} changed",
                artifact.role, artifact.path
            )));
        }
    }
    Ok(())
}

fn validate_relative(path: &str) -> Result<(), ResearchError> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ResearchError::Integrity(
            "experiment artifact path must be normalized and relative".into(),
        ));
    }
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, ResearchError> {
    fs::read(path).map_err(|source| ResearchError::Io {
        path: path.to_owned(),
        source,
    })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ResearchError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| ResearchError::Io {
        path: parent.to_owned(),
        source,
    })?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ResearchError::Integrity("ledger path needs a file name".into()))?;
    let temporary = parent.join(format!(".{name}.tmp-{}", std::process::id()));
    fs::write(&temporary, bytes).map_err(|source| ResearchError::Io {
        path: temporary.clone(),
        source,
    })?;
    fs::rename(&temporary, path).map_err(|source| ResearchError::Io {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(truth: Sha256Digest, evaluator: Sha256Digest) -> ExperimentSpec {
        ExperimentSpec {
            spec_version: SPEC_VERSION.into(),
            experiment_id: "experiment".into(),
            code_digest: Sha256Digest::of_bytes(b"code"),
            model_digest: None,
            corpus_digest: Sha256Digest::of_bytes(b"corpus"),
            evaluator_digest: evaluator,
            comparison_policy: ComparisonPolicy::default(),
            budget: ExperimentBudget {
                maximum_evaluations: 1,
                maximum_runner_wall_ms: 10_000,
            },
            protected: vec![
                ProtectedArtifact {
                    path: "truth.json".into(),
                    digest: truth,
                    role: "benchmark_truth".into(),
                },
                ProtectedArtifact {
                    path: "metrics.bin".into(),
                    digest: evaluator,
                    role: "evaluator_binary".into(),
                },
            ],
            trials: vec![TrialSpec::Mutation {
                id: "mutation-1".into(),
                command: CommandProvenance {
                    argv: vec!["trainer".into(), "--fixed-seed".into()],
                    environment: BTreeMap::from([("RUSTC".into(), "1.95.0".into())]),
                    working_directory_digest: Sha256Digest::of_bytes(b"tree"),
                },
                output_digest: None,
            }],
        }
    }

    #[test]
    fn runner_records_mutation_without_executing_it_and_resumes() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("truth.json"), b"truth").unwrap();
        fs::write(directory.path().join("metrics.bin"), b"metrics").unwrap();
        let spec = spec(
            Sha256Digest::of_bytes(b"truth"),
            Sha256Digest::of_bytes(b"metrics"),
        );
        let spec_path = directory.path().join("spec.json");
        let ledger_path = directory.path().join("ledger.json");
        fs::write(&spec_path, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
        let first = run(directory.path(), &spec_path, &ledger_path).unwrap();
        let second = run(directory.path(), &spec_path, &ledger_path).unwrap();
        assert_eq!(first.trials, second.trials);
        assert!(matches!(
            first.trials[0].status,
            TrialStatus::MutationRecorded { .. }
        ));
    }

    #[test]
    fn protected_truth_change_blocks_resume() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("truth.json"), b"truth").unwrap();
        fs::write(directory.path().join("metrics.bin"), b"metrics").unwrap();
        let spec = spec(
            Sha256Digest::of_bytes(b"truth"),
            Sha256Digest::of_bytes(b"metrics"),
        );
        let spec_path = directory.path().join("spec.json");
        fs::write(&spec_path, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
        fs::write(directory.path().join("truth.json"), b"changed").unwrap();
        assert!(
            run(
                directory.path(),
                &spec_path,
                &directory.path().join("ledger.json")
            )
            .is_err()
        );
    }

    #[test]
    fn evaluator_and_truth_are_mandatory_and_cannot_alias_outputs() {
        let mut spec = spec(
            Sha256Digest::of_bytes(b"truth"),
            Sha256Digest::of_bytes(b"metrics"),
        );
        spec.protected.pop();
        assert!(spec.validate().is_err());
        spec.protected.push(ProtectedArtifact {
            path: "metrics.bin".into(),
            digest: Sha256Digest::of_bytes(b"metrics"),
            role: "metric_code".into(),
        });
        spec.trials.push(TrialSpec::Evaluation {
            id: "evaluation".into(),
            command: CommandProvenance {
                argv: vec!["evaluator".into()],
                environment: BTreeMap::new(),
                working_directory_digest: Sha256Digest::of_bytes(b"tree"),
            },
            raw_report: "truth.json".into(),
            raw_report_digest: Sha256Digest::of_bytes(b"report"),
        });
        assert!(spec.validate().is_err());
    }

    #[test]
    fn checked_in_schema_snapshots_match_contracts() {
        let spec: serde_json::Value =
            serde_json::from_str(include_str!("../../../schemas/experiment-spec-v1.json")).unwrap();
        let ledger: serde_json::Value =
            serde_json::from_str(include_str!("../../../schemas/experiment-ledger-v1.json"))
                .unwrap();
        assert_eq!(
            spec,
            serde_json::to_value(schemars::schema_for!(ExperimentSpec)).unwrap()
        );
        assert_eq!(
            ledger,
            serde_json::to_value(schemars::schema_for!(ExperimentLedger)).unwrap()
        );
    }
}
