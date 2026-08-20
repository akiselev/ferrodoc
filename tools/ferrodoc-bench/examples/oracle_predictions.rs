//! Produces evaluator self-test predictions directly from visible truth.
//!
//! This deliberately refuses held-out cases and is not an engine benchmark.

use std::{error::Error, fs, path::PathBuf};

use ferrodoc_bench::{
    CandidateIdentity, CasePrediction, PredictedRegion, PredictionDocument, PredictionOutcome,
    PredictionSet, ResourceReport,
};
use ferrodoc_core::{CURRENT_SCHEMA_VERSION, Sha256Digest};
use ferrodoc_foundry::{CorpusManifest, Partition, TruthDocument};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [corpus_root, manifest_path, output] = arguments.as_slice() else {
        return Err(
            "usage: oracle_predictions <corpus-root> <manifest.json> <predictions.json>".into(),
        );
    };
    let corpus_root = PathBuf::from(corpus_root);
    let manifest: CorpusManifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
    if manifest
        .cases
        .iter()
        .any(|case| case.partition == Partition::HeldOut)
    {
        return Err("oracle self-test refuses held-out truth".into());
    }

    let cases = manifest
        .cases
        .iter()
        .map(|case| -> Result<_, Box<dyn Error>> {
            let truth: TruthDocument =
                serde_json::from_slice(&fs::read(corpus_root.join(&case.truth))?)?;
            let prediction = PredictionDocument {
                text: truth.text,
                regions: truth
                    .regions
                    .into_iter()
                    .map(|region| PredictedRegion {
                        id: region.id,
                        kind: region.kind,
                        text: region.text,
                        rect: region.rect,
                    })
                    .collect(),
                reading_order: truth.reading_order,
                tables: truth.tables,
                formulas: truth.formulas,
            };
            Ok(CasePrediction {
                case_id: case.id.clone(),
                outcome: PredictionOutcome::Success { prediction },
                timing: None,
                resources: ResourceReport::default(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let predictions = PredictionSet {
        schema_version: CURRENT_SCHEMA_VERSION,
        corpus_digest: manifest.corpus_digest,
        candidate: CandidateIdentity {
            engine_id: "evaluator.oracle-self-test".into(),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            model_digest: None,
            configuration_digest: Sha256Digest::of_bytes(b"visible-truth-oracle-v1"),
            toolchain: format!("rust-{}", env!("CARGO_PKG_RUST_VERSION")),
        },
        cases,
    };
    let mut bytes = serde_json::to_vec_pretty(&predictions)?;
    bytes.push(b'\n');
    fs::write(output, bytes)?;
    Ok(())
}
