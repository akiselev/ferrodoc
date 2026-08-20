use std::{collections::BTreeMap, fs, path::PathBuf};

use ferrodoc_core::{
    CURRENT_SCHEMA_VERSION, CoordinateSpace, CoordinateTransform, DeterministicProvenance,
    DocumentId, EvidenceId, LayerId, ModelManifest, PageId, PageRect, Rect, RegionId, Sha256Digest,
    Stage, Unit,
};
use ferrodoc_ir::{
    Document, DocumentMetadata, Evidence, EvidenceContent, Page, Region, RegionKind, SelectedView,
    SelectionReason, SourceLayer, SourceLayerKind,
};
use schemars::schema_for;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema_dir = root.join("schemas");
    let fixture_dir = root.join("fixtures");
    fs::create_dir_all(&schema_dir)?;
    fs::create_dir_all(&fixture_dir)?;
    write_json(
        schema_dir.join("model-manifest-v1.json"),
        &schema_for!(ModelManifest),
    )?;
    write_json(
        schema_dir.join("document-ir-v1.json"),
        &schema_for!(Document),
    )?;
    fs::write(
        fixture_dir.join("document-ir-v1.json"),
        sample_document().to_canonical_json()?,
    )?;
    Ok(())
}

fn write_json(
    path: PathBuf,
    value: &impl serde::Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

fn sample_document() -> Document {
    let digest = Sha256Digest::of_bytes(b"phase-1-golden-document");
    let document_id = DocumentId::derive(&[digest.as_bytes()]);
    let page_id = PageId::derive(&[document_id.as_str().as_bytes(), &0_u32.to_be_bytes()]);
    let layer_id = LayerId::derive(&[page_id.as_str().as_bytes(), b"native"]);
    let region_id = RegionId::derive(&[page_id.as_str().as_bytes(), b"heading"]);
    let evidence_id = EvidenceId::derive(&[region_id.as_str().as_bytes(), b"native"]);
    let provenance = DeterministicProvenance {
        schema_version: CURRENT_SCHEMA_VERSION,
        input_digest: digest,
        engine_id: "native-pdf".into(),
        engine_version: "0.2.0".into(),
        model_digest: None,
        parameters: BTreeMap::new(),
        stage: Stage::NativeExtract,
    };
    let page_bounds = PageRect {
        page_index: 0,
        rect: Rect::new(0.0, 0.0, 612.0, 792.0, CoordinateSpace::Pdf, Unit::Point).unwrap(),
        source_transform: CoordinateTransform::IDENTITY,
    };
    let heading_bounds = PageRect {
        page_index: 0,
        rect: Rect::new(72.0, 72.0, 240.0, 24.0, CoordinateSpace::Pdf, Unit::Point).unwrap(),
        source_transform: CoordinateTransform::IDENTITY,
    };
    Document {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: document_id,
        input_digest: digest,
        metadata: DocumentMetadata {
            title: Some("Ferrodoc evidence fixture".into()),
            authors: vec!["Ferrodoc contributors".into()],
            language: Some("en".into()),
            extra: BTreeMap::new(),
        },
        pages: vec![Page {
            id: page_id,
            index: 0,
            bounds: page_bounds,
            layers: vec![SourceLayer {
                id: layer_id.clone(),
                kind: SourceLayerKind::NativePdf,
                provenance: provenance.clone(),
            }],
            artifacts: Vec::new(),
            regions: vec![Region {
                id: region_id,
                kind: RegionKind::Heading,
                geometry: heading_bounds,
                evidence: vec![Evidence {
                    id: evidence_id.clone(),
                    layer_id,
                    content: EvidenceContent::Text {
                        text: "Ferrodoc evidence fixture".into(),
                    },
                    geometry: Some(heading_bounds),
                    confidence: None,
                    provenance,
                    engine_metadata: BTreeMap::new(),
                }],
                selected: Some(SelectedView {
                    evidence_ids: vec![evidence_id],
                    reason: SelectionReason::NativeQuality,
                    explanation: "native evidence passed the deterministic threshold".into(),
                }),
            }],
            reading_order: Vec::new(),
        }],
    }
}
