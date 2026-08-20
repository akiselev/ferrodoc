use std::collections::BTreeMap;

use ferrodoc_core::{
    CoordinateSpace, CoordinateTransform, DeterministicProvenance, PageRect, Rect, Stage, Unit,
};

use super::*;

fn provenance(input_digest: Sha256Digest, stage: Stage, engine: &str) -> DeterministicProvenance {
    DeterministicProvenance {
        schema_version: CURRENT_SCHEMA_VERSION,
        input_digest,
        engine_id: engine.into(),
        engine_version: "1.0.0".into(),
        model_digest: None,
        parameters: BTreeMap::new(),
        stage,
    }
}

fn fixture() -> Document {
    let input_digest = Sha256Digest::of_bytes(b"fixture-pdf");
    let document_id = DocumentId::derive(&[input_digest.as_bytes()]);
    let page_id = PageId::derive(&[document_id.as_str().as_bytes(), &0_u32.to_be_bytes()]);
    let native_layer = LayerId::derive(&[page_id.as_str().as_bytes(), b"native"]);
    let ocr_layer = LayerId::derive(&[page_id.as_str().as_bytes(), b"ocr"]);
    let region_id = RegionId::derive(&[page_id.as_str().as_bytes(), b"region-0"]);
    let native_evidence = EvidenceId::derive(&[region_id.as_str().as_bytes(), b"native"]);
    let ocr_evidence = EvidenceId::derive(&[region_id.as_str().as_bytes(), b"ocr"]);
    let bounds = Rect::new(0.0, 0.0, 612.0, 792.0, CoordinateSpace::Pdf, Unit::Point).unwrap();
    let geometry = PageRect {
        page_index: 0,
        rect: Rect::new(72.0, 72.0, 200.0, 24.0, CoordinateSpace::Pdf, Unit::Point).unwrap(),
        source_transform: CoordinateTransform::IDENTITY,
    };
    Document {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: document_id,
        input_digest,
        metadata: DocumentMetadata {
            title: Some("Evidence fixture".into()),
            authors: vec!["Ferrodoc".into()],
            language: Some("en".into()),
            extra: BTreeMap::from([
                ("a".into(), serde_json::json!(1)),
                ("z".into(), serde_json::json!(2)),
            ]),
        },
        pages: vec![Page {
            id: page_id,
            index: 0,
            bounds: PageRect {
                page_index: 0,
                rect: bounds,
                source_transform: CoordinateTransform::IDENTITY,
            },
            layers: vec![
                SourceLayer {
                    id: native_layer.clone(),
                    kind: SourceLayerKind::NativePdf,
                    provenance: provenance(input_digest, Stage::NativeExtract, "native-pdf"),
                },
                SourceLayer {
                    id: ocr_layer.clone(),
                    kind: SourceLayerKind::Ocr,
                    provenance: provenance(input_digest, Stage::Ocr, "mock-ocr"),
                },
            ],
            artifacts: Vec::new(),
            regions: vec![Region {
                id: region_id,
                kind: RegionKind::Heading,
                geometry,
                evidence: vec![
                    Evidence {
                        id: native_evidence.clone(),
                        layer_id: native_layer,
                        content: EvidenceContent::Text {
                            text: "Native title".into(),
                        },
                        geometry: Some(geometry),
                        confidence: None,
                        provenance: provenance(input_digest, Stage::NativeExtract, "native-pdf"),
                        engine_metadata: BTreeMap::new(),
                    },
                    Evidence {
                        id: ocr_evidence,
                        layer_id: ocr_layer,
                        content: EvidenceContent::Text {
                            text: "OCR title".into(),
                        },
                        geometry: Some(geometry),
                        confidence: Some(Probability::new(0.8).unwrap()),
                        provenance: provenance(input_digest, Stage::Ocr, "mock-ocr"),
                        engine_metadata: BTreeMap::new(),
                    },
                ],
                selected: Some(SelectedView {
                    evidence_ids: vec![native_evidence],
                    reason: SelectionReason::NativeQuality,
                    explanation: "native evidence passed the threshold".into(),
                }),
            }],
            reading_order: Vec::new(),
        }],
    }
}

#[test]
fn native_and_ocr_evidence_remain_distinct() {
    let document = fixture();
    document.validate().unwrap();
    let region = &document.pages[0].regions[0];
    assert_eq!(region.evidence.len(), 2);
    assert_ne!(region.evidence[0].layer_id, region.evidence[1].layer_id);
    assert_eq!(region.selected.as_ref().unwrap().evidence_ids.len(), 1);
}

#[test]
fn canonical_json_is_byte_deterministic() {
    let document = fixture();
    let first = document.to_canonical_json().unwrap();
    let decoded: Document = serde_json::from_slice(&first).unwrap();
    let second = decoded.to_canonical_json().unwrap();
    assert_eq!(first, second);
}

#[test]
fn additive_unknown_fields_are_ignored_within_a_major_version() {
    let document = fixture();
    let mut value = serde_json::to_value(document).unwrap();
    value.as_object_mut().unwrap().insert(
        "future_field".into(),
        serde_json::json!({"retained_by_newer_writer": true}),
    );
    let decoded: Document = serde_json::from_value(value).unwrap();
    decoded.validate().unwrap();
}

#[test]
fn reading_order_cycles_are_rejected() {
    let mut document = fixture();
    let page = &mut document.pages[0];
    let first = page.regions[0].id.clone();
    let second = RegionId::derive(&[page.id.as_str().as_bytes(), b"region-1"]);
    let mut region = page.regions[0].clone();
    region.id = second.clone();
    region.evidence.clear();
    region.selected = None;
    page.regions.push(region);
    page.reading_order = vec![
        ReadingOrderEdge {
            before: first.clone(),
            after: second.clone(),
        },
        ReadingOrderEdge {
            before: second,
            after: first,
        },
    ];
    assert!(document.validate().is_err());
}

#[test]
fn invalid_selected_reference_is_rejected() {
    let mut document = fixture();
    document.pages[0].regions[0]
        .selected
        .as_mut()
        .unwrap()
        .evidence_ids = vec![EvidenceId::derive(&[b"missing"])];
    assert!(document.validate().is_err());
}

#[test]
fn checked_in_ir_fixture_reserializes_byte_for_byte() {
    let bytes = include_bytes!("../../../fixtures/document-ir-v1.json");
    let document: Document = serde_json::from_slice(bytes).unwrap();
    assert_eq!(document.to_canonical_json().unwrap(), bytes);
}

#[test]
fn schema_snapshots_match_public_contracts() {
    let expected_ir: serde_json::Value =
        serde_json::from_str(include_str!("../../../schemas/document-ir-v1.json")).unwrap();
    let expected_manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../schemas/model-manifest-v1.json")).unwrap();
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(Document)).unwrap(),
        expected_ir
    );
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(ferrodoc_core::ModelManifest)).unwrap(),
        expected_manifest
    );
}
