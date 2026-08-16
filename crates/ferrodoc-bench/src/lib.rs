//! Binary-test benchmark harness inspired by unit-test based OCR evaluation.

use std::{collections::BTreeMap, fs, path::Path, time::Duration};

use anyhow::{Context, Result};
use ferrodoc_core::RegionKind;
use ferrodoc_foundry::{PageTruth, TruthAssertion};
use ferrodoc_ir::{Document, EvidenceContent};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExtractionArtifact {
    pub markdown: String,
    pub document: Option<Document>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AssertionResult {
    pub assertion: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaseReport {
    pub id: String,
    pub passed: usize,
    pub total: usize,
    pub score: f32,
    pub duration_ms: u64,
    #[serde(default)]
    pub assertions: Vec<AssertionResult>,
    #[serde(default)]
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SuiteReport {
    pub name: String,
    pub cases: Vec<CaseReport>,
    pub score: f32,
    pub assertions_passed: usize,
    pub assertions_total: usize,
    pub wall_time_ms: u64,
    pub pages_per_second: f64,
    #[serde(default)]
    pub metrics: BTreeMap<String, f64>,
}

pub fn evaluate_case(truth: &PageTruth, artifact: &ExtractionArtifact, duration: Duration) -> CaseReport {
    let normalized = normalize_text(&artifact.markdown);
    let mut results = Vec::new();
    for assertion in &truth.assertions {
        let (passed, detail) = evaluate_assertion(assertion, truth, artifact, &normalized);
        results.push(AssertionResult { assertion: describe_assertion(assertion), passed, detail });
    }
    let passed = results.iter().filter(|x| x.passed).count();
    let total = results.len();
    CaseReport {
        id: truth.id.clone(),
        passed,
        total,
        score: if total == 0 { 1.0 } else { passed as f32 / total as f32 },
        duration_ms: duration.as_millis() as u64,
        assertions: results,
        metrics: BTreeMap::new(),
    }
}

fn evaluate_assertion(assertion: &TruthAssertion, truth: &PageTruth, artifact: &ExtractionArtifact, normalized: &str) -> (bool, String) {
    match assertion {
        TruthAssertion::ContainsText { text } => {
            let needle = normalize_text(text);
            (normalized.contains(&needle), format!("contains {text:?}"))
        }
        TruthAssertion::ExcludesText { text } => {
            let needle = normalize_text(text);
            (!normalized.contains(&needle), format!("excludes {text:?}"))
        }
        TruthAssertion::ReadingOrder { before, after } => {
            let a = truth.regions.iter().find(|r| &r.id == before).and_then(|r| r.text.as_deref()).map(normalize_text);
            let b = truth.regions.iter().find(|r| &r.id == after).and_then(|r| r.text.as_deref()).map(normalize_text);
            match (a, b) {
                (Some(a), Some(b)) => {
                    let ai = first_fuzzy_position(normalized, &a);
                    let bi = first_fuzzy_position(normalized, &b);
                    (ai.is_some() && bi.is_some() && ai < bi, format!("positions: before={ai:?}, after={bi:?}"))
                }
                _ => (false, "truth region lacks text".into()),
            }
        }
        TruthAssertion::RegionKind { region, kind } => {
            let Some(document) = artifact.document.as_ref() else { return (false, "no structured document output".into()) };
            let Some(truth_region) = truth.regions.iter().find(|r| &r.id == region) else { return (false, "truth region missing".into()) };
            let best = document.pages.iter().flat_map(|p| p.regions.iter()).map(|r| (r.bbox.iou(truth_region.bbox), r)).max_by(|a, b| a.0.total_cmp(&b.0));
            match best {
                Some((iou, r)) => (iou >= 0.18 && r.kind == *kind, format!("best iou={iou:.3}, got={:?}, expected={kind:?}", r.kind)),
                None => (false, "no output regions".into()),
            }
        }
        TruthAssertion::TableShape { region, rows, columns } => {
            let Some(document) = artifact.document.as_ref() else { return (false, "no structured document output".into()) };
            let Some(truth_region) = truth.regions.iter().find(|r| &r.id == region) else { return (false, "truth region missing".into()) };
            let candidate = document.pages.iter().flat_map(|p| p.regions.iter()).filter(|r| r.kind == RegionKind::Table).max_by(|a, b| a.bbox.iou(truth_region.bbox).total_cmp(&b.bbox.iou(truth_region.bbox)));
            let shape = candidate.and_then(|r| r.selected().or_else(|| r.best_textual_evidence())).and_then(|e| match &e.content { EvidenceContent::Table { table } => Some((table.rows, table.columns)), _ => None });
            (shape == Some((*rows, *columns)), format!("got={shape:?}, expected=({rows},{columns})"))
        }
        TruthAssertion::Formula { region, latex } => {
            let Some(document) = artifact.document.as_ref() else {
                let expected = normalize_math(latex);
                let got = normalize_math(&artifact.markdown);
                return (got.contains(&expected), "formula checked against markdown".into());
            };
            let Some(truth_region) = truth.regions.iter().find(|r| &r.id == region) else { return (false, "truth region missing".into()) };
            let candidate = document.pages.iter().flat_map(|p| p.regions.iter()).filter(|r| r.kind == RegionKind::Equation).max_by(|a, b| a.bbox.iou(truth_region.bbox).total_cmp(&b.bbox.iou(truth_region.bbox)));
            let got = candidate.and_then(|r| r.selected().or_else(|| r.best_textual_evidence())).and_then(|e| match &e.content { EvidenceContent::Latex { latex } => Some(latex.as_str()), EvidenceContent::Text { text } => Some(text.as_str()), _ => None });
            let passed = got.map(|g| normalize_math(g) == normalize_math(latex) || normalize_math(g).contains(&normalize_math(latex))).unwrap_or(false);
            (passed, format!("got={got:?}"))
        }
    }
}

fn describe_assertion(a: &TruthAssertion) -> String {
    match a {
        TruthAssertion::ContainsText { text } => format!("contains:{text}"),
        TruthAssertion::ExcludesText { text } => format!("excludes:{text}"),
        TruthAssertion::ReadingOrder { before, after } => format!("order:{before}<{after}"),
        TruthAssertion::RegionKind { region, kind } => format!("kind:{region}={kind:?}"),
        TruthAssertion::TableShape { region, rows, columns } => format!("table:{region}={rows}x{columns}"),
        TruthAssertion::Formula { region, .. } => format!("formula:{region}"),
    }
}

pub fn suite_report(name: impl Into<String>, cases: Vec<CaseReport>) -> SuiteReport {
    let assertions_passed = cases.iter().map(|x| x.passed).sum();
    let assertions_total = cases.iter().map(|x| x.total).sum();
    let wall_time_ms: u64 = cases.iter().map(|x| x.duration_ms).sum();
    let pages_per_second = if wall_time_ms == 0 { 0.0 } else { cases.len() as f64 / (wall_time_ms as f64 / 1000.0) };
    SuiteReport {
        name: name.into(),
        score: if assertions_total == 0 { 1.0 } else { assertions_passed as f32 / assertions_total as f32 },
        cases,
        assertions_passed,
        assertions_total,
        wall_time_ms,
        pages_per_second,
        metrics: BTreeMap::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareReport {
    pub baseline: String,
    pub candidate: String,
    pub quality_delta: f32,
    pub throughput_delta: f64,
    pub regressions: Vec<String>,
    pub improvements: Vec<String>,
    pub pareto_dominates: bool,
}

pub fn compare_reports(baseline: &SuiteReport, candidate: &SuiteReport) -> CompareReport {
    let base_cases: BTreeMap<_, _> = baseline.cases.iter().map(|c| (&c.id, c)).collect();
    let mut regressions = Vec::new(); let mut improvements = Vec::new();
    for c in &candidate.cases {
        if let Some(b) = base_cases.get(&c.id) {
            let delta = c.score - b.score;
            if delta < -0.001 { regressions.push(format!("{}: {:+.3}", c.id, delta)); }
            if delta > 0.001 { improvements.push(format!("{}: {:+.3}", c.id, delta)); }
        }
    }
    let quality_delta = candidate.score - baseline.score;
    let throughput_delta = candidate.pages_per_second - baseline.pages_per_second;
    CompareReport {
        baseline: baseline.name.clone(), candidate: candidate.name.clone(), quality_delta, throughput_delta,
        pareto_dominates: quality_delta >= 0.0 && throughput_delta >= 0.0 && (quality_delta > 0.0 || throughput_delta > 0.0),
        regressions, improvements,
    }
}

pub fn load_truth(path: impl AsRef<Path>) -> Result<PageTruth> {
    serde_json::from_slice(&fs::read(path.as_ref()).with_context(|| format!("read truth {}", path.as_ref().display()))?).context("parse truth")
}

pub fn save_report(path: impl AsRef<Path>, report: &SuiteReport) -> Result<()> { fs::write(path, serde_json::to_vec_pretty(report)?)?; Ok(()) }
pub fn load_report(path: impl AsRef<Path>) -> Result<SuiteReport> { Ok(serde_json::from_slice(&fs::read(path)?)?) }

fn normalize_text(s: &str) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase() }
fn normalize_math(s: &str) -> String { s.chars().filter(|c| !c.is_whitespace() && !matches!(c, '$' | '`')).collect::<String>().to_lowercase() }
fn first_fuzzy_position(haystack: &str, needle: &str) -> Option<usize> {
    if let Some(pos) = haystack.find(needle) { return Some(pos); }
    // Long paragraphs only need a stable identifying prefix for order tests.
    let prefix: String = needle.chars().take(48).collect();
    haystack.find(&prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compare_detects_pareto_win() {
        let b = suite_report("b", vec![CaseReport { id: "x".into(), passed: 8, total: 10, score: .8, duration_ms: 1000, assertions: vec![], metrics: BTreeMap::new() }]);
        let c = suite_report("c", vec![CaseReport { id: "x".into(), passed: 9, total: 10, score: .9, duration_ms: 900, assertions: vec![], metrics: BTreeMap::new() }]);
        assert!(compare_reports(&b, &c).pareto_dominates);
    }
}
