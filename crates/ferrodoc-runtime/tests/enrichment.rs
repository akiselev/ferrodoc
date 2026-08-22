use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use ferrodoc_core::{
    BackendId, BlobId, BlobRange, Bytes, CURRENT_SCHEMA_VERSION, Capability, CoordinateSpace,
    CoordinateTransform, DeterministicProvenance, DeviceId, DeviceKind, DocumentId,
    DocumentStateId, Estimate, EstimateConfidence, EstimateSource, EvidenceId, LayerId, MediaType,
    MicroUsd, Millis, PageId, PageRect, Probability, Profile, Rect, RegionId, RequestId,
    ResourceEstimate, ScopedBlob, Sha256Digest, Stage, Unit,
};
use ferrodoc_engine_api::{
    Engine, EngineCandidate, EngineCompatibility, EngineDescriptor, EngineError, EngineRequest,
    EngineResponse, ExecutionContext, HardwareInventory, HealthReport, HealthRequest, HealthStatus,
    NetworkUse, conformance::unknown_inventory,
};
use ferrodoc_ir::{
    DOCUMENT_STATE_SCHEMA, Document, DocumentMetadata, DocumentStateManifest, Evidence,
    EvidenceContent, GeometryQuality, Page, PageRegionRef, RefinementScope, Region, RegionKind,
    materialize_from_checkpoint, materialize_state,
};
use ferrodoc_runtime::{
    CacheDecision, ConversionOptions, RuntimeError,
    durable::{DurableError, DurableStateStore, ReferenceCheckpointPolicy},
    enrichment::{
        CapabilityGoal, EnrichmentPlanningOutcome, EnrichmentRequest, EnrichmentRuntime,
        EnrichmentStageDescriptor, PlanEscalation, PlanningCacheState, PlanningEstimate,
        StagePlanningProfile, StageScopePolicy,
    },
};

#[derive(Clone)]
struct ScopedTableEngine {
    descriptor: EngineDescriptor,
    calls: Arc<Mutex<Vec<EngineRequest>>>,
    latency: u64,
    quality: Option<Probability>,
    vram: Estimate<Bytes>,
    wrong_content: bool,
}

impl ScopedTableEngine {
    fn new(calls: Arc<Mutex<Vec<EngineRequest>>>) -> Self {
        Self::with_estimate(calls, "scoped-table", 1, None)
    }

    fn with_estimate(
        calls: Arc<Mutex<Vec<EngineRequest>>>,
        id: &str,
        latency: u64,
        quality: Option<f64>,
    ) -> Self {
        Self {
            descriptor: EngineDescriptor {
                id: id.into(),
                version: "1.0.0".into(),
                capabilities: BTreeSet::from([Capability::TableRecognize]),
                compatibility: vec![EngineCompatibility {
                    backend: BackendId::new("fixture").unwrap(),
                    devices: BTreeSet::from([DeviceKind::Cpu]),
                }],
                deterministic: true,
                network_use: NetworkUse::None,
                max_concurrency: 1,
            },
            calls,
            latency,
            quality: quality.map(|value| Probability::new(value).unwrap()),
            vram: Estimate::Known(Bytes::new(0)),
            wrong_content: false,
        }
    }

    fn with_vram(mut self, vram: Estimate<Bytes>) -> Self {
        self.vram = vram;
        self
    }
}

impl Engine for ScopedTableEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }

    fn health(&mut self, _request: HealthRequest) -> Result<HealthReport, EngineError> {
        Ok(HealthReport {
            status: HealthStatus::Healthy,
            dependencies: Vec::new(),
            message: "ready".into(),
        })
    }

    fn estimate(
        &mut self,
        request: &EngineRequest,
        _inventory: &HardwareInventory,
    ) -> Result<Vec<EngineCandidate>, EngineError> {
        assert_eq!(request.capability, Capability::TableRecognize);
        Ok(vec![EngineCandidate {
            engine_id: self.descriptor.id.clone(),
            backend: BackendId::new("fixture").unwrap(),
            device: DeviceId::new(DeviceKind::Cpu, None).unwrap(),
            resources: ResourceEstimate {
                peak_ram: Estimate::Known(Bytes::new(1024)),
                warm_ram: Estimate::Known(Bytes::new(0)),
                peak_vram: self.vram.clone(),
                warm_vram: Estimate::Known(Bytes::new(0)),
                latency: Estimate::Known(Millis::new(self.latency)),
                remote_cost: Estimate::Known(MicroUsd::new(0)),
                quality: self.quality.map_or(Estimate::Unknown, Estimate::Known),
                source: Estimate::Unknown,
            },
        }])
    }

    fn execute(
        &mut self,
        request: EngineRequest,
        context: &ExecutionContext<'_>,
    ) -> Result<EngineResponse, EngineError> {
        context.checkpoint()?;
        let bytes = context.blobs.resolve(&request.input)?;
        assert_eq!(bytes, b"source-pdf");
        self.calls.lock().unwrap().push(request.clone());
        if !matches!(&request.scope, Some(RefinementScope::Regions { .. })) {
            return Ok(EngineResponse {
                request_id: request.request_id,
                evidence: Vec::new(),
                metadata: BTreeMap::new(),
            });
        }
        let page_index = request.page_index.expect("atomic region has a page");
        let scope = serde_json::to_vec(request.scope.as_ref().unwrap()).unwrap();
        let provenance = DeterministicProvenance {
            schema_version: CURRENT_SCHEMA_VERSION,
            input_digest: Sha256Digest::of_bytes(&bytes),
            engine_id: self.descriptor.id.clone(),
            engine_version: self.descriptor.version.clone(),
            model_digest: None,
            parameters: ferrodoc_engine_api::evidence_parameters(&request),
            stage: Stage::Layout,
        };
        let layer_id = LayerId::derive(&[b"table-layer", &scope]);
        let evidence = Evidence {
            id: EvidenceId::derive(&[b"table-evidence", &scope]),
            layer_id,
            content: if self.wrong_content {
                EvidenceContent::Unknown {
                    media_type: MediaType::new(
                        "application/vnd.ferrodoc.not-actually-a-table+json",
                    )
                    .unwrap(),
                    value: serde_json::json!({}),
                }
            } else {
                EvidenceContent::Table {
                    rows: 1,
                    columns: 1,
                    cells: Vec::new(),
                }
            },
            geometry: Some(PageRect {
                page_index,
                rect: Rect::new(10.0, 10.0, 20.0, 20.0, CoordinateSpace::Pdf, Unit::Point).unwrap(),
                source_transform: CoordinateTransform::IDENTITY,
            }),
            geometry_quality: GeometryQuality::Region,
            confidence: None,
            provenance,
            engine_metadata: BTreeMap::new(),
        };
        Ok(EngineResponse {
            request_id: request.request_id,
            evidence: vec![evidence],
            metadata: BTreeMap::new(),
        })
    }
}

fn fixture() -> (Vec<u8>, Document, DocumentStateManifest, Vec<PageRegionRef>) {
    let bytes = b"source-pdf".to_vec();
    let digest = Sha256Digest::of_bytes(&bytes);
    let document_id = DocumentId::derive(&[digest.as_bytes()]);
    let shared_region_id = RegionId::derive(&[b"page-local-region"]);
    let mut targets = Vec::new();
    let pages = (0_u32..3)
        .map(|index| {
            let page_id = PageId::derive(&[document_id.as_str().as_bytes(), &index.to_be_bytes()]);
            let region_id = if index < 2 {
                shared_region_id.clone()
            } else {
                RegionId::derive(&[b"unrelated-region"])
            };
            if index < 2 {
                targets.push(PageRegionRef {
                    page_id: page_id.clone(),
                    region_id: region_id.clone(),
                });
            }
            let bounds = PageRect {
                page_index: index,
                rect: Rect::new(0.0, 0.0, 100.0, 100.0, CoordinateSpace::Pdf, Unit::Point).unwrap(),
                source_transform: CoordinateTransform::IDENTITY,
            };
            Page {
                id: page_id,
                index,
                bounds,
                layers: Vec::new(),
                artifacts: Vec::new(),
                regions: vec![Region {
                    id: region_id,
                    kind: RegionKind::Table,
                    geometry: PageRect {
                        page_index: index,
                        rect: Rect::new(5.0, 5.0, 50.0, 50.0, CoordinateSpace::Pdf, Unit::Point)
                            .unwrap(),
                        source_transform: CoordinateTransform::IDENTITY,
                    },
                    evidence: Vec::new(),
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
    let manifest = DocumentStateManifest {
        state_schema: DOCUMENT_STATE_SCHEMA.into(),
        source_pdf_sha256: digest,
        ir_schema: CURRENT_SCHEMA_VERSION,
        evidence_delta_ids: BTreeSet::new(),
        reconciliation_policy_id: Sha256Digest::of_bytes(b"policy"),
        coverage: Vec::new(),
        materialized_ir_checkpoint: None,
        parent_state_ids: BTreeSet::new(),
    };
    (bytes, document, manifest, targets)
}

fn request(
    bytes: &[u8],
    manifest: &DocumentStateManifest,
    regions: BTreeSet<PageRegionRef>,
) -> EnrichmentRequest {
    EnrichmentRequest {
        request_id: RequestId::derive(&[b"fp1-request"]),
        source: ScopedBlob {
            id: BlobId::new("source-pdf").unwrap(),
            range: BlobRange::new(0, bytes.len() as u64).unwrap(),
            media_type: MediaType::new("application/pdf").unwrap(),
            expected_digest: Some(Sha256Digest::of_bytes(bytes)),
        },
        input_state_id: manifest.id().unwrap(),
        goals: vec![CapabilityGoal {
            capability: Capability::TableRecognize,
            scope: RefinementScope::Regions { regions },
        }],
    }
}

fn fixture_runtime(calls: Arc<Mutex<Vec<EngineRequest>>>) -> EnrichmentRuntime {
    let options = ConversionOptions {
        profile: Profile::Offline,
        ..ConversionOptions::default()
    };
    let mut runtime = EnrichmentRuntime::new(options, unknown_inventory(), None);
    runtime
        .register_stage(
            EnrichmentStageDescriptor {
                id: "table.structure".into(),
                stage: Stage::Layout,
                build: Sha256Digest::of_bytes(b"scoped-table-build-v1"),
                model_digest: None,
                produces: Capability::TableRecognize,
                requires: BTreeSet::new(),
            },
            ScopedTableEngine::new(calls),
        )
        .unwrap();
    runtime
}

fn measured_planning(
    quality: u64,
    cpu: u64,
    scope_policy: StageScopePolicy,
) -> StagePlanningProfile {
    let report = Sha256Digest::of_bytes(include_bytes!(
        "../../../fixtures/planning/fp5-controlled-observations.json"
    ));
    let measured = |value| {
        PlanningEstimate::known(
            value,
            value,
            value,
            EstimateSource {
                confidence: EstimateConfidence::Measured,
                method: "controlled_minimized_fixture_not_corpus_quality".into(),
            },
            Some(report),
        )
        .unwrap()
    };
    let calibrated = |value| {
        PlanningEstimate::known(
            value,
            value,
            value,
            EstimateSource {
                confidence: EstimateConfidence::Calibrated,
                method: "controlled_minimized_fixture_assumption".into(),
            },
            Some(report),
        )
        .unwrap()
    };
    let bounded = |value| {
        PlanningEstimate::known(
            value,
            value,
            value,
            EstimateSource {
                confidence: EstimateConfidence::Conservative,
                method: "controlled_minimized_fixture_static_resource_bound".into(),
            },
            Some(report),
        )
        .unwrap()
    };
    StagePlanningProfile {
        scope_policy,
        capability_gain_bps: calibrated(10_000),
        success_probability_bps: calibrated(9_500),
        quality_bps: measured(quality),
        cpu_millis: bounded(cpu),
        gpu_millis: bounded(0),
        read_bytes: bounded(10),
        write_bytes: bounded(20),
    }
}

#[test]
fn executes_only_two_page_qualified_table_targets() {
    let (bytes, document, manifest, targets) = fixture();
    let mut request = request(&bytes, &manifest, targets.iter().cloned().collect());
    request.goals.push(request.goals[0].clone());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = fixture_runtime(calls.clone());
    let plan = match runtime.plan(&request, &document, &manifest).unwrap() {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => {
            assert_eq!(pareto.len(), 1);
            pareto.remove(0)
        }
        outcome => panic!("unexpected plan: {outcome:?}"),
    };
    assert_eq!(plan.invocations.len(), 2);
    assert!(plan.invocations.iter().all(|invocation| {
        invocation.capability == Capability::TableRecognize
            && matches!(
                &invocation.scope,
                RefinementScope::Regions { regions } if regions.len() == 1
            )
    }));

    let result = runtime
        .execute(&request, &plan, bytes.clone(), &document, &manifest)
        .unwrap();
    assert_eq!(calls.lock().unwrap().len(), 2);
    assert_eq!(result.deltas.len(), 2);
    assert_eq!(result.document.pages[0].regions[0].evidence.len(), 1);
    assert_eq!(result.document.pages[1].regions[0].evidence.len(), 1);
    assert!(result.document.pages[2].regions[0].evidence.is_empty());
    assert_eq!(
        result
            .state_manifest
            .materialized_ir_checkpoint
            .as_ref()
            .unwrap()
            .document_ir_logical_sha256,
        Sha256Digest::of_bytes(&result.document.to_canonical_json().unwrap())
    );
    assert!(result.deltas.iter().all(|delta| {
        delta.coverage_delta[0].status == "complete"
            && matches!(
                &delta.scope,
                RefinementScope::Regions { regions } if regions.len() == 1
            )
    }));

    let mut second_runtime = fixture_runtime(Arc::new(Mutex::new(Vec::new())));
    let second_plan = match second_runtime.plan(&request, &document, &manifest).unwrap() {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        outcome => panic!("unexpected plan: {outcome:?}"),
    };
    let second = second_runtime
        .execute(&request, &second_plan, bytes, &document, &manifest)
        .unwrap();
    assert_eq!(result.deltas, second.deltas);
    assert_eq!(
        result.state_manifest.id().unwrap(),
        second.state_manifest.id().unwrap()
    );

    let mut satisfied = request.clone();
    satisfied.input_state_id = result.state_manifest.id().unwrap();
    assert!(matches!(
        runtime
            .plan(&satisfied, &result.document, &result.state_manifest)
            .unwrap(),
        EnrichmentPlanningOutcome::AlreadySatisfied
    ));
}

#[test]
fn wrong_page_and_missing_capability_fail_closed() {
    let (bytes, document, manifest, targets) = fixture();
    let wrong_page = PageRegionRef {
        page_id: document.pages[2].id.clone(),
        region_id: targets[0].region_id.clone(),
    };
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = fixture_runtime(calls);
    let wrong = request(&bytes, &manifest, BTreeSet::from([wrong_page]));
    assert!(matches!(
        runtime.plan(&wrong, &document, &manifest),
        Err(RuntimeError::InvalidEnrichment(_))
    ));

    let mut stale = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
    stale.input_state_id = DocumentStateId::derive(&[b"stale-state"]);
    assert!(matches!(
        runtime.plan(&stale, &document, &manifest),
        Err(RuntimeError::InvalidEnrichment(_))
    ));

    let mut unsupported = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
    unsupported.goals[0].capability = Capability::FormulaRecognize;
    assert!(matches!(
        runtime.plan(&unsupported, &document, &manifest).unwrap(),
        EnrichmentPlanningOutcome::NoAdmissiblePlan { .. }
    ));
}

#[test]
fn table_capability_rejects_non_table_engine_evidence() {
    let (bytes, document, manifest, targets) = fixture();
    let request = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut engine = ScopedTableEngine::new(calls);
    engine.wrong_content = true;
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
                id: "table.wrong-content".into(),
                stage: Stage::Layout,
                build: Sha256Digest::of_bytes(b"wrong-table-content"),
                model_digest: None,
                produces: Capability::TableRecognize,
                requires: BTreeSet::new(),
            },
            engine,
        )
        .unwrap();
    let plan = match runtime.plan(&request, &document, &manifest).unwrap() {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        outcome => panic!("unexpected plan: {outcome:?}"),
    };
    assert!(matches!(
        runtime.execute(&request, &plan, bytes, &document, &manifest),
        Err(RuntimeError::InvalidEnrichment(message))
            if message.contains("non-table evidence")
    ));
}

#[test]
fn declared_prerequisite_is_not_implicitly_executed() {
    let (bytes, document, manifest, targets) = fixture();
    let request = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let options = ConversionOptions {
        profile: Profile::Offline,
        ..ConversionOptions::default()
    };
    let mut runtime = EnrichmentRuntime::new(options, unknown_inventory(), None);
    runtime
        .register_stage(
            EnrichmentStageDescriptor {
                id: "table.needs-layout".into(),
                stage: Stage::Layout,
                build: Sha256Digest::of_bytes(b"scoped-table-build-v1"),
                model_digest: None,
                produces: Capability::TableRecognize,
                requires: BTreeSet::from([Capability::LayoutDetect]),
            },
            ScopedTableEngine::new(calls.clone()),
        )
        .unwrap();
    let outcome = runtime.plan(&request, &document, &manifest).unwrap();
    assert!(matches!(
        outcome,
        EnrichmentPlanningOutcome::NoAdmissiblePlan { ref reasons }
            if reasons.iter().any(|reason| reason.contains("requires layout.detect"))
    ));
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn document_and_page_scopes_execute_without_inventing_owned_evidence() {
    let (bytes, document, manifest, targets) = fixture();
    for scope in [
        RefinementScope::Document,
        RefinementScope::Pages {
            page_ids: BTreeSet::from([targets[0].page_id.clone()]),
        },
    ] {
        let mut request = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
        request.goals[0].scope = scope.clone();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut runtime = fixture_runtime(calls.clone());
        let plan = match runtime.plan(&request, &document, &manifest).unwrap() {
            EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
            outcome => panic!("unexpected plan: {outcome:?}"),
        };
        let result = runtime
            .execute(&request, &plan, bytes.clone(), &document, &manifest)
            .unwrap();
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert_eq!(result.deltas[0].scope, scope);
        assert_eq!(result.deltas[0].coverage_delta[0].status, "candidate");
        assert_eq!(result.document, document);
    }
}

#[test]
fn candidate_plans_remove_a_provably_dominated_alternative() {
    let (bytes, document, manifest, targets) = fixture();
    let request = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
    let options = ConversionOptions {
        profile: Profile::Offline,
        ..ConversionOptions::default()
    };
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = EnrichmentRuntime::new(options, unknown_inventory(), None);
    for (stage_id, engine_id, latency, quality) in [
        ("table.fast", "fast-table", 1, 0.9),
        ("table.slow", "slow-table", 2, 0.8),
    ] {
        runtime
            .register_stage_with_planning(
                EnrichmentStageDescriptor {
                    id: stage_id.into(),
                    stage: Stage::Layout,
                    build: Sha256Digest::of_bytes(engine_id.as_bytes()),
                    model_digest: None,
                    produces: Capability::TableRecognize,
                    requires: BTreeSet::new(),
                },
                measured_planning(
                    (quality * 10_000.0) as u64,
                    latency,
                    StageScopePolicy::Requested,
                ),
                ScopedTableEngine::with_estimate(calls.clone(), engine_id, latency, Some(quality)),
            )
            .unwrap();
    }
    let EnrichmentPlanningOutcome::CandidatePlans { pareto } =
        runtime.plan(&request, &document, &manifest).unwrap()
    else {
        panic!("expected candidate plans");
    };
    assert_eq!(pareto.len(), 1);
    assert_eq!(pareto[0].invocations[0].engine_id, "fast-table");
}

#[test]
fn pareto_retains_quality_cost_tradeoff_and_planning_does_not_execute() {
    let (bytes, document, manifest, targets) = fixture();
    let request = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = EnrichmentRuntime::new(
        ConversionOptions {
            profile: Profile::Offline,
            ..ConversionOptions::default()
        },
        unknown_inventory(),
        None,
    );
    for (stage_id, engine_id, latency, quality, quality_bps, cpu) in [
        ("table.accurate", "accurate-table", 10, 0.95, 9_500, 10),
        ("table.fast", "fast-table", 1, 0.80, 8_000, 1),
    ] {
        runtime
            .register_stage_with_planning(
                EnrichmentStageDescriptor {
                    id: stage_id.into(),
                    stage: Stage::Layout,
                    build: Sha256Digest::of_bytes(engine_id.as_bytes()),
                    model_digest: None,
                    produces: Capability::TableRecognize,
                    requires: BTreeSet::new(),
                },
                measured_planning(quality_bps, cpu, StageScopePolicy::Requested),
                ScopedTableEngine::with_estimate(calls.clone(), engine_id, latency, Some(quality)),
            )
            .unwrap();
    }
    let EnrichmentPlanningOutcome::CandidatePlans { pareto } =
        runtime.plan(&request, &document, &manifest).unwrap()
    else {
        panic!("expected plans")
    };
    assert_eq!(pareto.len(), 2);
    assert!(
        calls.lock().unwrap().is_empty(),
        "planning must not execute engines"
    );
    assert!(pareto.iter().all(|plan| matches!(
        plan.explanation.quality_bps,
        PlanningEstimate::Known {
            benchmark_digest: Some(_),
            ..
        }
    )));
}

#[test]
fn targeted_and_whole_document_alternatives_are_explainable_and_plan_id_is_semantic() {
    let (bytes, document, manifest, targets) = fixture();
    let mut request = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = EnrichmentRuntime::new(
        ConversionOptions {
            profile: Profile::Offline,
            ..ConversionOptions::default()
        },
        unknown_inventory(),
        None,
    );
    for (stage_id, engine_id, scope, quality, cpu) in [
        (
            "table.targeted",
            "targeted-table",
            StageScopePolicy::Requested,
            8_000,
            1,
        ),
        (
            "table.whole",
            "whole-table",
            StageScopePolicy::WholeDocument,
            9_000,
            20,
        ),
    ] {
        runtime
            .register_stage_with_planning(
                EnrichmentStageDescriptor {
                    id: stage_id.into(),
                    stage: Stage::Layout,
                    build: Sha256Digest::of_bytes(engine_id.as_bytes()),
                    model_digest: None,
                    produces: Capability::TableRecognize,
                    requires: BTreeSet::new(),
                },
                measured_planning(quality, cpu, scope),
                ScopedTableEngine::with_estimate(
                    calls.clone(),
                    engine_id,
                    cpu,
                    Some(quality as f64 / 10_000.0),
                ),
            )
            .unwrap();
    }
    let first = match runtime.plan(&request, &document, &manifest).unwrap() {
        EnrichmentPlanningOutcome::CandidatePlans { pareto } => pareto,
        outcome => panic!("unexpected {outcome:?}"),
    };
    assert_eq!(first.len(), 2);
    let whole = first
        .iter()
        .find(|plan| matches!(plan.invocations[0].scope, RefinementScope::Document))
        .unwrap();
    assert!(
        whole
            .explanation
            .escalations
            .iter()
            .any(|reason| matches!(reason, PlanEscalation::WholeDocumentForNarrowGoal { .. }))
    );
    let ids = first
        .iter()
        .map(|plan| plan.plan_id)
        .collect::<BTreeSet<_>>();
    request.request_id = RequestId::derive(&[b"different-correlation-id"]);
    let second = match runtime.plan(&request, &document, &manifest).unwrap() {
        EnrichmentPlanningOutcome::CandidatePlans { pareto } => pareto,
        outcome => panic!("unexpected {outcome:?}"),
    };
    assert_eq!(ids, second.iter().map(|plan| plan.plan_id).collect());
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn unknown_planning_dimensions_make_dominance_indeterminate() {
    let (bytes, document, manifest, targets) = fixture();
    let request = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = EnrichmentRuntime::new(
        ConversionOptions {
            profile: Profile::Offline,
            ..ConversionOptions::default()
        },
        unknown_inventory(),
        None,
    );
    for (stage_id, engine_id, latency, quality) in [
        ("table.fast", "unknown-fast", 1, 0.9),
        ("table.slow", "unknown-slow", 2, 0.8),
    ] {
        runtime
            .register_stage(
                EnrichmentStageDescriptor {
                    id: stage_id.into(),
                    stage: Stage::Layout,
                    build: Sha256Digest::of_bytes(engine_id.as_bytes()),
                    model_digest: None,
                    produces: Capability::TableRecognize,
                    requires: BTreeSet::new(),
                },
                ScopedTableEngine::with_estimate(calls.clone(), engine_id, latency, Some(quality)),
            )
            .unwrap();
    }
    let EnrichmentPlanningOutcome::CandidatePlans { pareto } =
        runtime.plan(&request, &document, &manifest).unwrap()
    else {
        panic!("expected plans")
    };
    assert_eq!(pareto.len(), 2);
    assert!(pareto.iter().all(|plan| {
        plan.explanation
            .escalations
            .iter()
            .any(|reason| matches!(reason, PlanEscalation::UnknownEstimate { .. }))
    }));
}

#[test]
fn enrichment_low_vram_hard_gate_refuses_excess_and_unknown_without_fallback() {
    let (bytes, document, manifest, targets) = fixture();
    let request = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
    for (engine_id, vram, expected) in [
        (
            "over-vram",
            Estimate::Known(Bytes::new(3 * Bytes::GIB)),
            "VramBudgetExceeded",
        ),
        ("unknown-vram", Estimate::Unknown, "VramUnknown"),
    ] {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut runtime = EnrichmentRuntime::new(
            ConversionOptions {
                profile: Profile::LowVram,
                ..ConversionOptions::default()
            },
            unknown_inventory(),
            None,
        );
        runtime
            .register_stage(
                EnrichmentStageDescriptor {
                    id: format!("stage.{engine_id}"),
                    stage: Stage::Layout,
                    build: Sha256Digest::of_bytes(engine_id.as_bytes()),
                    model_digest: None,
                    produces: Capability::TableRecognize,
                    requires: BTreeSet::new(),
                },
                ScopedTableEngine::with_estimate(calls.clone(), engine_id, 1, Some(0.8))
                    .with_vram(vram),
            )
            .unwrap();
        let EnrichmentPlanningOutcome::NoAdmissiblePlan { reasons } =
            runtime.plan(&request, &document, &manifest).unwrap()
        else {
            panic!("hard VRAM gate must refuse")
        };
        assert!(reasons.iter().any(|reason| reason.contains(expected)));
        assert!(calls.lock().unwrap().is_empty());
    }
}

#[test]
fn controlled_measurement_fixture_is_self_consistent_and_explicitly_non_corpus() {
    let value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../fixtures/planning/fp5-controlled-observations.json"
    ))
    .unwrap();
    assert!(
        value["scope"]
            .as_str()
            .unwrap()
            .contains("not representative corpus")
    );
    for observation in value["observations"].as_array().unwrap() {
        let truth = observation["truth_items"].as_u64().unwrap();
        let correct = observation["correct_items"].as_u64().unwrap();
        assert_eq!(
            observation["quality_bps"].as_u64(),
            Some(correct * 10_000 / truth)
        );
    }
}

#[test]
fn durable_cold_warm_reuse_and_checkpoint_tail_replay_are_canonical() {
    let durable_root = tempfile::tempdir().unwrap();
    let (bytes, initial, initial_manifest, targets) = fixture();
    let first_request = request(
        &bytes,
        &initial_manifest,
        BTreeSet::from([targets[0].clone()]),
    );
    let cold_calls = Arc::new(Mutex::new(Vec::new()));
    let mut cold = fixture_runtime(cold_calls.clone())
        .with_durable_store(DurableStateStore::open(durable_root.path()).unwrap());
    let first_plan = match cold
        .plan(&first_request, &initial, &initial_manifest)
        .unwrap()
    {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        outcome => panic!("unexpected plan: {outcome:?}"),
    };
    let first = cold
        .execute(
            &first_request,
            &first_plan,
            bytes.clone(),
            &initial,
            &initial_manifest,
        )
        .unwrap();
    assert_eq!(cold_calls.lock().unwrap().len(), 1);
    assert_eq!(first.durable_reuse[0].cache, CacheDecision::Miss);
    let warm_plan = match cold
        .plan(&first_request, &initial, &initial_manifest)
        .unwrap()
    {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        outcome => panic!("unexpected plan: {outcome:?}"),
    };
    assert!(
        warm_plan
            .explanation
            .cache_states
            .iter()
            .all(|observation| { observation.state == PlanningCacheState::VerifiedHit })
    );
    let first_artifacts = first.durable_artifacts.clone().unwrap();
    let storage = first_artifacts.summarize(bytes.len() as u64, first.document.pages.len() as u32);
    assert!(storage.delta_bytes > 0);
    assert!(storage.state_manifest_bytes > 0);
    assert_eq!(
        storage.checkpoint_bytes,
        first_artifacts.checkpoint.as_ref().map(|item| item.bytes)
    );
    assert!(storage.incremental_to_pdf_ratio.unwrap().is_finite());

    // A separately constructed worker shares only the durable root and does not execute its engine.
    let warm_calls = Arc::new(Mutex::new(Vec::new()));
    let mut warm = fixture_runtime(warm_calls.clone())
        .with_durable_store(DurableStateStore::open(durable_root.path()).unwrap());
    let warm_plan = match warm
        .plan(&first_request, &initial, &initial_manifest)
        .unwrap()
    {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        outcome => panic!("unexpected plan: {outcome:?}"),
    };
    let reused = warm
        .execute(
            &first_request,
            &warm_plan,
            bytes.clone(),
            &initial,
            &initial_manifest,
        )
        .unwrap();
    assert!(warm_calls.lock().unwrap().is_empty());
    assert_eq!(reused.durable_reuse[0].cache, CacheDecision::Hit);
    assert_eq!(first.deltas, reused.deltas);
    assert_eq!(first.state_manifest, reused.state_manifest);
    assert_eq!(first.document, reused.document);
    assert_eq!(first.durable_artifacts, reused.durable_artifacts);

    let store = DurableStateStore::open(durable_root.path()).unwrap();
    let loaded_first_manifest = store
        .load_manifest(&first_artifacts.state_manifest)
        .unwrap();
    let first_checkpoint_ref = first_artifacts.checkpoint.as_ref().unwrap();
    let loaded_first_checkpoint = store.load_checkpoint(first_checkpoint_ref).unwrap();
    let loaded_first_delta = store.load_delta(&first_artifacts.deltas[0]).unwrap();
    assert_eq!(loaded_first_manifest, first.state_manifest);
    assert_eq!(loaded_first_checkpoint, first.document);
    assert_eq!(loaded_first_delta, first.deltas[0]);

    // Produce a later independent state, retaining the first state's durable anchor.
    let later_request = request(
        &bytes,
        &first.state_manifest,
        BTreeSet::from([targets[1].clone()]),
    );
    let later_plan = match cold
        .plan(&later_request, &first.document, &first.state_manifest)
        .unwrap()
    {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        outcome => panic!("unexpected plan: {outcome:?}"),
    };
    let later = cold
        .execute(
            &later_request,
            &later_plan,
            bytes,
            &first.document,
            &first.state_manifest,
        )
        .unwrap();
    let full = materialize_state(
        &initial,
        &[first.deltas[0].clone(), later.deltas[0].clone()],
        &later.state_manifest,
    )
    .unwrap();
    let checkpoint_tail = materialize_from_checkpoint(
        &loaded_first_checkpoint,
        &loaded_first_manifest,
        &later.deltas,
        &later.state_manifest,
    )
    .unwrap();
    assert_eq!(
        full.to_canonical_json().unwrap(),
        checkpoint_tail.to_canonical_json().unwrap()
    );
    assert_eq!(
        full.to_canonical_json().unwrap(),
        later.document.to_canonical_json().unwrap()
    );

    // The old state/evidence anchor remains independently loadable after the newer state exists.
    let old_again = store.load_checkpoint(first_checkpoint_ref).unwrap();
    let old_evidence_id = first.deltas[0].page_additions[0].region_evidence[0].evidence[0]
        .id
        .clone();
    assert!(
        old_again
            .pages
            .iter()
            .flat_map(|page| &page.regions)
            .flat_map(|region| &region.evidence)
            .any(|item| item.id == old_evidence_id)
    );
    assert!(
        later
            .document
            .pages
            .iter()
            .flat_map(|page| &page.regions)
            .flat_map(|region| &region.evidence)
            .any(|item| item.id == old_evidence_id)
    );
}

#[test]
fn checkpoint_policy_can_retain_only_deltas_and_manifest() {
    let durable_root = tempfile::tempdir().unwrap();
    let (bytes, document, manifest, targets) = fixture();
    let request = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
    let mut runtime = fixture_runtime(Arc::new(Mutex::new(Vec::new())))
        .with_durable_store(DurableStateStore::open(durable_root.path()).unwrap())
        .with_checkpoint_policy(Arc::new(ReferenceCheckpointPolicy::Never));
    let plan = match runtime.plan(&request, &document, &manifest).unwrap() {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        outcome => panic!("unexpected plan: {outcome:?}"),
    };
    let result = runtime
        .execute(&request, &plan, bytes, &document, &manifest)
        .unwrap();
    let artifacts = result.durable_artifacts.unwrap();
    assert!(result.state_manifest.materialized_ir_checkpoint.is_none());
    assert!(artifacts.checkpoint.is_none());
    assert_eq!(
        materialize_state(&document, &result.deltas, &result.state_manifest)
            .unwrap()
            .to_canonical_json()
            .unwrap(),
        result.document.to_canonical_json().unwrap()
    );
}

#[test]
fn durable_physical_realizations_are_nonsemantic_and_fail_closed() {
    let durable_root = tempfile::tempdir().unwrap();
    let (bytes, document, manifest, targets) = fixture();
    let request = request(&bytes, &manifest, BTreeSet::from([targets[0].clone()]));
    let mut runtime = fixture_runtime(Arc::new(Mutex::new(Vec::new())))
        .with_durable_store(DurableStateStore::open(durable_root.path()).unwrap());
    let plan = match runtime.plan(&request, &document, &manifest).unwrap() {
        EnrichmentPlanningOutcome::CandidatePlans { mut pareto } => pareto.remove(0),
        outcome => panic!("unexpected plan: {outcome:?}"),
    };
    let execution = runtime
        .execute(&request, &plan, bytes.clone(), &document, &manifest)
        .unwrap();
    let artifacts = execution.durable_artifacts.unwrap();
    let store = DurableStateStore::open(durable_root.path()).unwrap();

    let mut alternate = execution.state_manifest.clone();
    alternate
        .materialized_ir_checkpoint
        .as_mut()
        .unwrap()
        .representation =
        "application/vnd.ferrodoc.document-ir+json;version=1;storage=alternate".into();
    let alternate_ref = store.persist_manifest(&alternate).unwrap();
    assert_eq!(
        alternate.id().unwrap(),
        execution.state_manifest.id().unwrap()
    );
    assert_ne!(
        alternate_ref.artifact_id,
        artifacts.state_manifest.artifact_id
    );

    let empty = tempfile::tempdir().unwrap();
    assert!(matches!(
        DurableStateStore::open(empty.path())
            .unwrap()
            .load_checkpoint(artifacts.checkpoint.as_ref().unwrap()),
        Err(DurableError::Missing { .. })
    ));
    let mut stale = artifacts.state_manifest.clone();
    stale.logical_id = DocumentStateId::derive(&[b"stale-state"]).to_string();
    assert!(matches!(
        store.load_manifest(&stale),
        Err(DurableError::Invalid { .. })
    ));

    // A backend must not be able to relabel arbitrary canonical JSON as a refinement delta.
    let refinement_metadata =
        std::fs::read_dir(durable_root.path().join("refinements").join("entries"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .join("metadata.json");
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&refinement_metadata).unwrap()).unwrap();
    metadata["media_type"] = serde_json::json!("application/octet-stream");
    std::fs::write(
        &refinement_metadata,
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
    let mut relabeled = fixture_runtime(Arc::new(Mutex::new(Vec::new())))
        .with_durable_store(DurableStateStore::open(durable_root.path()).unwrap());
    assert!(matches!(
        relabeled.plan(&request, &document, &manifest),
        Err(RuntimeError::Durable(DurableError::Invalid { .. }))
    ));

    // Locate this checkpoint's content entry through its validated metadata, then corrupt it.
    let entries = durable_root.path().join("artifacts").join("entries");
    let checkpoint_value = std::fs::read_dir(entries)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find_map(|directory| {
            let metadata: serde_json::Value =
                serde_json::from_slice(&std::fs::read(directory.join("metadata.json")).ok()?)
                    .ok()?;
            (metadata["key"]["input_digest"]
                == serde_json::to_value(artifacts.checkpoint.as_ref().unwrap().bytes_sha256)
                    .unwrap())
            .then(|| directory.join("value.bin"))
        })
        .unwrap();
    std::fs::write(checkpoint_value, b"corrupt").unwrap();
    assert!(matches!(
        store.load_checkpoint(artifacts.checkpoint.as_ref().unwrap()),
        Err(DurableError::Cache(_))
    ));
}
