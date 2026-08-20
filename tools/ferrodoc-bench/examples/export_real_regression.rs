//! Regenerates the checked, purpose-built real-PDF regression corpus metadata.

use std::{error::Error, fs, path::Path};

use ferrodoc_core::{CURRENT_SCHEMA_VERSION, Sha256Digest};
use ferrodoc_foundry::{
    AssetSpec, BlockKind, CorpusCase, CorpusManifest, Degradation, Partition, RedistributionStatus,
    TruthDocument, TruthProvenance, TruthRect, TruthRegion,
};

const GENERATOR_VERSION: &str = "ferrodoc-real-regression/1";

fn main() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = root.join("benchmarks/real-regression");
    fs::create_dir_all(output.join("truth"))?;
    let generator_path = root.join("crates/ferrodoc-pdf/examples/generate_fixtures.rs");
    let assets = vec![
        AssetSpec {
            id: "fixture-generator-source".into(),
            license: "MIT OR Apache-2.0".into(),
            source: "crates/ferrodoc-pdf/examples/generate_fixtures.rs".into(),
            redistribution: RedistributionStatus::Redistributable,
            digest: Some(Sha256Digest::of_file(&generator_path)?),
        },
        AssetSpec {
            id: "pdf-standard-font-helvetica".into(),
            license: "ISO-32000-built-in".into(),
            source: "PDF standard Type1 Helvetica".into(),
            redistribution: RedistributionStatus::BuiltIn,
            digest: None,
        },
    ];
    let definitions = [
        Definition {
            name: "born-digital",
            document: "fixtures/pdf/born-digital.pdf",
            regions: vec![
                region(
                    "heading",
                    BlockKind::Heading,
                    "FERRODOC FIXTURE HEADING",
                    72.0,
                    754.0,
                    330.0,
                    30.0,
                ),
                region(
                    "paragraph-1",
                    BlockKind::Paragraph,
                    "First paragraph appears before the second paragraph.",
                    72.0,
                    704.0,
                    390.0,
                    18.0,
                ),
                region(
                    "paragraph-2",
                    BlockKind::Paragraph,
                    "Second paragraph confirms deterministic reading order.",
                    72.0,
                    674.0,
                    410.0,
                    18.0,
                ),
            ],
        },
        Definition {
            name: "image-only",
            document: "fixtures/pdf/image-only.pdf",
            regions: vec![
                region(
                    "heading",
                    BlockKind::Heading,
                    "SCANNED FERRODOC PAGE",
                    70.0,
                    522.0,
                    365.0,
                    36.0,
                ),
                region(
                    "paragraph-1",
                    BlockKind::Paragraph,
                    "Optical text survives the CPU path.",
                    70.0,
                    474.0,
                    340.0,
                    24.0,
                ),
            ],
        },
    ];
    let spec_digest = Sha256Digest::of_bytes(&serde_json::to_vec(&(
        GENERATOR_VERSION,
        &assets,
        &definitions,
    ))?);
    let mut cases = Vec::new();
    for definition in definitions {
        let case_identity = Sha256Digest::of_bytes(&serde_json::to_vec(&definition)?);
        let case_id = format!(
            "real_{}_{}",
            definition.name.replace('-', "_"),
            &case_identity.to_string()[..12]
        );
        let text = definition
            .regions
            .iter()
            .map(|region| region.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let reading_order = definition
            .regions
            .windows(2)
            .map(|regions| [regions[0].id.clone(), regions[1].id.clone()])
            .collect();
        let truth = TruthDocument {
            schema_version: CURRENT_SCHEMA_VERSION,
            case_id: case_id.clone(),
            text,
            regions: definition.regions,
            reading_order,
            tables: Vec::new(),
            formulas: Vec::new(),
            provenance: TruthProvenance {
                generator_version: GENERATOR_VERSION.into(),
                seed: 0,
                degradation: if definition.name == "image-only" {
                    Degradation::Scan {
                        dpi: 144,
                        noise_per_mille: 0,
                    }
                } else {
                    Degradation::Native
                },
                assets: assets.iter().map(|asset| asset.id.clone()).collect(),
            },
        };
        let truth_relative = format!("benchmarks/real-regression/truth/{case_id}.json");
        let truth_bytes = pretty(&truth)?;
        fs::write(root.join(&truth_relative), &truth_bytes)?;
        let document_bytes = fs::read(root.join(definition.document))?;
        cases.push(CorpusCase {
            id: case_id,
            case_identity,
            partition: Partition::Regression,
            document: definition.document.into(),
            truth: truth_relative,
            document_digest: Sha256Digest::of_bytes(&document_bytes),
            truth_digest: Sha256Digest::of_bytes(&truth_bytes),
            categories: truth.regions.iter().map(|region| region.kind).collect(),
        });
    }
    let corpus_digest = Sha256Digest::of_bytes(&serde_json::to_vec(&(
        GENERATOR_VERSION,
        spec_digest,
        0_u64,
        &assets,
        &cases,
    ))?);
    let manifest = CorpusManifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        generator_version: GENERATOR_VERSION.into(),
        spec_digest,
        seed: 0,
        assets,
        cases,
        corpus_digest,
    };
    fs::write(output.join("manifest.json"), pretty(&manifest)?)?;
    Ok(())
}

#[derive(serde::Serialize)]
struct Definition {
    name: &'static str,
    document: &'static str,
    regions: Vec<TruthRegion>,
}

fn region(
    id: &str,
    kind: BlockKind,
    text: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> TruthRegion {
    TruthRegion {
        id: id.into(),
        kind,
        text: text.into(),
        rect: TruthRect {
            x,
            y,
            width,
            height,
        },
    }
}

fn pretty(value: &impl serde::Serialize) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
