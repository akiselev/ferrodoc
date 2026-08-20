//! Leakage-resistant routing records, deterministic baselines, and a guarded stump model.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
};

use ferrodoc_bench::{BenchmarkReport, EvaluatedOutcome, EvidenceValue};
use ferrodoc_core::{Profile, Sha256Digest};
use ferrodoc_foundry::Partition;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Routing dataset contract.
pub const DATASET_VERSION: &str = "ferrodoc-routing-dataset/1";
/// Features available before engine execution. Held-out truth is deliberately absent.
pub const FEATURE_SCHEMA_VERSION: &str = "ferrodoc-routing-features/1";
/// Learned model contract.
pub const MODEL_VERSION: &str = "ferrodoc-router-stump/1";

/// An observed feature or a visible reason it is unavailable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FeatureValue<T> {
    /// Value observed before routing.
    Observed { value: T },
    /// Value was unavailable; it must not be silently imputed.
    Missing { reason: String },
}

impl<T> FeatureValue<T> {
    fn observed(&self) -> Option<&T> {
        match self {
            Self::Observed { value } => Some(value),
            Self::Missing { .. } => None,
        }
    }
}

/// Stable pre-execution features. No truth or post-execution metric is admitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoutingFeatures {
    pub page_count: FeatureValue<u32>,
    pub native_characters: FeatureValue<u64>,
    pub image_coverage: FeatureValue<f64>,
    pub scanned_likelihood: FeatureValue<f64>,
}

/// Coarse semantic route selected before a concrete engine.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum RouteClass {
    Native,
    Ocr,
}

/// Digest-bound file from which one routing observation was derived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactSource {
    /// Repository- or run-root-relative path.
    pub path: String,
    /// Exact bytes consumed.
    pub digest: Sha256Digest,
}

/// Historical candidate result copied from a benchmark case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteOutcome {
    pub engine_id: String,
    pub route: RouteClass,
    pub benchmark_report: ArtifactSource,
    pub case_id: String,
    pub quality: f64,
    pub failed: bool,
    pub cold_wall_ms: Option<f64>,
}

/// One trace-derived training/evaluation example.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoutingRecord {
    pub case_id: String,
    pub document_digest: Sha256Digest,
    /// Related synthetic degradations share this identity and must share a partition.
    pub family_id: String,
    pub partition: Partition,
    pub conversion_trace: ArtifactSource,
    pub features: RoutingFeatures,
    pub outcomes: Vec<RouteOutcome>,
}

/// Complete traceable routing dataset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoutingDataset {
    pub dataset_version: String,
    pub feature_schema_version: String,
    pub corpus_digest: Sha256Digest,
    pub records: Vec<RoutingRecord>,
}

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("routing integrity error: {0}")]
    Integrity(String),
    #[error("routing file operation at {path:?}: {source}")]
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("routing JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("routing digest error: {0}")]
    Core(#[from] ferrodoc_core::CoreError),
}

impl RoutingDataset {
    /// Validates schema, partitions, examples, and leakage barriers.
    pub fn validate(&self) -> Result<(), RouterError> {
        if self.dataset_version != DATASET_VERSION
            || self.feature_schema_version != FEATURE_SCHEMA_VERSION
        {
            return Err(RouterError::Integrity(
                "unsupported dataset or feature schema version".into(),
            ));
        }
        if self.records.is_empty() {
            return Err(RouterError::Integrity("routing dataset is empty".into()));
        }
        let mut cases = BTreeSet::new();
        let mut families = BTreeMap::new();
        for record in &self.records {
            if record.case_id.is_empty() || record.family_id.is_empty() {
                return Err(RouterError::Integrity(
                    "empty case or family identity".into(),
                ));
            }
            if !cases.insert(record.case_id.clone()) {
                return Err(RouterError::Integrity(format!(
                    "duplicate routing case {:?}",
                    record.case_id
                )));
            }
            if let Some(previous) = families.insert(record.family_id.clone(), record.partition)
                && previous != record.partition
            {
                return Err(RouterError::Integrity(format!(
                    "family {:?} crosses corpus partitions",
                    record.family_id
                )));
            }
            validate_source_path(&record.conversion_trace.path)?;
            if record.outcomes.is_empty() {
                return Err(RouterError::Integrity(format!(
                    "case {:?} has no observed candidate outcomes",
                    record.case_id
                )));
            }
            let mut engines = BTreeSet::new();
            for outcome in &record.outcomes {
                validate_source_path(&outcome.benchmark_report.path)?;
                if outcome.case_id != record.case_id || outcome.engine_id.is_empty() {
                    return Err(RouterError::Integrity(
                        "benchmark outcome identity differs from routing record".into(),
                    ));
                }
                if !outcome.quality.is_finite() || !(0.0..=1.0).contains(&outcome.quality) {
                    return Err(RouterError::Integrity("invalid outcome quality".into()));
                }
                if !engines.insert(outcome.engine_id.clone()) {
                    return Err(RouterError::Integrity(format!(
                        "duplicate engine outcome in case {:?}",
                        record.case_id
                    )));
                }
            }
            validate_features(&record.features)?;
        }
        Ok(())
    }

    /// Re-hashes every source and proves copied outcomes match benchmark case records.
    pub fn verify_sources(&self, root: &Path) -> Result<(), RouterError> {
        self.validate()?;
        for record in &self.records {
            verify_digest(root, &record.conversion_trace)?;
            for outcome in &record.outcomes {
                let path = verify_digest(root, &outcome.benchmark_report)?;
                let report: BenchmarkReport =
                    serde_json::from_slice(&fs::read(&path).map_err(|source| {
                        RouterError::Io {
                            path: path.clone(),
                            source,
                        }
                    })?)?;
                report
                    .validate()
                    .map_err(|error| RouterError::Integrity(error.to_string()))?;
                if report.corpus_digest != self.corpus_digest
                    || report.candidate.engine_id != outcome.engine_id
                {
                    return Err(RouterError::Integrity(
                        "routing outcome report or corpus identity mismatch".into(),
                    ));
                }
                let case = report
                    .cases
                    .iter()
                    .find(|case| case.case_id == outcome.case_id)
                    .ok_or_else(|| {
                        RouterError::Integrity("routing case absent from benchmark report".into())
                    })?;
                let (quality, failed) = match &case.outcome {
                    EvaluatedOutcome::Success { metrics } => (metrics.quality.get(), false),
                    EvaluatedOutcome::Failure { .. } | EvaluatedOutcome::Skipped { .. } => {
                        (0.0, true)
                    }
                };
                if quality != outcome.quality || failed != outcome.failed {
                    return Err(RouterError::Integrity(
                        "copied routing outcome differs from benchmark case".into(),
                    ));
                }
                let cold_wall_ms =
                    case.timing
                        .as_ref()
                        .and_then(|timing| match &timing.cold_wall_ms {
                            EvidenceValue::Measured { value, .. } => Some(*value),
                            EvidenceValue::Estimated { .. } | EvidenceValue::Unknown { .. } => None,
                        });
                if cold_wall_ms != outcome.cold_wall_ms {
                    return Err(RouterError::Integrity(
                        "copied routing latency differs from measured benchmark case".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn validate_features(features: &RoutingFeatures) -> Result<(), RouterError> {
    for value in [
        features.image_coverage.observed(),
        features.scanned_likelihood.observed(),
    ]
    .into_iter()
    .flatten()
    {
        if !value.is_finite() || !(0.0..=1.0).contains(value) {
            return Err(RouterError::Integrity(
                "routing probability feature is outside zero through one".into(),
            ));
        }
    }
    Ok(())
}

fn validate_source_path(path: &str) -> Result<(), RouterError> {
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
        return Err(RouterError::Integrity(
            "routing source path must be normalized and relative".into(),
        ));
    }
    Ok(())
}

fn verify_digest(root: &Path, source: &ArtifactSource) -> Result<std::path::PathBuf, RouterError> {
    let path = root.join(&source.path);
    let digest = Sha256Digest::of_file(&path)?;
    if digest != source.digest {
        return Err(RouterError::Integrity(format!(
            "source digest changed for {:?}",
            source.path
        )));
    }
    Ok(path)
}

/// Deterministic non-learned policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BaselinePolicy {
    AlwaysNative,
    NativeThreshold { minimum_characters: u64 },
    PageTypeRules { minimum_characters: u64 },
    ProfileSpecific { profile: Profile },
}

impl BaselinePolicy {
    pub fn route(&self, features: &RoutingFeatures) -> RouteClass {
        let characters = features.native_characters.observed().copied();
        match self {
            Self::AlwaysNative => RouteClass::Native,
            Self::NativeThreshold { minimum_characters } => characters
                .filter(|value| value >= minimum_characters)
                .map_or(RouteClass::Ocr, |_| RouteClass::Native),
            Self::PageTypeRules { minimum_characters } => {
                let looks_scanned = features
                    .scanned_likelihood
                    .observed()
                    .is_some_and(|value| *value >= 0.5);
                if !looks_scanned && characters.is_some_and(|value| value >= *minimum_characters) {
                    RouteClass::Native
                } else {
                    RouteClass::Ocr
                }
            }
            Self::ProfileSpecific { profile } => match profile {
                Profile::Accurate => features
                    .image_coverage
                    .observed()
                    .filter(|value| **value >= 0.5)
                    .map_or(RouteClass::Native, |_| RouteClass::Ocr),
                Profile::Fast | Profile::Cheap | Profile::Cpu | Profile::LowVram => characters
                    .filter(|value| *value > 0)
                    .map_or(RouteClass::Ocr, |_| RouteClass::Native),
                Profile::Balanced | Profile::Offline | Profile::Private => characters
                    .filter(|value| *value >= 32)
                    .map_or(RouteClass::Ocr, |_| RouteClass::Native),
            },
        }
    }
}

/// Objective declared before training. Scalarization is only an admission check;
/// separate quality/latency/failure values remain in every evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrainingObjective {
    pub latency_penalty_per_second: f64,
    pub failure_penalty: f64,
    pub minimum_improvement: f64,
    pub minimum_confidence: f64,
}

impl Default for TrainingObjective {
    fn default() -> Self {
        Self {
            latency_penalty_per_second: 0.001,
            failure_penalty: 1.0,
            minimum_improvement: 0.001,
            minimum_confidence: 0.6,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouterEvaluation {
    pub partition: Partition,
    pub case_ids: Vec<String>,
    pub mean_quality: f64,
    pub mean_latency_ms: Option<f64>,
    pub failure_rate: f64,
    pub objective: f64,
}

/// Qualification state prevents an unproven model from being used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Qualification {
    Qualified,
    Rejected { reason: String },
}

/// Small, auditable learned router.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouterModel {
    pub model_version: String,
    pub feature_schema_version: String,
    pub dataset_digest: Sha256Digest,
    pub threshold: u64,
    pub native_if_at_least: bool,
    pub native_engine: String,
    pub ocr_engine: String,
    pub confidence: f64,
    pub minimum_confidence: f64,
    pub qualification: Qualification,
    pub training: RouterEvaluation,
    pub held_out: RouterEvaluation,
    pub baselines: Vec<(BaselinePolicy, RouterEvaluation)>,
}

/// Train a decision stump on training cases and qualify it only on held-out cases.
pub fn train_and_evaluate(
    dataset: &RoutingDataset,
    objective: TrainingObjective,
) -> Result<RouterModel, RouterError> {
    dataset.validate()?;
    validate_objective(&objective)?;
    let training: Vec<_> = dataset
        .records
        .iter()
        .filter(|record| record.partition == Partition::Train)
        .collect();
    let held_out: Vec<_> = dataset
        .records
        .iter()
        .filter(|record| record.partition == Partition::HeldOut)
        .collect();
    if training.is_empty() || held_out.is_empty() {
        return Err(RouterError::Integrity(
            "training requires nonempty train and held-out partitions".into(),
        ));
    }
    let native_engine = common_engine(&dataset.records, RouteClass::Native)?;
    let ocr_engine = common_engine(&dataset.records, RouteClass::Ocr)?;
    let mut thresholds: Vec<u64> = training
        .iter()
        .filter_map(|record| record.features.native_characters.observed().copied())
        .collect();
    thresholds.push(0);
    thresholds.sort_unstable();
    thresholds.dedup();
    let mut best = None;
    for threshold in thresholds {
        for native_if_at_least in [false, true] {
            let evaluation = evaluate_records(
                &training,
                Partition::Train,
                &objective,
                |features| stump_route(features, threshold, native_if_at_least),
                &native_engine,
                &ocr_engine,
            );
            if best
                .as_ref()
                .is_none_or(|(_, _, previous): &(u64, bool, RouterEvaluation)| {
                    evaluation.objective > previous.objective
                })
            {
                best = Some((threshold, native_if_at_least, evaluation));
            }
        }
    }
    let (threshold, native_if_at_least, training_evaluation) = best.expect("candidates exist");
    let held_out_evaluation = evaluate_records(
        &held_out,
        Partition::HeldOut,
        &objective,
        |features| stump_route(features, threshold, native_if_at_least),
        &native_engine,
        &ocr_engine,
    );
    let baseline_policies = vec![
        BaselinePolicy::AlwaysNative,
        BaselinePolicy::NativeThreshold {
            minimum_characters: 32,
        },
        BaselinePolicy::PageTypeRules {
            minimum_characters: 32,
        },
        BaselinePolicy::ProfileSpecific {
            profile: Profile::Balanced,
        },
    ];
    let baselines: Vec<_> = baseline_policies
        .into_iter()
        .map(|policy| {
            let evaluation = evaluate_records(
                &held_out,
                Partition::HeldOut,
                &objective,
                |features| policy.route(features),
                &native_engine,
                &ocr_engine,
            );
            (policy, evaluation)
        })
        .collect();
    let best_baseline = baselines
        .iter()
        .map(|(_, result)| result.objective)
        .fold(f64::NEG_INFINITY, f64::max);
    let observed = training
        .iter()
        .filter(|record| record.features.native_characters.observed().is_some())
        .count();
    let confidence = observed as f64 / training.len() as f64;
    let qualification = if confidence < objective.minimum_confidence {
        Qualification::Rejected {
            reason: "training feature coverage is below the declared confidence floor".into(),
        }
    } else if held_out_evaluation.objective <= best_baseline + objective.minimum_improvement {
        Qualification::Rejected {
            reason: "model did not beat deterministic baselines on identical held-out cases".into(),
        }
    } else {
        Qualification::Qualified
    };
    Ok(RouterModel {
        model_version: MODEL_VERSION.into(),
        feature_schema_version: FEATURE_SCHEMA_VERSION.into(),
        dataset_digest: Sha256Digest::of_bytes(&serde_json::to_vec(dataset)?),
        threshold,
        native_if_at_least,
        native_engine,
        ocr_engine,
        confidence,
        minimum_confidence: objective.minimum_confidence,
        qualification,
        training: training_evaluation,
        held_out: held_out_evaluation,
        baselines,
    })
}

fn validate_objective(objective: &TrainingObjective) -> Result<(), RouterError> {
    if !objective.latency_penalty_per_second.is_finite()
        || objective.latency_penalty_per_second < 0.0
        || !objective.failure_penalty.is_finite()
        || objective.failure_penalty < 0.0
        || !objective.minimum_improvement.is_finite()
        || objective.minimum_improvement < 0.0
        || !objective.minimum_confidence.is_finite()
        || !(0.0..=1.0).contains(&objective.minimum_confidence)
    {
        return Err(RouterError::Integrity("invalid training objective".into()));
    }
    Ok(())
}

fn common_engine(records: &[RoutingRecord], route: RouteClass) -> Result<String, RouterError> {
    let engines: BTreeSet<_> = records
        .iter()
        .flat_map(|record| &record.outcomes)
        .filter(|outcome| outcome.route == route)
        .map(|outcome| outcome.engine_id.clone())
        .collect();
    if engines.len() != 1 {
        return Err(RouterError::Integrity(format!(
            "training requires exactly one {:?} candidate across all cases",
            route
        )));
    }
    Ok(engines.into_iter().next().expect("one engine"))
}

fn stump_route(features: &RoutingFeatures, threshold: u64, native_if_at_least: bool) -> RouteClass {
    let Some(characters) = features.native_characters.observed() else {
        return RouteClass::Native;
    };
    if (*characters >= threshold) == native_if_at_least {
        RouteClass::Native
    } else {
        RouteClass::Ocr
    }
}

fn evaluate_records(
    records: &[&RoutingRecord],
    partition: Partition,
    objective: &TrainingObjective,
    route: impl Fn(&RoutingFeatures) -> RouteClass,
    native_engine: &str,
    ocr_engine: &str,
) -> RouterEvaluation {
    let mut quality = 0.0;
    let mut latency = Vec::new();
    let mut failures = 0_u64;
    let mut case_ids = Vec::with_capacity(records.len());
    for record in records {
        case_ids.push(record.case_id.clone());
        let selected_engine = match route(&record.features) {
            RouteClass::Native => native_engine,
            RouteClass::Ocr => ocr_engine,
        };
        match record
            .outcomes
            .iter()
            .find(|outcome| outcome.engine_id == selected_engine)
        {
            Some(outcome) => {
                quality += outcome.quality;
                failures += u64::from(outcome.failed);
                if let Some(value) = outcome.cold_wall_ms {
                    latency.push(value);
                }
            }
            None => failures += 1,
        }
    }
    case_ids.sort();
    let count = records.len() as f64;
    let mean_quality = quality / count;
    let failure_rate = failures as f64 / count;
    let mean_latency_ms =
        (latency.len() == records.len()).then(|| latency.iter().sum::<f64>() / count);
    let latency_penalty = mean_latency_ms
        .map(|value| value / 1000.0 * objective.latency_penalty_per_second)
        .unwrap_or(0.0);
    RouterEvaluation {
        partition,
        case_ids,
        mean_quality,
        mean_latency_ms,
        failure_rate,
        objective: mean_quality - failure_rate * objective.failure_penalty - latency_penalty,
    }
}

/// Why a concrete decision was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSource {
    QualifiedModel,
    DeterministicFallback,
}

/// Planner-safe route decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RouteDecision {
    pub engine_id: String,
    pub source: DecisionSource,
    pub reason: String,
}

/// Applies a learned recommendation only after planner hard-policy filtering.
pub fn guarded_decision(
    model: &RouterModel,
    features: &RoutingFeatures,
    planner_accepted: &BTreeSet<String>,
    fallback_order: &[String],
) -> Result<RouteDecision, RouterError> {
    let fallback = || {
        fallback_order
            .iter()
            .find(|engine| planner_accepted.contains(*engine))
            .cloned()
            .or_else(|| planner_accepted.iter().next().cloned())
            .map(|engine_id| RouteDecision {
                engine_id,
                source: DecisionSource::DeterministicFallback,
                reason: "learned output unavailable, unqualified, low-confidence, or rejected by hard policy".into(),
            })
            .ok_or_else(|| RouterError::Integrity("planner accepted no candidate".into()))
    };
    if model.model_version != MODEL_VERSION
        || model.feature_schema_version != FEATURE_SCHEMA_VERSION
        || model.qualification != Qualification::Qualified
        || model.confidence < model.minimum_confidence
        || features.native_characters.observed().is_none()
    {
        return fallback();
    }
    let engine_id = match stump_route(features, model.threshold, model.native_if_at_least) {
        RouteClass::Native => &model.native_engine,
        RouteClass::Ocr => &model.ocr_engine,
    };
    if !planner_accepted.contains(engine_id) {
        return fallback();
    }
    Ok(RouteDecision {
        engine_id: engine_id.clone(),
        source: DecisionSource::QualifiedModel,
        reason: "qualified model recommendation passed planner hard policy".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features(chars: Option<u64>) -> RoutingFeatures {
        RoutingFeatures {
            page_count: FeatureValue::Observed { value: 1 },
            native_characters: chars.map_or_else(
                || FeatureValue::Missing {
                    reason: "probe failed".into(),
                },
                |value| FeatureValue::Observed { value },
            ),
            image_coverage: FeatureValue::Observed { value: 0.2 },
            scanned_likelihood: FeatureValue::Observed { value: 0.1 },
        }
    }

    fn evaluation() -> RouterEvaluation {
        RouterEvaluation {
            partition: Partition::HeldOut,
            case_ids: vec!["case".into()],
            mean_quality: 1.0,
            mean_latency_ms: Some(1.0),
            failure_rate: 0.0,
            objective: 1.0,
        }
    }

    fn qualified_model() -> RouterModel {
        RouterModel {
            model_version: MODEL_VERSION.into(),
            feature_schema_version: FEATURE_SCHEMA_VERSION.into(),
            dataset_digest: Sha256Digest::of_bytes(b"dataset"),
            threshold: 32,
            native_if_at_least: true,
            native_engine: "native".into(),
            ocr_engine: "ocr".into(),
            confidence: 1.0,
            minimum_confidence: 0.6,
            qualification: Qualification::Qualified,
            training: evaluation(),
            held_out: evaluation(),
            baselines: Vec::new(),
        }
    }

    #[test]
    fn learned_output_cannot_override_planner() {
        let decision = guarded_decision(
            &qualified_model(),
            &features(Some(0)),
            &BTreeSet::from(["native".into()]),
            &["native".into()],
        )
        .unwrap();
        assert_eq!(decision.engine_id, "native");
        assert_eq!(decision.source, DecisionSource::DeterministicFallback);
    }

    #[test]
    fn missing_features_fall_back_deterministically() {
        let accepted = BTreeSet::from(["native".into(), "ocr".into()]);
        let decision = guarded_decision(
            &qualified_model(),
            &features(None),
            &accepted,
            &["native".into(), "ocr".into()],
        )
        .unwrap();
        assert_eq!(decision.engine_id, "native");
        assert_eq!(decision.source, DecisionSource::DeterministicFallback);
    }

    #[test]
    fn related_variants_cannot_cross_partitions() {
        let source = ArtifactSource {
            path: "trace.json".into(),
            digest: Sha256Digest::of_bytes(b"trace"),
        };
        let outcome = |case_id: &str| RouteOutcome {
            engine_id: "native".into(),
            route: RouteClass::Native,
            benchmark_report: source.clone(),
            case_id: case_id.into(),
            quality: 1.0,
            failed: false,
            cold_wall_ms: None,
        };
        let record = |case_id: &str, partition| RoutingRecord {
            case_id: case_id.into(),
            document_digest: Sha256Digest::of_bytes(case_id.as_bytes()),
            family_id: "same-family".into(),
            partition,
            conversion_trace: source.clone(),
            features: features(Some(100)),
            outcomes: vec![outcome(case_id)],
        };
        let dataset = RoutingDataset {
            dataset_version: DATASET_VERSION.into(),
            feature_schema_version: FEATURE_SCHEMA_VERSION.into(),
            corpus_digest: Sha256Digest::of_bytes(b"corpus"),
            records: vec![
                record("train", Partition::Train),
                record("held", Partition::HeldOut),
            ],
        };
        assert!(dataset.validate().is_err());
    }

    #[test]
    fn baselines_handle_missing_values_explicitly() {
        assert_eq!(
            BaselinePolicy::NativeThreshold {
                minimum_characters: 1
            }
            .route(&features(None)),
            RouteClass::Ocr
        );
        assert_eq!(
            BaselinePolicy::AlwaysNative.route(&features(None)),
            RouteClass::Native
        );
    }
}
