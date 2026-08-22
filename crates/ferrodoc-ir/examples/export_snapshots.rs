use std::{collections::BTreeMap, fs, path::PathBuf};

use ferrodoc_core::{
    CURRENT_SCHEMA_VERSION, CoordinateSpace, CoordinateTransform, DeterministicProvenance,
    DocumentId, EvidenceId, LayerId, ModelManifest, PageId, PageRect, Rect, RegionId, Sha256Digest,
    Stage, Unit,
};
use ferrodoc_ir::{
    DOCUMENT_STATE_SCHEMA, DeltaProducer, Document, DocumentMetadata, DocumentStateManifest,
    EVIDENCE_DELTA_SCHEMA, Evidence, EvidenceContent, EvidenceDelta, Page, RefinementScope, Region,
    RegionKind, SelectedView, SelectionReason, SourceLayer, SourceLayerKind,
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
    write_json(
        schema_dir.join("evidence-delta-v1.json"),
        &schema_for!(EvidenceDelta),
    )?;
    write_json(
        schema_dir.join("document-state-manifest-v1.json"),
        &schema_for!(DocumentStateManifest),
    )?;
    let document = sample_document();
    let delta = sample_delta(&document);
    let state = sample_state(&document, &delta);
    fs::write(
        fixture_dir.join("document-ir-v1.json"),
        document.to_canonical_json()?,
    )?;
    fs::write(
        fixture_dir.join("evidence-delta-v1.json"),
        delta.to_canonical_json()?,
    )?;
    fs::write(
        fixture_dir.join("document-state-manifest-v1.json"),
        state.to_canonical_json()?,
    )?;
    Ok(())
}

fn sample_delta(document: &Document) -> EvidenceDelta {
    EvidenceDelta {
        delta_schema: EVIDENCE_DELTA_SCHEMA.into(),
        source_pdf_sha256: document.input_digest,
        ir_schema: document.schema_version,
        stage: Stage::NativeExtract,
        producer: DeltaProducer {
            name: "ferrodoc-ir-golden".into(),
            version: "1.0.0".into(),
            build: Sha256Digest::of_bytes(b"ferrodoc-ir-golden-build"),
            model_digest: None,
            configuration_digest: Sha256Digest::of_bytes(b"ferrodoc-ir-golden-config"),
        },
        scope: RefinementScope::Document,
        input_state_id: None,
        required_evidence_ids: Default::default(),
        new_pages: document.pages.clone(),
        page_additions: Vec::new(),
        selection_hints: Vec::new(),
        diagnostics: Vec::new(),
        coverage_delta: Vec::new(),
    }
}

fn sample_state(document: &Document, delta: &EvidenceDelta) -> DocumentStateManifest {
    DocumentStateManifest {
        state_schema: DOCUMENT_STATE_SCHEMA.into(),
        source_pdf_sha256: document.input_digest,
        ir_schema: document.schema_version,
        evidence_delta_ids: [delta.id().unwrap()].into_iter().collect(),
        reconciliation_policy_id: Sha256Digest::of_bytes(b"golden-reconciliation-policy"),
        coverage: Vec::new(),
        materialized_ir_checkpoint: None,
        parent_state_ids: Default::default(),
    }
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
                    geometry_quality: ferrodoc_ir::GeometryQuality::Region,
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
