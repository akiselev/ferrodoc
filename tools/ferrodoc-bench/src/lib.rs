//! Integrity-first benchmark contracts, metrics, measurement, and comparison.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use ferrodoc_core::{
    Bytes, CURRENT_SCHEMA_VERSION, MicroUsd, Millis, Probability, SchemaVersion, Sha256Digest,
};
use ferrodoc_foundry::{
    BlockKind, CorpusManifest, FormulaTruth, Partition, TableTruth, TruthDocument, TruthRect,
    verify as verify_corpus,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

/// Benchmark report schema version.
pub const REPORT_VERSION: &str = "ferrodoc-benchmark-report/1";
/// Metric semantics version.
pub const METRIC_VERSION: &str = "ferrodoc-metrics/1";

/// Candidate identity required for reproducibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CandidateIdentity {
    /// Engine ID.
    pub engine_id: String,
    /// Engine implementation version.
    pub engine_version: String,
    /// Optional immutable model digest.
    pub model_digest: Option<Sha256Digest>,
    /// Digest of normalized candidate configuration.
    pub configuration_digest: Sha256Digest,
    /// Rust toolchain identity or external runtime identity.
    pub toolchain: String,
}

/// A value whose evidence class cannot be confused with zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EvidenceValue<T> {
    /// Direct observation.
    Measured {
        /// Observed value.
        value: T,
        /// Stable measurement method.
        method: String,
    },
    /// Defensible estimate, not an observation.
    Estimated {
        /// Estimated value.
        value: T,
        /// Stable estimation method.
        method: String,
    },
    /// No defensible value is available.
    Unknown {
        /// Explanation.
        reason: String,
    },
}

impl<T> EvidenceValue<T> {
    /// Returns a reference to measured or estimated values.
    pub const fn value(&self) -> Option<&T> {
        match self {
            Self::Measured { value, .. } | Self::Estimated { value, .. } => Some(value),
            Self::Unknown { .. } => None,
        }
    }
}

/// Summary over repeated samples.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SampleSummary {
    /// Sample count.
    pub count: u32,
    /// Minimum milliseconds.
    pub min_ms: f64,
    /// Arithmetic mean milliseconds.
    pub mean_ms: f64,
    /// Maximum milliseconds.
    pub max_ms: f64,
    /// Population standard deviation in milliseconds.
    pub stddev_ms: f64,
}

/// Cold/warm wall and CPU measurements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimingReport {
    /// First wall-clock sample.
    pub cold_wall_ms: EvidenceValue<f64>,
    /// Remaining wall-clock samples.
    pub warm_wall_ms: EvidenceValue<SampleSummary>,
    /// First process CPU sample.
    pub cold_cpu_ms: EvidenceValue<f64>,
    /// Remaining process CPU samples.
    pub warm_cpu_ms: EvidenceValue<SampleSummary>,
}

/// Resource and cost evidence attached to a case or aggregate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResourceReport {
    /// Peak resident memory.
    pub peak_rss: EvidenceValue<Bytes>,
    /// Peak device VRAM.
    pub peak_vram: EvidenceValue<Bytes>,
    /// Model loading duration.
    pub model_load_time: EvidenceValue<Millis>,
    /// Retained warm memory.
    pub warm_residency: EvidenceValue<Bytes>,
    /// Remote monetary cost.
    pub remote_cost: EvidenceValue<MicroUsd>,
}

impl Default for ResourceReport {
    fn default() -> Self {
        Self {
            peak_rss: unknown("peak RSS was not supplied"),
            peak_vram: unknown("peak VRAM was not supplied"),
            model_load_time: unknown("model load time was not supplied"),
            warm_residency: unknown("warm residency was not supplied"),
            remote_cost: unknown("remote cost was not supplied"),
        }
    }
}

fn unknown<T>(reason: &str) -> EvidenceValue<T> {
    EvidenceValue::Unknown {
        reason: reason.into(),
    }
}

/// Predicted spatial region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PredictedRegion {
    /// Prediction-local ID used by table/formula association.
    pub id: String,
    /// Predicted category.
    pub kind: BlockKind,
    /// Predicted text.
    pub text: String,
    /// Predicted PDF-point geometry.
    pub rect: TruthRect,
}

/// Complete prediction for a successful case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PredictionDocument {
    /// Full predicted text.
    pub text: String,
    /// Predicted regions.
    pub regions: Vec<PredictedRegion>,
    /// Directed reading-order edges using prediction IDs.
    #[serde(default)]
    pub reading_order: Vec<[String; 2]>,
    /// Tables associated with predicted region IDs.
    #[serde(default)]
    pub tables: Vec<TableTruth>,
    /// Formulas associated with predicted region IDs.
    #[serde(default)]
    pub formulas: Vec<FormulaTruth>,
}

/// Candidate-supplied case outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PredictionOutcome {
    /// Conversion succeeded.
    Success {
        /// Candidate prediction.
        prediction: PredictionDocument,
    },
    /// Conversion failed and must count as a failure.
    Failure {
        /// Stable failure category.
        category: String,
        /// Redacted failure explanation.
        message: String,
    },
    /// Case was explicitly excluded.
    Skipped {
        /// Visible exclusion rationale.
        exclusion: String,
    },
}

/// One candidate prediction record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CasePrediction {
    /// Exact manifest case ID.
    pub case_id: String,
    /// Outcome.
    #[serde(flatten)]
    pub outcome: PredictionOutcome,
    /// Optional timing evidence.
    pub timing: Option<TimingReport>,
    /// Resource/cost evidence; unknown remains explicit.
    #[serde(default)]
    pub resources: ResourceReport,
}

/// Predictions to evaluate against one exact corpus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PredictionSet {
    /// Prediction schema.
    pub schema_version: SchemaVersion,
    /// Exact corpus identity.
    pub corpus_digest: Sha256Digest,
    /// Candidate identity.
    pub candidate: CandidateIdentity,
    /// Case predictions. Missing manifest cases become failures; extras reject the set.
    pub cases: Vec<CasePrediction>,
}

/// Edit-distance score with visible denominator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorRate {
    /// Edit operations.
    pub errors: u64,
    /// Truth units.
    pub truth_units: u64,
    /// Errors divided by truth units; may exceed one.
    pub rate: f64,
}

/// Precision/recall/F1 counts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EdgeScore {
    /// Correct predicted truth edges.
    pub true_positive: u64,
    /// Predicted edges absent from truth.
    pub false_positive: u64,
    /// Truth edges not recovered.
    pub false_negative: u64,
    /// Harmonic mean; zero when no correct edge exists.
    pub f1: f64,
}

/// One-to-one region assignment score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegionScore {
    /// Truth region count.
    pub truth_regions: u64,
    /// Prediction region count.
    pub predicted_regions: u64,
    /// Spatially matched pairs.
    pub matched_regions: u64,
    /// Matched pairs with correct category.
    pub classification_correct: u64,
    /// Mean IoU over matched pairs, zero when none match.
    pub mean_iou: f64,
}

/// Quality breakdown for one truth-region category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CategoryMetrics {
    /// Number of truth regions in this category.
    pub truth_regions: u64,
    /// One-to-one matches whose prediction has this category.
    pub correct_matches: u64,
    /// Mean IoU with unmatched or misclassified truth regions contributing zero.
    pub mean_iou: f64,
    /// Mean normalized region-text similarity with missing matches contributing zero.
    pub text_similarity: f64,
    /// Mean of coverage, IoU, and region-text similarity.
    pub quality: Probability,
}

/// Spatially associated table score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TableScore {
    /// Number of truth tables.
    pub truth_tables: u64,
    /// Truth tables with a spatially matched prediction table.
    pub associated_tables: u64,
    /// Mean row/column structure similarity.
    pub structure_similarity: f64,
    /// Mean normalized cell-text similarity.
    pub semantic_similarity: f64,
}

/// Spatially associated formula score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FormulaScore {
    /// Number of truth formulas.
    pub truth_formulas: u64,
    /// Truth formulas with a spatially matched prediction formula.
    pub associated_formulas: u64,
    /// Mean normalized LaTeX-token similarity.
    pub token_similarity: f64,
}

/// Complete metrics for one successful case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CaseMetrics {
    /// Unicode-normalized character error rate.
    pub cer: ErrorRate,
    /// Unicode-normalized word error rate.
    pub wer: ErrorRate,
    /// Reading-order edge score.
    pub reading_order: EdgeScore,
    /// One-to-one spatial region score.
    pub regions: RegionScore,
    /// Explicit quality breakdown for every category present in truth.
    pub categories: BTreeMap<BlockKind, CategoryMetrics>,
    /// Table score when tables exist in truth.
    pub tables: Option<TableScore>,
    /// Formula score when formulas exist in truth.
    pub formulas: Option<FormulaScore>,
    /// Exact semantic regression check for regression cases.
    pub exact_regression: Option<bool>,
    /// Average of applicable quality components.
    pub quality: Probability,
}

/// Evaluated case outcome with failures and exclusions retained.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EvaluatedOutcome {
    /// Successful evaluation.
    Success {
        /// Metrics.
        metrics: CaseMetrics,
    },
    /// Conversion/prediction failure, including a missing prediction.
    Failure {
        /// Stable category.
        category: String,
        /// Redacted message.
        message: String,
    },
    /// Visible exclusion.
    Skipped {
        /// Exclusion rationale.
        exclusion: String,
    },
}

/// One complete evaluated case record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CaseResult {
    /// Exact case ID.
    pub case_id: String,
    /// Corpus partition.
    pub partition: Partition,
    /// Evaluated outcome.
    #[serde(flatten)]
    pub outcome: EvaluatedOutcome,
    /// Timing evidence.
    pub timing: Option<TimingReport>,
    /// Resource/cost evidence.
    pub resources: ResourceReport,
}

/// Aggregate quality and coverage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AggregateMetrics {
    /// Total manifest cases.
    pub total_cases: u64,
    /// Successful cases.
    pub succeeded: u64,
    /// Failed cases.
    pub failed: u64,
    /// Explicitly skipped cases.
    pub skipped: u64,
    /// Mean quality with failures included as zero and skips excluded.
    pub quality: Probability,
    /// Failed divided by non-skipped cases.
    pub failure_rate: Probability,
    /// Throughput when measured.
    pub throughput_pages_per_second: EvidenceValue<f64>,
    /// Aggregate latency when measured.
    pub latency_ms: EvidenceValue<f64>,
    /// Aggregate peak RAM evidence.
    pub peak_ram: EvidenceValue<Bytes>,
    /// Aggregate peak VRAM evidence.
    pub peak_vram: EvidenceValue<Bytes>,
    /// Aggregate remote cost evidence.
    pub remote_cost: EvidenceValue<MicroUsd>,
}

/// Complete benchmark report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BenchmarkReport {
    /// Report semantic version.
    pub report_version: String,
    /// Metric semantic version.
    pub metric_version: String,
    /// Exact metric configuration, including category thresholds.
    pub metric_config: MetricConfig,
    /// Exact corpus digest.
    pub corpus_digest: Sha256Digest,
    /// Candidate identity.
    pub candidate: CandidateIdentity,
    /// Complete case accounting.
    pub cases: Vec<CaseResult>,
    /// Aggregate metrics.
    pub aggregate: AggregateMetrics,
}

/// Metric configuration, including category-specific geometry thresholds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MetricConfig {
    /// Minimum IoU by truth category.
    pub geometry_thresholds: BTreeMap<BlockKind, f64>,
}

impl Default for MetricConfig {
    fn default() -> Self {
        Self {
            geometry_thresholds: BTreeMap::from([
                (BlockKind::Heading, 0.5),
                (BlockKind::Paragraph, 0.5),
                (BlockKind::Table, 0.3),
                (BlockKind::Formula, 0.3),
            ]),
        }
    }
}

/// Benchmark integrity or evaluation failure.
#[derive(Debug, Error)]
pub enum BenchError {
    /// Corpus/prediction/report contract is invalid.
    #[error("benchmark integrity error: {0}")]
    Integrity(String),
    /// Artifact I/O failed.
    #[error("benchmark file operation at {path:?}: {source}")]
    Io {
        /// Affected path.
        path: std::path::PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// JSON encoding failed.
    #[error("benchmark JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// Corpus verification failed.
    #[error("corpus verification failed: {0}")]
    Corpus(#[from] ferrodoc_foundry::FoundryError),
}

/// Evaluates a prediction set with complete manifest-case accounting.
pub fn evaluate(
    corpus_root: &Path,
    manifest: &CorpusManifest,
    predictions: &PredictionSet,
    config: &MetricConfig,
) -> Result<BenchmarkReport, BenchError> {
    verify_corpus(corpus_root, manifest)?;
    validate_metric_config(config)?;
    if predictions.schema_version.major != CURRENT_SCHEMA_VERSION.major {
        return Err(BenchError::Integrity(
            "unsupported prediction schema major".into(),
        ));
    }
    if predictions.corpus_digest != manifest.corpus_digest {
        return Err(BenchError::Integrity(
            "prediction corpus digest mismatch".into(),
        ));
    }
    let manifest_ids: BTreeSet<_> = manifest.cases.iter().map(|case| case.id.as_str()).collect();
    let mut supplied = BTreeMap::new();
    for prediction in &predictions.cases {
        if !manifest_ids.contains(prediction.case_id.as_str()) {
            return Err(BenchError::Integrity(format!(
                "prediction contains extra case {:?}",
                prediction.case_id
            )));
        }
        if supplied
            .insert(prediction.case_id.as_str(), prediction)
            .is_some()
        {
            return Err(BenchError::Integrity(format!(
                "duplicate prediction case {:?}",
                prediction.case_id
            )));
        }
    }
    let mut results = Vec::with_capacity(manifest.cases.len());
    for case in &manifest.cases {
        let truth_path = corpus_root.join(&case.truth);
        let truth: TruthDocument = serde_json::from_slice(
            &fs::read(&truth_path).map_err(|source| io_error(&truth_path, source))?,
        )?;
        let (outcome, timing, resources) = match supplied.get(case.id.as_str()) {
            None => (
                EvaluatedOutcome::Failure {
                    category: "missing_prediction".into(),
                    message: "candidate omitted a manifest case".into(),
                },
                None,
                ResourceReport::default(),
            ),
            Some(prediction) => {
                let outcome = match &prediction.outcome {
                    PredictionOutcome::Success { prediction } => EvaluatedOutcome::Success {
                        metrics: score_case(&truth, prediction, case.partition, config)?,
                    },
                    PredictionOutcome::Failure { category, message } => EvaluatedOutcome::Failure {
                        category: category.clone(),
                        message: message.clone(),
                    },
                    PredictionOutcome::Skipped { exclusion } => {
                        if exclusion.trim().is_empty() {
                            return Err(BenchError::Integrity(format!(
                                "case {} has an empty exclusion",
                                case.id
                            )));
                        }
                        EvaluatedOutcome::Skipped {
                            exclusion: exclusion.clone(),
                        }
                    }
                };
                (
                    outcome,
                    prediction.timing.clone(),
                    prediction.resources.clone(),
                )
            }
        };
        results.push(CaseResult {
            case_id: case.id.clone(),
            partition: case.partition,
            outcome,
            timing,
            resources,
        });
    }
    let aggregate = aggregate(&results)?;
    let report = BenchmarkReport {
        report_version: REPORT_VERSION.into(),
        metric_version: METRIC_VERSION.into(),
        metric_config: config.clone(),
        corpus_digest: manifest.corpus_digest,
        candidate: predictions.candidate.clone(),
        cases: results,
        aggregate,
    };
    report.validate()?;
    Ok(report)
}

impl BenchmarkReport {
    /// Rejects empty, duplicate, versionless, or internally inconsistent reports.
    pub fn validate(&self) -> Result<(), BenchError> {
        if self.report_version != REPORT_VERSION || self.metric_version != METRIC_VERSION {
            return Err(BenchError::Integrity(
                "unsupported report or metric version".into(),
            ));
        }
        if self.cases.is_empty() {
            return Err(BenchError::Integrity(
                "benchmark report has no cases".into(),
            ));
        }
        validate_metric_config(&self.metric_config)?;
        let ids: BTreeSet<_> = self
            .cases
            .iter()
            .map(|case| case.case_id.as_str())
            .collect();
        if ids.len() != self.cases.len() {
            return Err(BenchError::Integrity(
                "benchmark report has duplicate cases".into(),
            ));
        }
        if self.aggregate.total_cases != self.cases.len() as u64
            || self.aggregate.succeeded + self.aggregate.failed + self.aggregate.skipped
                != self.aggregate.total_cases
        {
            return Err(BenchError::Integrity(
                "aggregate case accounting mismatch".into(),
            ));
        }
        let succeeded = self
            .cases
            .iter()
            .filter(|case| matches!(case.outcome, EvaluatedOutcome::Success { .. }))
            .count() as u64;
        let failed = self
            .cases
            .iter()
            .filter(|case| matches!(case.outcome, EvaluatedOutcome::Failure { .. }))
            .count() as u64;
        let skipped = self.cases.len() as u64 - succeeded - failed;
        if (succeeded, failed, skipped)
            != (
                self.aggregate.succeeded,
                self.aggregate.failed,
                self.aggregate.skipped,
            )
        {
            return Err(BenchError::Integrity(
                "aggregate outcome counts do not match case records".into(),
            ));
        }
        let denominator = succeeded + failed;
        let expected_quality = self
            .cases
            .iter()
            .map(|case| case_quality(&case.outcome).unwrap_or(0.0))
            .sum::<f64>()
            / denominator.max(1) as f64;
        let expected_failure_rate = ratio(failed, denominator.max(1));
        if (self.aggregate.quality.get() - expected_quality).abs() > 1e-12
            || (self.aggregate.failure_rate.get() - expected_failure_rate).abs() > 1e-12
        {
            return Err(BenchError::Integrity(
                "aggregate quality or failure rate does not match case records".into(),
            ));
        }
        Ok(())
    }
}

fn validate_metric_config(config: &MetricConfig) -> Result<(), BenchError> {
    for kind in [
        BlockKind::Heading,
        BlockKind::Paragraph,
        BlockKind::Table,
        BlockKind::Formula,
    ] {
        let Some(threshold) = config.geometry_thresholds.get(&kind) else {
            return Err(BenchError::Integrity(format!(
                "missing geometry threshold for {kind:?}"
            )));
        };
        if !threshold.is_finite() || !(0.0..=1.0).contains(threshold) {
            return Err(BenchError::Integrity(
                "geometry thresholds must be finite probabilities".into(),
            ));
        }
    }
    Ok(())
}

fn score_case(
    truth: &TruthDocument,
    prediction: &PredictionDocument,
    partition: Partition,
    config: &MetricConfig,
) -> Result<CaseMetrics, BenchError> {
    let truth_text = normalize_text(&truth.text);
    if truth_text.is_empty() {
        return Err(BenchError::Integrity(format!(
            "truth case {} has empty normalized text",
            truth.case_id
        )));
    }
    let predicted_text = normalize_text(&prediction.text);
    let cer = error_rate(
        &truth_text.chars().collect::<Vec<_>>(),
        &predicted_text.chars().collect::<Vec<_>>(),
    );
    let truth_words: Vec<_> = truth_text.split_whitespace().collect();
    let predicted_words: Vec<_> = predicted_text.split_whitespace().collect();
    let wer = error_rate(&truth_words, &predicted_words);
    let assignment = assign_regions(&truth.regions, &prediction.regions, config);
    let reading_order = edge_score(truth, prediction, &assignment);
    let regions = region_score(truth, prediction, &assignment);
    let categories = category_metrics(truth, prediction, &assignment)?;
    let tables = (!truth.tables.is_empty()).then(|| table_score(truth, prediction, &assignment));
    let formulas =
        (!truth.formulas.is_empty()).then(|| formula_score(truth, prediction, &assignment));
    let exact_regression = (partition == Partition::Regression)
        .then(|| exact_regression(truth, prediction, &assignment));
    let mut components = vec![
        similarity_from_error(&cer),
        similarity_from_error(&wer),
        reading_order.f1,
        ratio(regions.classification_correct, regions.truth_regions.max(1)),
        regions.mean_iou,
    ];
    if let Some(score) = &tables {
        components.extend([score.structure_similarity, score.semantic_similarity]);
    }
    if let Some(score) = &formulas {
        components.push(score.token_similarity);
    }
    if let Some(exact) = exact_regression {
        components.push(if exact { 1.0 } else { 0.0 });
    }
    let quality = Probability::new(components.iter().sum::<f64>() / components.len() as f64)
        .map_err(|error| BenchError::Integrity(error.to_string()))?;
    Ok(CaseMetrics {
        cer,
        wer,
        reading_order,
        regions,
        categories,
        tables,
        formulas,
        exact_regression,
        quality,
    })
}

fn category_metrics(
    truth: &TruthDocument,
    prediction: &PredictionDocument,
    assignment: &Assignment,
) -> Result<BTreeMap<BlockKind, CategoryMetrics>, BenchError> {
    let kinds: BTreeSet<_> = truth.regions.iter().map(|region| region.kind).collect();
    kinds
        .into_iter()
        .map(|kind| {
            let truth_indices: Vec<_> = truth
                .regions
                .iter()
                .enumerate()
                .filter_map(|(index, region)| (region.kind == kind).then_some(index))
                .collect();
            let truth_regions = truth_indices.len() as u64;
            let mut correct_matches = 0_u64;
            let mut iou_sum = 0.0;
            let mut text_sum = 0.0;
            for truth_index in truth_indices {
                let Some(prediction_index) = assignment.truth_to_prediction[truth_index] else {
                    continue;
                };
                let predicted = &prediction.regions[prediction_index];
                if predicted.kind != kind {
                    continue;
                }
                correct_matches += 1;
                iou_sum += assignment.ious[truth_index];
                text_sum += string_similarity(&truth.regions[truth_index].text, &predicted.text);
            }
            let denominator = truth_regions as f64;
            let coverage = correct_matches as f64 / denominator;
            let mean_iou = iou_sum / denominator;
            let text_similarity = text_sum / denominator;
            let quality = Probability::new((coverage + mean_iou + text_similarity) / 3.0)
                .map_err(|error| BenchError::Integrity(error.to_string()))?;
            Ok((
                kind,
                CategoryMetrics {
                    truth_regions,
                    correct_matches,
                    mean_iou,
                    text_similarity,
                    quality,
                },
            ))
        })
        .collect()
}

fn normalize_text(input: &str) -> String {
    input
        .nfkc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn error_rate<T: Eq>(truth: &[T], prediction: &[T]) -> ErrorRate {
    let errors = levenshtein(truth, prediction) as u64;
    let truth_units = truth.len() as u64;
    ErrorRate {
        errors,
        truth_units,
        rate: errors as f64 / truth_units.max(1) as f64,
    }
}

fn levenshtein<T: Eq>(left: &[T], right: &[T]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    let mut previous: Vec<_> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_item) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_item) in right.iter().enumerate() {
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + usize::from(left_item != right_item));
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

fn similarity_from_error(rate: &ErrorRate) -> f64 {
    (1.0 - rate.rate).max(0.0)
}

#[derive(Debug)]
struct Assignment {
    truth_to_prediction: Vec<Option<usize>>,
    ious: Vec<f64>,
}

fn assign_regions(
    truth: &[ferrodoc_foundry::TruthRegion],
    predictions: &[PredictedRegion],
    config: &MetricConfig,
) -> Assignment {
    let mut adjacency = Vec::with_capacity(truth.len());
    for truth_region in truth {
        let threshold = config.geometry_thresholds[&truth_region.kind];
        let mut candidates: Vec<_> = predictions
            .iter()
            .enumerate()
            .filter_map(|(index, prediction)| {
                let overlap = iou(truth_region.rect, prediction.rect);
                (overlap >= threshold).then_some((index, overlap))
            })
            .collect();
        candidates.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        adjacency.push(candidates);
    }
    let mut prediction_to_truth = vec![None; predictions.len()];
    for truth_index in 0..truth.len() {
        let mut visited = vec![false; predictions.len()];
        augment(
            truth_index,
            &adjacency,
            &mut visited,
            &mut prediction_to_truth,
        );
    }
    let mut truth_to_prediction = vec![None; truth.len()];
    for (prediction, truth) in prediction_to_truth.into_iter().enumerate() {
        if let Some(truth) = truth {
            truth_to_prediction[truth] = Some(prediction);
        }
    }
    let ious = truth_to_prediction
        .iter()
        .enumerate()
        .map(|(truth_index, prediction)| {
            prediction.map_or(0.0, |prediction_index| {
                iou(truth[truth_index].rect, predictions[prediction_index].rect)
            })
        })
        .collect();
    Assignment {
        truth_to_prediction,
        ious,
    }
}

fn augment(
    truth: usize,
    adjacency: &[Vec<(usize, f64)>],
    visited: &mut [bool],
    prediction_to_truth: &mut [Option<usize>],
) -> bool {
    for &(prediction, _) in &adjacency[truth] {
        if visited[prediction] {
            continue;
        }
        visited[prediction] = true;
        if prediction_to_truth[prediction]
            .is_none_or(|other_truth| augment(other_truth, adjacency, visited, prediction_to_truth))
        {
            prediction_to_truth[prediction] = Some(truth);
            return true;
        }
    }
    false
}

fn iou(left: TruthRect, right: TruthRect) -> f64 {
    let left_right = left.x + left.width;
    let left_top = left.y + left.height;
    let right_right = right.x + right.width;
    let right_top = right.y + right.height;
    let intersection_width = left_right.min(right_right) - left.x.max(right.x);
    let intersection_height = left_top.min(right_top) - left.y.max(right.y);
    if intersection_width <= 0.0 || intersection_height <= 0.0 {
        return 0.0;
    }
    let intersection = intersection_width * intersection_height;
    intersection / (left.width * left.height + right.width * right.height - intersection)
}

fn region_score(
    truth: &TruthDocument,
    prediction: &PredictionDocument,
    assignment: &Assignment,
) -> RegionScore {
    let matched: Vec<_> = assignment
        .truth_to_prediction
        .iter()
        .enumerate()
        .filter_map(|(truth, prediction)| prediction.map(|prediction| (truth, prediction)))
        .collect();
    RegionScore {
        truth_regions: truth.regions.len() as u64,
        predicted_regions: prediction.regions.len() as u64,
        matched_regions: matched.len() as u64,
        classification_correct: matched
            .iter()
            .filter(|(truth_index, prediction_index)| {
                truth.regions[*truth_index].kind == prediction.regions[*prediction_index].kind
            })
            .count() as u64,
        mean_iou: if matched.is_empty() {
            0.0
        } else {
            matched
                .iter()
                .map(|(truth_index, _)| assignment.ious[*truth_index])
                .sum::<f64>()
                / matched.len() as f64
        },
    }
}

fn edge_score(
    truth: &TruthDocument,
    prediction: &PredictionDocument,
    assignment: &Assignment,
) -> EdgeScore {
    let prediction_to_truth_id: BTreeMap<_, _> = assignment
        .truth_to_prediction
        .iter()
        .enumerate()
        .filter_map(|(truth_index, prediction_index)| {
            prediction_index.map(|prediction_index| {
                (
                    prediction.regions[prediction_index].id.as_str(),
                    truth.regions[truth_index].id.as_str(),
                )
            })
        })
        .collect();
    let truth_edges: BTreeSet<_> = truth
        .reading_order
        .iter()
        .map(|edge| (edge[0].as_str(), edge[1].as_str()))
        .collect();
    let predicted_edges: BTreeSet<_> = prediction
        .reading_order
        .iter()
        .filter_map(|edge| {
            Some((
                *prediction_to_truth_id.get(edge[0].as_str())?,
                *prediction_to_truth_id.get(edge[1].as_str())?,
            ))
        })
        .collect();
    let true_positive = truth_edges.intersection(&predicted_edges).count() as u64;
    let false_positive = predicted_edges.difference(&truth_edges).count() as u64;
    let false_negative = truth_edges.difference(&predicted_edges).count() as u64;
    let precision = ratio(true_positive, true_positive + false_positive);
    let recall = ratio(true_positive, true_positive + false_negative);
    let f1 = if truth_edges.is_empty() && predicted_edges.is_empty() {
        1.0
    } else if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    EdgeScore {
        true_positive,
        false_positive,
        false_negative,
        f1,
    }
}

fn table_score(
    truth: &TruthDocument,
    prediction: &PredictionDocument,
    assignment: &Assignment,
) -> TableScore {
    let predicted_tables: BTreeMap<_, _> = prediction
        .tables
        .iter()
        .map(|table| (table.region_id.as_str(), table))
        .collect();
    let truth_indices: BTreeMap<_, _> = truth
        .regions
        .iter()
        .enumerate()
        .map(|(index, region)| (region.id.as_str(), index))
        .collect();
    let mut associated = 0_u64;
    let mut structure = 0.0;
    let mut semantics = 0.0;
    for truth_table in &truth.tables {
        let Some(&truth_index) = truth_indices.get(truth_table.region_id.as_str()) else {
            continue;
        };
        let Some(prediction_index) = assignment.truth_to_prediction[truth_index] else {
            continue;
        };
        let prediction_id = prediction.regions[prediction_index].id.as_str();
        let Some(predicted_table) = predicted_tables.get(prediction_id) else {
            continue;
        };
        associated += 1;
        let truth_rows = truth_table.cells.len();
        let predicted_rows = predicted_table.cells.len();
        let truth_columns = truth_table.cells.first().map_or(0, Vec::len);
        let predicted_columns = predicted_table.cells.first().map_or(0, Vec::len);
        structure += dimension_similarity(truth_rows, predicted_rows)
            * dimension_similarity(truth_columns, predicted_columns);
        let truth_cells: Vec<_> = truth_table.cells.iter().flatten().collect();
        let predicted_cells: Vec<_> = predicted_table.cells.iter().flatten().collect();
        let count = truth_cells.len().max(predicted_cells.len()).max(1);
        semantics += (0..count)
            .map(
                |index| match (truth_cells.get(index), predicted_cells.get(index)) {
                    (Some(truth), Some(predicted)) => string_similarity(truth, predicted),
                    _ => 0.0,
                },
            )
            .sum::<f64>()
            / count as f64;
    }
    TableScore {
        truth_tables: truth.tables.len() as u64,
        associated_tables: associated,
        structure_similarity: structure / truth.tables.len().max(1) as f64,
        semantic_similarity: semantics / truth.tables.len().max(1) as f64,
    }
}

fn formula_score(
    truth: &TruthDocument,
    prediction: &PredictionDocument,
    assignment: &Assignment,
) -> FormulaScore {
    let predicted: BTreeMap<_, _> = prediction
        .formulas
        .iter()
        .map(|formula| (formula.region_id.as_str(), formula))
        .collect();
    let truth_indices: BTreeMap<_, _> = truth
        .regions
        .iter()
        .enumerate()
        .map(|(index, region)| (region.id.as_str(), index))
        .collect();
    let mut associated = 0_u64;
    let mut similarity = 0.0;
    for truth_formula in &truth.formulas {
        let Some(&truth_index) = truth_indices.get(truth_formula.region_id.as_str()) else {
            continue;
        };
        let Some(prediction_index) = assignment.truth_to_prediction[truth_index] else {
            continue;
        };
        let prediction_id = prediction.regions[prediction_index].id.as_str();
        let Some(predicted_formula) = predicted.get(prediction_id) else {
            continue;
        };
        associated += 1;
        let truth_tokens = latex_tokens(&truth_formula.latex);
        let predicted_tokens = latex_tokens(&predicted_formula.latex);
        similarity += sequence_similarity(&truth_tokens, &predicted_tokens);
    }
    FormulaScore {
        truth_formulas: truth.formulas.len() as u64,
        associated_formulas: associated,
        token_similarity: similarity / truth.formulas.len().max(1) as f64,
    }
}

fn latex_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut command = false;
    for character in input.nfkc() {
        if character.is_whitespace() || matches!(character, '{' | '}') {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            command = false;
        } else if character == '\\' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            current.push(character);
            command = true;
        } else if character.is_alphanumeric() || (command && character.is_alphabetic()) {
            current.push(character);
        } else {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push(character.to_string());
            command = false;
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn exact_regression(
    truth: &TruthDocument,
    prediction: &PredictionDocument,
    assignment: &Assignment,
) -> bool {
    normalize_text(&truth.text) == normalize_text(&prediction.text)
        && assignment.truth_to_prediction.iter().all(Option::is_some)
        && truth.regions.len() == prediction.regions.len()
        && truth.tables == prediction.tables
        && truth.formulas == prediction.formulas
        && edge_score(truth, prediction, assignment).f1 == 1.0
        && assignment.truth_to_prediction.iter().enumerate().all(
            |(truth_index, prediction_index)| {
                let prediction = &prediction.regions[prediction_index.expect("all assigned")];
                let truth = &truth.regions[truth_index];
                truth.kind == prediction.kind
                    && normalize_text(&truth.text) == normalize_text(&prediction.text)
                    && truth.rect == prediction.rect
            },
        )
}

fn string_similarity(left: &str, right: &str) -> f64 {
    let left = normalize_text(left);
    let right = normalize_text(right);
    sequence_similarity(
        &left.chars().collect::<Vec<_>>(),
        &right.chars().collect::<Vec<_>>(),
    )
}

fn sequence_similarity<T: Eq>(left: &[T], right: &[T]) -> f64 {
    1.0 - levenshtein(left, right) as f64 / left.len().max(right.len()).max(1) as f64
}

fn dimension_similarity(left: usize, right: usize) -> f64 {
    left.min(right) as f64 / left.max(right).max(1) as f64
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn aggregate(results: &[CaseResult]) -> Result<AggregateMetrics, BenchError> {
    if results.is_empty() {
        return Err(BenchError::Integrity(
            "cannot aggregate an empty suite".into(),
        ));
    }
    let succeeded = results
        .iter()
        .filter(|case| matches!(case.outcome, EvaluatedOutcome::Success { .. }))
        .count() as u64;
    let failed = results
        .iter()
        .filter(|case| matches!(case.outcome, EvaluatedOutcome::Failure { .. }))
        .count() as u64;
    let skipped = results.len() as u64 - succeeded - failed;
    let denominator = succeeded + failed;
    let quality_sum: f64 = results
        .iter()
        .map(|case| match &case.outcome {
            EvaluatedOutcome::Success { metrics } => metrics.quality.get(),
            EvaluatedOutcome::Failure { .. } | EvaluatedOutcome::Skipped { .. } => 0.0,
        })
        .sum();
    let quality = Probability::new(if denominator == 0 {
        0.0
    } else {
        quality_sum / denominator as f64
    })
    .map_err(|error| BenchError::Integrity(error.to_string()))?;
    let failure_rate = Probability::new(ratio(failed, denominator.max(1)))
        .map_err(|error| BenchError::Integrity(error.to_string()))?;
    let latency_values: Vec<_> = results
        .iter()
        .filter_map(|case| case.timing.as_ref())
        .filter_map(|timing| timing.cold_wall_ms.value().copied())
        .collect();
    Ok(AggregateMetrics {
        total_cases: results.len() as u64,
        succeeded,
        failed,
        skipped,
        quality,
        failure_rate,
        throughput_pages_per_second: if latency_values.len() == results.len() {
            let seconds = latency_values.iter().sum::<f64>() / 1000.0;
            EvidenceValue::Measured {
                value: results.len() as f64 / seconds.max(f64::EPSILON),
                method: "sum of supplied cold wall samples".into(),
            }
        } else {
            unknown("not every case supplied cold wall timing")
        },
        latency_ms: if latency_values.len() == results.len() {
            EvidenceValue::Measured {
                value: latency_values.iter().sum(),
                method: "sum of supplied cold wall samples".into(),
            }
        } else {
            unknown("not every case supplied cold wall timing")
        },
        peak_ram: maximum_bytes(
            results.iter().map(|case| &case.resources.peak_rss),
            "peak RAM",
        ),
        peak_vram: maximum_bytes(
            results.iter().map(|case| &case.resources.peak_vram),
            "peak VRAM",
        ),
        remote_cost: sum_cost(results.iter().map(|case| &case.resources.remote_cost)),
    })
}

fn maximum_bytes<'a>(
    values: impl Iterator<Item = &'a EvidenceValue<Bytes>>,
    label: &str,
) -> EvidenceValue<Bytes> {
    let values: Vec<_> = values.collect();
    let known: Vec<_> = values
        .iter()
        .filter_map(|value| value.value().copied())
        .collect();
    if known.len() != values.len() {
        unknown(&format!("at least one case has unknown {label}"))
    } else {
        EvidenceValue::Measured {
            value: known.into_iter().max().unwrap_or_default(),
            method: format!("maximum of per-case {label} evidence"),
        }
    }
}

fn sum_cost<'a>(
    values: impl Iterator<Item = &'a EvidenceValue<MicroUsd>>,
) -> EvidenceValue<MicroUsd> {
    let values: Vec<_> = values.collect();
    let known: Vec<_> = values
        .iter()
        .filter_map(|value| value.value().copied())
        .collect();
    if known.len() != values.len() {
        unknown("at least one case has unknown remote cost")
    } else {
        EvidenceValue::Measured {
            value: MicroUsd::new(
                known
                    .iter()
                    .fold(0_u64, |sum, value| sum.saturating_add(value.get())),
            ),
            method: "sum of per-case remote cost evidence".into(),
        }
    }
}

fn io_error(path: &Path, source: std::io::Error) -> BenchError {
    BenchError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Repeated measurement output retaining every operation result.
pub struct MeasurementRun<T> {
    /// One result per sample, cold first.
    pub outputs: Vec<T>,
    /// Cold/warm timing summary.
    pub timing: TimingReport,
    /// Process-level peak resident memory evidence.
    pub peak_rss: EvidenceValue<Bytes>,
}

/// Runs at least two samples while measuring wall time, process CPU time, and peak RSS.
pub fn measure_repeated<T, E>(
    repeats: u32,
    mut operation: impl FnMut() -> Result<T, E>,
) -> Result<MeasurementRun<T>, E> {
    assert!(repeats >= 2, "measurement requires cold and warm samples");
    let mut outputs = Vec::with_capacity(repeats as usize);
    let mut wall = Vec::with_capacity(repeats as usize);
    let mut cpu = Vec::with_capacity(repeats as usize);
    for _ in 0..repeats {
        let cpu_start = process_cpu_duration();
        let wall_start = std::time::Instant::now();
        outputs.push(operation()?);
        wall.push(wall_start.elapsed().as_secs_f64() * 1000.0);
        cpu.push(cpu_start.and_then(|start| {
            process_cpu_duration().map(|end| end.saturating_sub(start).as_secs_f64() * 1000.0)
        }));
    }
    let cold_wall = wall[0];
    let warm_wall = summarize(&wall[1..]);
    let cold_cpu = cpu[0];
    let warm_cpu: Option<Vec<_>> = cpu[1..].iter().copied().collect();
    Ok(MeasurementRun {
        outputs,
        timing: TimingReport {
            cold_wall_ms: EvidenceValue::Measured {
                value: cold_wall,
                method: "std::time::Instant".into(),
            },
            warm_wall_ms: EvidenceValue::Measured {
                value: warm_wall,
                method: "std::time::Instant repeated samples after first".into(),
            },
            cold_cpu_ms: cold_cpu.map_or_else(
                || unknown("process CPU clock is unavailable"),
                |value| EvidenceValue::Measured {
                    value,
                    method: "getrusage process user+system CPU".into(),
                },
            ),
            warm_cpu_ms: warm_cpu.map_or_else(
                || unknown("process CPU clock is unavailable for at least one sample"),
                |values| EvidenceValue::Measured {
                    value: summarize(&values),
                    method: "getrusage repeated process user+system CPU samples".into(),
                },
            ),
        },
        peak_rss: peak_resident_memory(),
    })
}

fn summarize(samples: &[f64]) -> SampleSummary {
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let variance = samples
        .iter()
        .map(|sample| (sample - mean).powi(2))
        .sum::<f64>()
        / samples.len() as f64;
    SampleSummary {
        count: samples.len() as u32,
        min_ms: samples.iter().copied().fold(f64::INFINITY, f64::min),
        mean_ms: mean,
        max_ms: samples.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        stddev_ms: variance.sqrt(),
    }
}

#[cfg(unix)]
fn process_cpu_duration() -> Option<std::time::Duration> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: `getrusage` initializes the supplied `rusage` on success and the
    // pointer is valid for the duration of the call.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: the successful call above initialized the value.
    let usage = unsafe { usage.assume_init() };
    timeval_duration(usage.ru_utime).checked_add(timeval_duration(usage.ru_stime))
}

#[cfg(unix)]
fn timeval_duration(value: libc::timeval) -> std::time::Duration {
    std::time::Duration::from_secs(value.tv_sec.max(0) as u64)
        + std::time::Duration::from_micros(value.tv_usec.max(0) as u64)
}

#[cfg(not(unix))]
fn process_cpu_duration() -> Option<std::time::Duration> {
    None
}

#[cfg(unix)]
fn peak_resident_memory() -> EvidenceValue<Bytes> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: see `process_cpu_duration`; the same contract applies here.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return unknown("getrusage failed");
    }
    // SAFETY: the successful call initialized the value.
    let usage = unsafe { usage.assume_init() };
    #[cfg(target_os = "macos")]
    let bytes = usage.ru_maxrss.max(0) as u64;
    #[cfg(not(target_os = "macos"))]
    let bytes = (usage.ru_maxrss.max(0) as u64).saturating_mul(Bytes::KIB);
    EvidenceValue::Measured {
        value: Bytes::new(bytes),
        method: "getrusage(RUSAGE_SELF).ru_maxrss process-lifetime peak".into(),
    }
}

#[cfg(not(unix))]
fn peak_resident_memory() -> EvidenceValue<Bytes> {
    unknown("peak RSS measurement is unsupported on this platform")
}

/// Dimension used for policy-specific Pareto comparison.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum Dimension {
    /// Higher aggregate quality is better.
    Quality,
    /// Higher throughput is better.
    Throughput,
    /// Lower latency is better.
    Latency,
    /// Lower peak RAM is better.
    Ram,
    /// Lower peak VRAM is better.
    Vram,
    /// Lower remote cost is better.
    Cost,
    /// Lower failure rate is better.
    FailureRate,
}

/// Dimensions and numerical tolerances used for comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComparisonPolicy {
    /// Dimensions required to be known and considered.
    pub required: BTreeSet<Dimension>,
    /// No-worse/better tolerance per dimension in its native units.
    #[serde(default)]
    pub tolerances: BTreeMap<Dimension, f64>,
}

impl Default for ComparisonPolicy {
    fn default() -> Self {
        Self {
            required: BTreeSet::from([
                Dimension::Quality,
                Dimension::Latency,
                Dimension::Ram,
                Dimension::FailureRate,
            ]),
            tolerances: BTreeMap::new(),
        }
    }
}

/// Pareto relationship between candidate and baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum Dominance {
    /// Candidate is no worse in every required dimension and better in at least one.
    CandidateDominates,
    /// Baseline is no worse in every required dimension and better in at least one.
    BaselineDominates,
    /// Each side wins at least one dimension.
    Tradeoff,
    /// A required dimension is unknown.
    Indeterminate,
    /// All considered dimensions are equal within tolerance.
    Equal,
}

/// One case-level comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CaseDelta {
    /// Case ID.
    pub case_id: String,
    /// Candidate quality minus baseline quality when both succeeded.
    pub quality_delta: Option<f64>,
    /// Candidate outcome summary.
    pub candidate_status: String,
    /// Baseline outcome summary.
    pub baseline_status: String,
}

/// Complete multidimensional comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComparisonReport {
    /// Exact corpus digest.
    pub corpus_digest: Sha256Digest,
    /// Applied policy.
    pub policy: ComparisonPolicy,
    /// Pareto relationship.
    pub dominance: Dominance,
    /// Candidate-minus-baseline normalized dimension deltas; higher is better.
    pub dimension_deltas: BTreeMap<Dimension, EvidenceValue<f64>>,
    /// Per-case regressions and improvements.
    pub cases: Vec<CaseDelta>,
    /// Visible repeated-sample variance notes.
    pub variance_notes: Vec<String>,
}

/// Compares compatible reports without collapsing unknown dimensions to zero.
pub fn compare(
    baseline: &BenchmarkReport,
    candidate: &BenchmarkReport,
    policy: ComparisonPolicy,
) -> Result<ComparisonReport, BenchError> {
    baseline.validate()?;
    candidate.validate()?;
    if baseline.corpus_digest != candidate.corpus_digest {
        return Err(BenchError::Integrity(
            "comparison corpus digests differ".into(),
        ));
    }
    if baseline.metric_version != candidate.metric_version
        || baseline.report_version != candidate.report_version
        || baseline.metric_config != candidate.metric_config
    {
        return Err(BenchError::Integrity(
            "comparison report version, metric version, or metric configuration differs".into(),
        ));
    }
    let baseline_ids: BTreeSet<_> = baseline.cases.iter().map(|case| &case.case_id).collect();
    let candidate_ids: BTreeSet<_> = candidate.cases.iter().map(|case| &case.case_id).collect();
    if baseline_ids != candidate_ids {
        return Err(BenchError::Integrity(
            "candidate and baseline case sets differ".into(),
        ));
    }
    let mut deltas = BTreeMap::new();
    let mut candidate_no_worse = true;
    let mut baseline_no_worse = true;
    let mut candidate_better = false;
    let mut baseline_better = false;
    let mut indeterminate = false;
    for dimension in &policy.required {
        let values = dimension_values(*dimension, baseline, candidate);
        let tolerance = policy.tolerances.get(dimension).copied().unwrap_or(0.0);
        match values {
            Some((baseline_value, candidate_value)) => {
                let delta = candidate_value - baseline_value;
                deltas.insert(
                    *dimension,
                    EvidenceValue::Measured {
                        value: delta,
                        method: "candidate minus baseline; sign normalized so higher is better"
                            .into(),
                    },
                );
                if delta < -tolerance {
                    candidate_no_worse = false;
                    baseline_better = true;
                }
                if delta > tolerance {
                    baseline_no_worse = false;
                    candidate_better = true;
                }
            }
            None => {
                indeterminate = true;
                deltas.insert(
                    *dimension,
                    unknown("required comparison dimension is unknown"),
                );
            }
        }
    }
    let candidate_omitted_baseline_success = candidate.cases.iter().any(|candidate_case| {
        matches!(
            candidate_case.outcome,
            EvaluatedOutcome::Failure { ref category, .. } if category == "missing_prediction"
        ) && baseline
            .cases
            .iter()
            .find(|baseline_case| baseline_case.case_id == candidate_case.case_id)
            .is_some_and(|baseline_case| {
                matches!(baseline_case.outcome, EvaluatedOutcome::Success { .. })
            })
    });
    if candidate_omitted_baseline_success {
        candidate_no_worse = false;
        baseline_better = true;
    }
    let dominance = if indeterminate {
        Dominance::Indeterminate
    } else if candidate_no_worse && candidate_better {
        Dominance::CandidateDominates
    } else if baseline_no_worse && baseline_better {
        Dominance::BaselineDominates
    } else if !candidate_better && !baseline_better {
        Dominance::Equal
    } else {
        Dominance::Tradeoff
    };
    let baseline_by_id: BTreeMap<_, _> = baseline
        .cases
        .iter()
        .map(|case| (case.case_id.as_str(), case))
        .collect();
    let cases = candidate
        .cases
        .iter()
        .map(|candidate_case| {
            let baseline_case = baseline_by_id[candidate_case.case_id.as_str()];
            CaseDelta {
                case_id: candidate_case.case_id.clone(),
                quality_delta: case_quality(&candidate_case.outcome)
                    .zip(case_quality(&baseline_case.outcome))
                    .map(|(candidate, baseline)| candidate - baseline),
                candidate_status: outcome_name(&candidate_case.outcome).into(),
                baseline_status: outcome_name(&baseline_case.outcome).into(),
            }
        })
        .collect();
    let variance_notes = variance_notes(baseline, "baseline")
        .into_iter()
        .chain(variance_notes(candidate, "candidate"))
        .collect();
    Ok(ComparisonReport {
        corpus_digest: baseline.corpus_digest,
        policy,
        dominance,
        dimension_deltas: deltas,
        cases,
        variance_notes,
    })
}

fn dimension_values(
    dimension: Dimension,
    baseline: &BenchmarkReport,
    candidate: &BenchmarkReport,
) -> Option<(f64, f64)> {
    match dimension {
        Dimension::Quality => Some((
            baseline.aggregate.quality.get(),
            candidate.aggregate.quality.get(),
        )),
        Dimension::FailureRate => Some((
            -baseline.aggregate.failure_rate.get(),
            -candidate.aggregate.failure_rate.get(),
        )),
        Dimension::Throughput => Some((
            *baseline.aggregate.throughput_pages_per_second.value()?,
            *candidate.aggregate.throughput_pages_per_second.value()?,
        )),
        Dimension::Latency => Some((
            -*baseline.aggregate.latency_ms.value()?,
            -*candidate.aggregate.latency_ms.value()?,
        )),
        Dimension::Ram => Some((
            -(baseline.aggregate.peak_ram.value()?.get() as f64),
            -(candidate.aggregate.peak_ram.value()?.get() as f64),
        )),
        Dimension::Vram => Some((
            -(baseline.aggregate.peak_vram.value()?.get() as f64),
            -(candidate.aggregate.peak_vram.value()?.get() as f64),
        )),
        Dimension::Cost => Some((
            -(baseline.aggregate.remote_cost.value()?.get() as f64),
            -(candidate.aggregate.remote_cost.value()?.get() as f64),
        )),
    }
}

fn case_quality(outcome: &EvaluatedOutcome) -> Option<f64> {
    match outcome {
        EvaluatedOutcome::Success { metrics } => Some(metrics.quality.get()),
        EvaluatedOutcome::Failure { .. } => Some(0.0),
        EvaluatedOutcome::Skipped { .. } => None,
    }
}

fn outcome_name(outcome: &EvaluatedOutcome) -> &'static str {
    match outcome {
        EvaluatedOutcome::Success { .. } => "success",
        EvaluatedOutcome::Failure { .. } => "failure",
        EvaluatedOutcome::Skipped { .. } => "skipped",
    }
}

fn variance_notes(report: &BenchmarkReport, label: &str) -> Vec<String> {
    report
        .cases
        .iter()
        .filter_map(|case| {
            let summary = case.timing.as_ref()?.warm_wall_ms.value()?;
            Some(format!(
                "{label} {} warm wall n={} mean={:.3}ms stddev={:.3}ms",
                case.case_id, summary.count, summary.mean_ms, summary.stddev_ms
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ferrodoc_foundry::{
        AssetSpec, BlockSpec, CaseSpec, Degradation, FoundrySpec, RedistributionStatus, generate,
    };

    use super::*;

    fn foundry_spec(partition: Partition) -> FoundrySpec {
        FoundrySpec {
            schema_version: CURRENT_SCHEMA_VERSION,
            seed: 7,
            assets: vec![AssetSpec {
                id: "pdf.helvetica".into(),
                license: "ISO-32000-2".into(),
                source: "PDF standard 14 font".into(),
                redistribution: RedistributionStatus::BuiltIn,
                digest: None,
            }],
            cases: vec![CaseSpec {
                name: "benchmark".into(),
                partition,
                degradation: Degradation::Native,
                blocks: vec![
                    BlockSpec {
                        id: "table".into(),
                        kind: BlockKind::Table,
                        text: "A B".into(),
                        font_size: 12,
                        x: 10.0,
                        y: 100.0,
                        width: 100.0,
                        height: 50.0,
                    },
                    BlockSpec {
                        id: "formula".into(),
                        kind: BlockKind::Formula,
                        text: "x + y".into(),
                        font_size: 12,
                        x: 10.0,
                        y: 40.0,
                        width: 100.0,
                        height: 30.0,
                    },
                ],
                reading_order: vec![["table".into(), "formula".into()]],
                tables: vec![TableTruth {
                    region_id: "table".into(),
                    cells: vec![vec!["A".into(), "B".into()]],
                }],
                formulas: vec![FormulaTruth {
                    region_id: "formula".into(),
                    latex: "x + y".into(),
                }],
            }],
        }
    }

    fn candidate(id: &str) -> CandidateIdentity {
        CandidateIdentity {
            engine_id: id.into(),
            engine_version: "1".into(),
            model_digest: None,
            configuration_digest: Sha256Digest::of_bytes(b"config"),
            toolchain: "fixture".into(),
        }
    }

    fn prediction_from_truth(truth: &TruthDocument) -> PredictionDocument {
        PredictionDocument {
            text: truth.text.clone(),
            regions: truth
                .regions
                .iter()
                .map(|region| PredictedRegion {
                    id: region.id.clone(),
                    kind: region.kind,
                    text: region.text.clone(),
                    rect: region.rect,
                })
                .collect(),
            reading_order: truth.reading_order.clone(),
            tables: truth.tables.clone(),
            formulas: truth.formulas.clone(),
        }
    }

    fn generated(
        partition: Partition,
    ) -> (
        tempfile::TempDir,
        std::path::PathBuf,
        CorpusManifest,
        TruthDocument,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("corpus");
        let manifest = generate(&foundry_spec(partition), &root).unwrap();
        let truth: TruthDocument =
            serde_json::from_slice(&fs::read(root.join(&manifest.cases[0].truth)).unwrap())
                .unwrap();
        (directory, root, manifest, truth)
    }

    fn predictions(
        manifest: &CorpusManifest,
        truth: &TruthDocument,
        identity: CandidateIdentity,
    ) -> PredictionSet {
        PredictionSet {
            schema_version: CURRENT_SCHEMA_VERSION,
            corpus_digest: manifest.corpus_digest,
            candidate: identity,
            cases: vec![CasePrediction {
                case_id: manifest.cases[0].id.clone(),
                outcome: PredictionOutcome::Success {
                    prediction: prediction_from_truth(truth),
                },
                timing: None,
                resources: ResourceReport::default(),
            }],
        }
    }

    #[test]
    fn perfect_regression_requires_spatial_and_semantic_equality() {
        let (_directory, root, manifest, truth) = generated(Partition::Regression);
        let report = evaluate(
            &root,
            &manifest,
            &predictions(&manifest, &truth, candidate("perfect")),
            &MetricConfig::default(),
        )
        .unwrap();
        assert_eq!(report.aggregate.quality.get(), 1.0);
        let EvaluatedOutcome::Success { metrics } = &report.cases[0].outcome else {
            panic!("expected success")
        };
        assert_eq!(metrics.exact_regression, Some(true));
        assert_eq!(metrics.tables.as_ref().unwrap().associated_tables, 1);
        assert_eq!(metrics.formulas.as_ref().unwrap().associated_formulas, 1);
        assert_eq!(metrics.categories[&BlockKind::Table].quality.get(), 1.0);
        assert_eq!(metrics.categories[&BlockKind::Formula].quality.get(), 1.0);
    }

    #[test]
    fn missing_work_is_failure_and_cannot_dominate_complete_baseline() {
        let (_directory, root, manifest, truth) = generated(Partition::HeldOut);
        let baseline = evaluate(
            &root,
            &manifest,
            &predictions(&manifest, &truth, candidate("baseline")),
            &MetricConfig::default(),
        )
        .unwrap();
        let missing = PredictionSet {
            schema_version: CURRENT_SCHEMA_VERSION,
            corpus_digest: manifest.corpus_digest,
            candidate: candidate("missing"),
            cases: Vec::new(),
        };
        let candidate = evaluate(&root, &manifest, &missing, &MetricConfig::default()).unwrap();
        assert_eq!(candidate.aggregate.failed, 1);
        assert_eq!(candidate.aggregate.quality.get(), 0.0);
        let comparison = compare(
            &baseline,
            &candidate,
            ComparisonPolicy {
                required: BTreeSet::from([Dimension::Quality, Dimension::FailureRate]),
                tolerances: BTreeMap::new(),
            },
        )
        .unwrap();
        assert_ne!(comparison.dominance, Dominance::CandidateDominates);
    }

    #[test]
    fn one_prediction_cannot_satisfy_two_truth_regions() {
        let truth = TruthDocument {
            schema_version: CURRENT_SCHEMA_VERSION,
            case_id: "case".into(),
            text: "a b".into(),
            regions: vec![
                ferrodoc_foundry::TruthRegion {
                    id: "a".into(),
                    kind: BlockKind::Paragraph,
                    text: "a".into(),
                    rect: TruthRect {
                        x: 0.0,
                        y: 0.0,
                        width: 10.0,
                        height: 10.0,
                    },
                },
                ferrodoc_foundry::TruthRegion {
                    id: "b".into(),
                    kind: BlockKind::Paragraph,
                    text: "b".into(),
                    rect: TruthRect {
                        x: 0.0,
                        y: 0.0,
                        width: 10.0,
                        height: 10.0,
                    },
                },
            ],
            reading_order: Vec::new(),
            tables: Vec::new(),
            formulas: Vec::new(),
            provenance: ferrodoc_foundry::TruthProvenance {
                generator_version: "fixture".into(),
                seed: 0,
                degradation: Degradation::Native,
                assets: Vec::new(),
            },
        };
        let prediction = PredictionDocument {
            text: "a b".into(),
            regions: vec![PredictedRegion {
                id: "one".into(),
                kind: BlockKind::Paragraph,
                text: "a b".into(),
                rect: TruthRect {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
            }],
            reading_order: Vec::new(),
            tables: Vec::new(),
            formulas: Vec::new(),
        };
        let score = score_case(
            &truth,
            &prediction,
            Partition::Train,
            &MetricConfig::default(),
        )
        .unwrap();
        assert_eq!(score.regions.matched_regions, 1);
    }

    #[test]
    fn tables_and_formulas_need_spatial_association() {
        let (_directory, _root, _manifest, truth) = generated(Partition::Train);
        let mut prediction = prediction_from_truth(&truth);
        prediction.tables[0].region_id = "unassociated".into();
        prediction.formulas[0].region_id = "unassociated".into();
        let score = score_case(
            &truth,
            &prediction,
            Partition::Train,
            &MetricConfig::default(),
        )
        .unwrap();
        assert_eq!(score.tables.unwrap().semantic_similarity, 0.0);
        assert_eq!(score.formulas.unwrap().token_similarity, 0.0);
    }

    #[test]
    fn unicode_normalization_and_measurement_are_non_vacuous() {
        assert_eq!(string_similarity("e\u{301}", "é"), 1.0);
        let run = measure_repeated(3, || Ok::<_, ()>(42)).unwrap();
        assert_eq!(run.outputs, vec![42; 3]);
        let EvidenceValue::Measured { value, .. } = run.timing.warm_wall_ms else {
            panic!("warm wall timing must be measured")
        };
        assert_eq!(value.count, 2);
    }

    #[test]
    fn unknown_resources_serialize_as_unknown_not_zero() {
        let encoded = serde_json::to_string(&ResourceReport::default()).unwrap();
        assert!(encoded.contains("\"status\":\"unknown\""));
        assert!(!encoded.contains("peak_vram\":0"));
    }

    #[test]
    fn comparison_rejects_incompatible_corpus_and_metric_versions() {
        let (_directory, root, manifest, truth) = generated(Partition::Train);
        let baseline = evaluate(
            &root,
            &manifest,
            &predictions(&manifest, &truth, candidate("baseline")),
            &MetricConfig::default(),
        )
        .unwrap();
        let mut incompatible = baseline.clone();
        incompatible.metric_version = "other".into();
        assert!(compare(&baseline, &incompatible, ComparisonPolicy::default()).is_err());
        let mut incompatible = baseline.clone();
        incompatible.corpus_digest = Sha256Digest::of_bytes(b"other");
        assert!(compare(&baseline, &incompatible, ComparisonPolicy::default()).is_err());
        let mut incompatible = baseline.clone();
        incompatible
            .metric_config
            .geometry_thresholds
            .insert(BlockKind::Table, 0.9);
        assert!(compare(&baseline, &incompatible, ComparisonPolicy::default()).is_err());
    }

    #[test]
    fn report_validation_rejects_fabricated_aggregate_quality() {
        let (_directory, root, manifest, truth) = generated(Partition::Train);
        let mut report = evaluate(
            &root,
            &manifest,
            &predictions(&manifest, &truth, candidate("candidate")),
            &MetricConfig::default(),
        )
        .unwrap();
        report.aggregate.quality = Probability::new(0.5).unwrap();
        assert!(report.validate().is_err());
    }
}
