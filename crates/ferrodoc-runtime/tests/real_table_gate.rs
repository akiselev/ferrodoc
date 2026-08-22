//! Optional rights-reviewed real-document qualification for the bounded FLS5 table gate.
//!
//! The PDF is deliberately not a repository fixture. Set `FERRODOC_TEST_RP2040_PDF` to the
//! retained exact artifact named by `fixtures/table/rp2040-table1-oracle-v1.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::time::Instant;

use ferrodoc_core::{
    BlobId, BlobRange, Capability, MediaType, Profile, RequestId, ScopedBlob, Sha256Digest, Stage,
};
use ferrodoc_engine_api::conformance::unknown_inventory;
use ferrodoc_ir::{
    CoverageEntry, DOCUMENT_STATE_SCHEMA, DocumentStateManifest, EvidenceContent, GeometryQuality,
    PageRegionRef, RefinementScope,
};
use ferrodoc_runtime::{
    ConversionOptions, Converter,
    enrichment::{
        CapabilityGoal, EnrichmentPlanningOutcome, EnrichmentRequest, EnrichmentRuntime,
        EnrichmentStageDescriptor,
    },
};
use ferrodoc_table_rulebased::RuleBasedTableEngine;

#[test]
fn rp2040_table1_fragment_has_evidence_bearing_cells_when_pdf_is_provided() {
    let Some(path) = std::env::var_os("FERRODOC_TEST_RP2040_PDF") else {
        return;
    };
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/table/rp2040-table1-oracle-v1.json"
    ))
    .unwrap();
    let bytes = fs::read(path).unwrap();
    let digest = Sha256Digest::of_bytes(&bytes);
    assert_eq!(digest.to_string(), oracle["source_pdf_sha256"]);
    assert_eq!(bytes.len() as u64, oracle["source_pdf_bytes"]);

    let mut converter = Converter::new(ConversionOptions {
        native_character_threshold: 0,
        profile: Profile::Offline,
        ..ConversionOptions::default()
    });
    let conversion_started = Instant::now();
    let conversion = converter.convert(bytes.clone()).unwrap();
    let conversion_millis = conversion_started.elapsed().as_millis();
    let page_index = oracle["page_index"].as_u64().unwrap() as u32;
    let page = &conversion.document.pages[page_index as usize];
    assert_eq!(page.id.as_str(), oracle["page_id"]);
    let region = page
        .regions
        .iter()
        .find(|region| region.id.as_str() == oracle["region_id"])
        .unwrap();
    let target = PageRegionRef {
        page_id: page.id.clone(),
        region_id: region.id.clone(),
    };
    let source = region
        .evidence
        .iter()
        .find(|evidence| evidence.id.as_str() == oracle["source_evidence_id"])
        .unwrap();
    let EvidenceContent::Text { text } = &source.content else {
        panic!("oracle source evidence is not text")
    };
    assert_eq!(
        Sha256Digest::of_bytes(text.as_bytes()).to_string(),
        oracle["source_text_sha256"]
    );
    assert_eq!(source.geometry, Some(page.bounds));
    assert_eq!(source.geometry_quality, GeometryQuality::PageOnly);

    let manifest = DocumentStateManifest {
        state_schema: DOCUMENT_STATE_SCHEMA.into(),
        source_pdf_sha256: digest,
        ir_schema: conversion.document.schema_version,
        evidence_delta_ids: BTreeSet::new(),
        reconciliation_policy_id: Sha256Digest::of_bytes(b"fls5-real-table-gate/1"),
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
    let request = EnrichmentRequest {
        request_id: RequestId::derive(&[b"fls5-rp2040-table1", digest.as_bytes()]),
        source: ScopedBlob {
            id: BlobId::new("rights-reviewed-rp2040-pdf").unwrap(),
            range: BlobRange::new(0, bytes.len() as u64).unwrap(),
            media_type: MediaType::new("application/pdf").unwrap(),
            expected_digest: Some(digest),
        },
        input_state_id: manifest.id().unwrap(),
        goals: vec![CapabilityGoal {
            capability: Capability::TableRecognize,
            scope: RefinementScope::Regions {
                regions: BTreeSet::from([target.clone()]),
            },
        }],
    };
    let mut runtime = EnrichmentRuntime::new(
        ConversionOptions {
            profile: Profile::Offline,
            ..ConversionOptions::default()
        },
        unknown_inventory(),
        None,
    );
    runtime
        .register_stage(
            EnrichmentStageDescriptor {
                id: "table.structure.rulebased".into(),
                stage: Stage::Layout,
                build: Sha256Digest::of_bytes(b"table-rulebased-build-v2"),
                model_digest: None,
                parameters: BTreeMap::new(),
                produces: Capability::TableRecognize,
                requires: BTreeSet::from([Capability::LayoutDetect]),
            },
            RuleBasedTableEngine::new(),
        )
        .unwrap();
    let plan = match runtime
        .plan(&request, &conversion.document, &manifest)
        .unwrap()
    {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        other => panic!("unexpected planning outcome: {other:?}"),
    };
    assert_eq!(plan.invocations.len(), 1);
    assert_eq!(plan.invocations[0].scope, request.goals[0].scope);
    let unchanged_page = &conversion.document.pages[page_index as usize + 1];
    let unchanged_page_id = unchanged_page.id.clone();
    let unchanged_bounds = unchanged_page.bounds;
    let unchanged_layers = unchanged_page
        .layers
        .iter()
        .map(|layer| layer.id.clone())
        .collect::<BTreeSet<_>>();
    let unchanged_artifacts = unchanged_page
        .artifacts
        .iter()
        .map(|artifact| artifact.id.clone())
        .collect::<BTreeSet<_>>();
    let unchanged_evidence = unchanged_page
        .regions
        .iter()
        .flat_map(|region| &region.evidence)
        .map(|evidence| (evidence.id.clone(), serde_json::to_vec(evidence).unwrap()))
        .collect::<BTreeMap<_, _>>();
    let unchanged_edges = unchanged_page
        .reading_order
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let refinement_started = Instant::now();
    let refined = runtime
        .execute(&request, &plan, bytes, &conversion.document, &manifest)
        .unwrap();
    let refinement_millis = refinement_started.elapsed().as_millis();
    refined.document.validate_evidence_grade().unwrap();
    let after_untouched = &refined.document.pages[page_index as usize + 1];
    assert_eq!(after_untouched.id, unchanged_page_id);
    assert_eq!(after_untouched.bounds, unchanged_bounds);
    assert_eq!(
        after_untouched
            .layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect::<BTreeSet<_>>(),
        unchanged_layers
    );
    assert_eq!(
        after_untouched
            .artifacts
            .iter()
            .map(|artifact| artifact.id.clone())
            .collect::<BTreeSet<_>>(),
        unchanged_artifacts
    );
    assert_eq!(
        after_untouched
            .regions
            .iter()
            .flat_map(|region| &region.evidence)
            .map(|evidence| (evidence.id.clone(), serde_json::to_vec(evidence).unwrap()))
            .collect::<BTreeMap<_, _>>(),
        unchanged_evidence
    );
    assert_eq!(
        after_untouched
            .reading_order
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        unchanged_edges
    );
    assert_eq!(refined.deltas[0].page_additions.len(), 1);
    assert_eq!(refined.deltas[0].page_additions[0].page_id, target.page_id);
    let refined_region = refined.document.pages[page_index as usize]
        .regions
        .iter()
        .find(|region| region.id == target.region_id)
        .unwrap();
    assert!(
        refined_region
            .evidence
            .iter()
            .any(|item| item.id == source.id)
    );
    let tables = refined_region
        .evidence
        .iter()
        .filter(|evidence| matches!(evidence.content, EvidenceContent::Table { .. }))
        .collect::<Vec<_>>();
    assert_eq!(tables.len(), 1);
    assert_eq!(
        tables[0].engine_metadata["grammar"],
        "name_description_fragment_v1"
    );
    let EvidenceContent::Table {
        rows,
        columns,
        cells,
    } = &tables[0].content
    else {
        unreachable!()
    };
    assert_eq!(
        (*rows as u64, *columns as u64),
        (
            oracle["expected_rows"].as_u64().unwrap(),
            oracle["expected_columns"].as_u64().unwrap()
        )
    );
    assert_eq!(
        cells.iter().map(|cell| &cell.text).collect::<Vec<_>>(),
        oracle["expected_cells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|cell| cell.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert!(cells.iter().all(|cell| {
        cell.geometry == Some(page.bounds)
            && cell.geometry_quality == GeometryQuality::PageOnly
            && cell.source_spans.len() == 1
            && cell.source_spans[0].evidence_id == source.id
            && text.get(cell.source_spans[0].start as usize..cell.source_spans[0].end as usize)
                == Some(cell.text.as_str())
    }));
    assert_eq!(
        refined.deltas[0].required_evidence_ids,
        BTreeSet::from([source.id.clone()])
    );
    assert_eq!(refined.deltas[0].scope, request.goals[0].scope);
    assert_eq!(manifest.id().unwrap().as_str(), oracle["input_state_id"]);
    assert_eq!(
        refined.state_manifest.id().unwrap().as_str(),
        oracle["output_state_id"]
    );
    assert_eq!(refined.deltas[0].id().unwrap().as_str(), oracle["delta_id"]);
    assert_eq!(
        refined.deltas[0].artifact_digest().unwrap().to_string(),
        oracle["delta_artifact_sha256"]
    );
    assert_eq!(tables[0].id.as_str(), oracle["table_evidence_id"]);
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": "ferrodoc-fls5-real-table-result/1",
            "source_pdf_sha256": digest,
            "page_index": page_index,
            "page_id": target.page_id,
            "region_id": target.region_id,
            "source_evidence_id": source.id,
            "input_state_id": manifest.id().unwrap(),
            "output_state_id": refined.state_manifest.id().unwrap(),
            "delta_id": refined.deltas[0].id().unwrap(),
            "delta_artifact_sha256": refined.deltas[0].artifact_digest().unwrap(),
            "table_evidence_id": tables[0].id,
            "rows": rows,
            "columns": columns,
            "geometry_quality": "page_only",
            "conversion_millis": conversion_millis,
            "targeted_refinement_millis": refinement_millis,
            "cells": cells.iter().map(|cell| serde_json::json!({
                "row": cell.row,
                "column": cell.column,
                "text": cell.text,
                "start": cell.source_spans[0].start,
                "end": cell.source_spans[0].end,
            })).collect::<Vec<_>>()
        }))
        .unwrap()
    );
}
