use std::collections::{BTreeMap, BTreeSet};

use ferrodoc_core::{
    BlobId, BlobRange, CURRENT_SCHEMA_VERSION, Capability, CoordinateSpace, CoordinateTransform,
    DeterministicProvenance, DocumentId, EvidenceId, LayerId, MediaType, PageId, PageRect, Profile,
    Rect, RegionId, RequestId, ScopedBlob, Sha256Digest, Stage, Unit,
};
use ferrodoc_engine_api::conformance::unknown_inventory;
use ferrodoc_ir::{
    CoverageEntry, DOCUMENT_STATE_SCHEMA, Document, DocumentMetadata, DocumentStateManifest,
    Evidence, EvidenceContent, GeometryQuality, Page, PageRegionRef, RefinementScope, Region,
    RegionKind, SourceLayer, SourceLayerKind,
};
use ferrodoc_runtime::{
    CacheDecision, ConversionOptions,
    cache::StageCache,
    enrichment::{
        CapabilityGoal, EnrichmentPlanningOutcome, EnrichmentRequest, EnrichmentRuntime,
        EnrichmentStageDescriptor,
    },
};
use ferrodoc_table_rulebased::RuleBasedTableEngine;

#[test]
fn targeted_table_refinement_retains_old_hypothesis_and_leaves_other_page_unchanged() {
    let (bytes, document, manifest, target, source_evidence_id) = fixture();
    let request = EnrichmentRequest {
        request_id: RequestId::derive(&[b"fp3-targeted-table"]),
        source: ScopedBlob {
            id: BlobId::new("source-pdf").unwrap(),
            range: BlobRange::new(0, bytes.len() as u64).unwrap(),
            media_type: MediaType::new("application/pdf").unwrap(),
            expected_digest: Some(Sha256Digest::of_bytes(&bytes)),
        },
        input_state_id: manifest.id().unwrap(),
        goals: vec![CapabilityGoal {
            capability: Capability::TableRecognize,
            scope: RefinementScope::Regions {
                regions: BTreeSet::from([target.clone()]),
            },
        }],
    };
    let unchanged_page = serde_json::to_vec(&document.pages[1]).unwrap();
    let mut enrichment_runtime = runtime();
    let plan = match enrichment_runtime
        .plan(&request, &document, &manifest)
        .unwrap()
    {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => {
            assert_eq!(pareto.len(), 1);
            pareto.remove(0)
        }
        other => panic!("unexpected planning outcome: {other:?}"),
    };
    assert_eq!(plan.invocations.len(), 1);
    assert_eq!(plan.invocations[0].scope, request.goals[0].scope);

    let result = enrichment_runtime
        .execute(&request, &plan, bytes.clone(), &document, &manifest)
        .unwrap();
    result.document.validate_evidence_grade().unwrap();
    assert_eq!(
        serde_json::to_vec(&result.document.pages[1]).unwrap(),
        unchanged_page
    );
    let region = &result.document.pages[0].regions[0];
    assert_eq!(region.evidence.len(), 2);
    assert_eq!(region.evidence[0].id, source_evidence_id);
    assert_eq!(
        region.selected.as_ref().unwrap().evidence_ids,
        vec![region.evidence[1].id.clone()]
    );
    let EvidenceContent::Table { cells, .. } = &region.evidence[1].content else {
        panic!("expected appended table hypothesis")
    };
    assert!(
        !region.evidence[1]
            .provenance
            .parameters
            .contains_key(ferrodoc_engine_api::SOURCE_TEXT_EVIDENCE_PARAMETER)
    );
    assert!(cells.iter().all(|cell| {
        cell.source_spans.len() == 1
            && cell.source_spans[0].evidence_id == source_evidence_id
            && cell.geometry_quality == GeometryQuality::Region
    }));
    assert_eq!(
        result.deltas[0].required_evidence_ids,
        BTreeSet::from([source_evidence_id.clone()])
    );
    assert_eq!(result.deltas[0].coverage_delta[0].status, "complete");

    let mut repeated_runtime = runtime();
    let repeated_plan = match repeated_runtime
        .plan(&request, &document, &manifest)
        .unwrap()
    {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        other => panic!("unexpected planning outcome: {other:?}"),
    };
    let repeated = repeated_runtime
        .execute(&request, &repeated_plan, bytes, &document, &manifest)
        .unwrap();
    assert_eq!(result.deltas, repeated.deltas);
    assert_eq!(
        result.document.to_canonical_json().unwrap(),
        repeated.document.to_canonical_json().unwrap()
    );
    assert_eq!(
        result.state_manifest.id().unwrap(),
        repeated.state_manifest.id().unwrap()
    );

    let cache_directory = tempfile::tempdir().unwrap();
    let cache = StageCache::open(cache_directory.path()).unwrap();
    let mut cold_runtime = runtime_with_cache(Some(cache.clone()));
    let cold_plan = match cold_runtime.plan(&request, &document, &manifest).unwrap() {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        other => panic!("unexpected planning outcome: {other:?}"),
    };
    let cold = cold_runtime
        .execute(
            &request,
            &cold_plan,
            b"%PDF-minimized-fp3-targeted-table".to_vec(),
            &document,
            &manifest,
        )
        .unwrap();
    assert_eq!(cold.resources.stages[0].cache, CacheDecision::Miss);
    let mut warm_runtime = runtime_with_cache(Some(cache));
    let warm_plan = match warm_runtime.plan(&request, &document, &manifest).unwrap() {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        other => panic!("unexpected planning outcome: {other:?}"),
    };
    let warm = warm_runtime
        .execute(
            &request,
            &warm_plan,
            b"%PDF-minimized-fp3-targeted-table".to_vec(),
            &document,
            &manifest,
        )
        .unwrap();
    assert_eq!(warm.resources.stages[0].cache, CacheDecision::Hit);
    assert_eq!(cold.deltas, warm.deltas);
}

fn runtime() -> EnrichmentRuntime {
    runtime_with_cache(None)
}

fn runtime_with_cache(cache: Option<StageCache>) -> EnrichmentRuntime {
    let mut runtime = EnrichmentRuntime::new(
        ConversionOptions {
            profile: Profile::Offline,
            ..ConversionOptions::default()
        },
        unknown_inventory(),
        cache,
    );
    runtime
        .register_stage(
            EnrichmentStageDescriptor {
                id: "table.structure.rulebased".into(),
                stage: Stage::Layout,
                build: Sha256Digest::of_bytes(b"table-rulebased-build-v1"),
                model_digest: None,
                parameters: BTreeMap::new(),
                produces: Capability::TableRecognize,
                requires: BTreeSet::from([Capability::LayoutDetect]),
            },
            RuleBasedTableEngine::new(),
        )
        .unwrap();
    runtime
}

fn fixture() -> (
    Vec<u8>,
    Document,
    DocumentStateManifest,
    PageRegionRef,
    EvidenceId,
) {
    let bytes = b"%PDF-minimized-fp3-targeted-table".to_vec();
    let digest = Sha256Digest::of_bytes(&bytes);
    let document_id = DocumentId::derive(&[digest.as_bytes()]);
    let mut target = None;
    let mut source_evidence_id = None;
    let pages = [
        "Pin | Name | Voltage\n1 | VCC | 3.3 V\n2 | GND | 0 V",
        "This page must not be recomputed.",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, text)| {
        let index = index as u32;
        let page_id = PageId::derive(&[document_id.as_str().as_bytes(), &index.to_be_bytes()]);
        let region_id = RegionId::derive(&[page_id.as_str().as_bytes(), b"region"]);
        let layer_id = LayerId::derive(&[page_id.as_str().as_bytes(), b"native-text"]);
        let evidence_id = EvidenceId::derive(&[page_id.as_str().as_bytes(), text.as_bytes()]);
        let bounds = PageRect {
            page_index: index,
            rect: Rect::new(0.0, 0.0, 612.0, 792.0, CoordinateSpace::Pdf, Unit::Point).unwrap(),
            source_transform: CoordinateTransform::IDENTITY,
        };
        let geometry = PageRect {
            page_index: index,
            rect: Rect::new(36.0, 72.0, 420.0, 120.0, CoordinateSpace::Pdf, Unit::Point).unwrap(),
            source_transform: CoordinateTransform::IDENTITY,
        };
        let provenance = DeterministicProvenance {
            schema_version: CURRENT_SCHEMA_VERSION,
            input_digest: digest,
            engine_id: "native.pdf".into(),
            engine_version: "1.0.0".into(),
            model_digest: None,
            parameters: BTreeMap::new(),
            stage: Stage::NativeExtract,
        };
        if index == 0 {
            target = Some(PageRegionRef {
                page_id: page_id.clone(),
                region_id: region_id.clone(),
            });
            source_evidence_id = Some(evidence_id.clone());
        }
        Page {
            id: page_id,
            index,
            bounds,
            layers: vec![SourceLayer {
                id: layer_id.clone(),
                kind: SourceLayerKind::NativePdf,
                provenance: provenance.clone(),
            }],
            artifacts: Vec::new(),
            regions: vec![Region {
                id: region_id,
                kind: if index == 0 {
                    RegionKind::Table
                } else {
                    RegionKind::Paragraph
                },
                geometry,
                evidence: vec![Evidence {
                    id: evidence_id,
                    layer_id,
                    content: EvidenceContent::Text { text: text.into() },
                    geometry: Some(geometry),
                    geometry_quality: GeometryQuality::Region,
                    confidence: None,
                    provenance,
                    engine_metadata: BTreeMap::new(),
                }],
                selected: None,
            }],
            reading_order: Vec::new(),
        }
    })
    .collect();
    let document = Document {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: document_id,
        input_digest: digest,
        metadata: DocumentMetadata::default(),
        pages,
    };
    document.validate_evidence_grade().unwrap();
    let target = target.unwrap();
    let manifest = DocumentStateManifest {
        state_schema: DOCUMENT_STATE_SCHEMA.into(),
        source_pdf_sha256: digest,
        ir_schema: CURRENT_SCHEMA_VERSION,
        evidence_delta_ids: BTreeSet::new(),
        reconciliation_policy_id: Sha256Digest::of_bytes(b"fp3-policy"),
        coverage: vec![CoverageEntry {
            capability: Capability::LayoutDetect,
            scope: RefinementScope::Regions {
                regions: BTreeSet::from([target.clone()]),
            },
            status: "complete".into(),
        }],
        materialized_ir_checkpoint: None,
        parent_state_ids: BTreeSet::new(),
    };
    (
        bytes,
        document,
        manifest,
        target,
        source_evidence_id.unwrap(),
    )
}
