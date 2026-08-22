//! Capability-scoped progressive execution over immutable document states.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use ferrodoc_core::{
    ArtifactId, Capability, DocumentStateId, LayerId, PageId, RequestId, ScopedBlob, Sha256Digest,
    Stage,
};
use ferrodoc_engine_api::{
    Engine, EngineCandidate, EngineRequest, HardwareInventory, SOURCE_TEXT_EVIDENCE_PARAMETER,
    SourceTextEvidence, evidence_parameters,
};
use ferrodoc_ir::{
    CoverageEntry, DOCUMENT_STATE_SCHEMA, DeltaProducer, Document, DocumentStateManifest,
    EVIDENCE_DELTA_SCHEMA, EvidenceContent, EvidenceDelta, LayerOwner, MaterializedIrCheckpoint,
    OwnedSourceLayer, PageDelta, RefinementScope, RegionEvidenceAddition, SourceLayer,
    SourceLayerKind, materialize_from_checkpoint,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    CacheDecision, ConversionOptions, OneBlob, ResourceExecutionTrace, ResourceRuntime,
    RuntimeError, StageExecutionRecord,
    cache::{Cacheability, StageCache},
    durable::{
        CheckpointPolicy, CheckpointPolicyContext, DurableExecutionArtifacts, DurableStateStore,
        ReferenceCheckpointPolicy, RefinementKeyInput, refinement_key,
    },
    execute_controlled, planner,
};

const MAX_PARETO_PLANS: usize = 64;

/// One generic capability requested over an explicit semantic scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityGoal {
    /// Capability to produce.
    pub capability: Capability,
    /// Whole-document, explicit-page, or page-qualified-region scope.
    pub scope: RefinementScope,
}

/// A state-aware progressive execution request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EnrichmentRequest {
    /// Observational correlation identity. It does not enter evidence identity.
    pub request_id: RequestId,
    /// Exact immutable source bytes exposed through the scoped-blob boundary.
    pub source: ScopedBlob,
    /// Pinned logical base state.
    pub input_state_id: DocumentStateId,
    /// Nonempty capability goals.
    pub goals: Vec<CapabilityGoal>,
}

/// One registered stage's semantic declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EnrichmentStageDescriptor {
    /// Stable stage identity, unique within one runtime.
    pub id: String,
    /// Logical provenance stage written into the delta.
    pub stage: Stage,
    /// Immutable engine source/build identity written into delta provenance.
    pub build: Sha256Digest,
    /// Immutable model identity, when this registered stage uses a model.
    #[serde(default)]
    pub model_digest: Option<Sha256Digest>,
    /// Capability this stage produces.
    pub produces: Capability,
    /// Capabilities that must already be complete over the same scope.
    #[serde(default)]
    pub requires: BTreeSet<Capability>,
}

/// One atomic engine invocation. Page and region sets are split into singleton scopes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EnrichmentInvocation {
    /// Registered stage.
    pub stage_id: String,
    /// Selected engine.
    pub engine_id: String,
    /// Capability produced by the stage.
    pub capability: Capability,
    /// Atomic scope sent to the engine.
    pub scope: RefinementScope,
    /// Transport-independent semantic request.
    pub request: EngineRequest,
    /// Selected admissible placement and conservative estimate.
    pub candidate: EngineCandidate,
}

/// One admissible local plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EnrichmentCandidatePlan {
    /// Correlation identity of the request this plan answers.
    pub enrichment_request_id: RequestId,
    /// Deterministically ordered atomic invocations.
    pub invocations: Vec<EnrichmentInvocation>,
}

/// Explainable planning outcome for a capability request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EnrichmentPlanningOutcome {
    /// Every requested capability is already complete over the requested scopes.
    AlreadySatisfied,
    /// At least one goal has no stage whose prerequisites and hard policy pass.
    NoAdmissiblePlan {
        /// Stable, bounded explanations.
        reasons: Vec<String>,
    },
    /// Nondominated local alternatives. FP5 will add benchmark quality/value estimates.
    CandidatePlans {
        /// Bounded deterministic frontier.
        pareto: Vec<EnrichmentCandidatePlan>,
    },
}

/// Immutable semantic results plus resource observations kept outside their identities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EnrichmentExecution {
    /// One immutable delta per atomic stage invocation.
    pub deltas: Vec<EvidenceDelta>,
    /// Resulting logical state manifest.
    pub state_manifest: DocumentStateManifest,
    /// Canonical materialized view, returned here as an FP1 conformance oracle.
    pub document: Document,
    /// Scheduler, cache, and measured-resource observations.
    pub resources: ResourceExecutionTrace,
    /// Durable physical artifacts, when a durable provider was configured.
    #[serde(default)]
    pub durable_artifacts: Option<DurableExecutionArtifacts>,
    /// State-aware durable reuse observations, kept outside semantic identities.
    #[serde(default)]
    pub durable_reuse: Vec<DurableReuseRecord>,
}

/// One state-aware durable refinement lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DurableReuseRecord {
    /// Registered semantic stage.
    pub stage: String,
    /// Complete semantic cache-key digest.
    pub key_sha256: Sha256Digest,
    /// Cold execution or verified warm reuse.
    pub cache: CacheDecision,
}

struct RegisteredStage {
    descriptor: EnrichmentStageDescriptor,
    engine: Box<dyn Engine>,
}

/// Local scoped-capability planner/executor using the established engine/runtime boundaries.
pub struct EnrichmentRuntime {
    stages: BTreeMap<String, RegisteredStage>,
    options: ConversionOptions,
    inventory: HardwareInventory,
    cache: Option<StageCache>,
    durable: Option<DurableStateStore>,
    checkpoint_policy: Arc<dyn CheckpointPolicy>,
}

impl EnrichmentRuntime {
    /// Creates a runtime with explicit policy, inventory, and optional deterministic cache.
    pub fn new(
        options: ConversionOptions,
        inventory: HardwareInventory,
        cache: Option<StageCache>,
    ) -> Self {
        Self {
            stages: BTreeMap::new(),
            options,
            inventory,
            cache,
            durable: None,
            checkpoint_policy: Arc::new(ReferenceCheckpointPolicy::Always),
        }
    }

    /// Enables shared durable delta/state/checkpoint persistence and cross-worker reuse.
    pub fn with_durable_store(mut self, store: DurableStateStore) -> Self {
        self.durable = Some(store);
        self
    }

    /// Selects physical checkpoint placement independently from logical state identity.
    pub fn with_checkpoint_policy(mut self, policy: Arc<dyn CheckpointPolicy>) -> Self {
        self.checkpoint_policy = policy;
        self
    }

    /// Registers a semantic stage backed by an embedded or process-transport engine.
    pub fn register_stage(
        &mut self,
        descriptor: EnrichmentStageDescriptor,
        engine: impl Engine + 'static,
    ) -> Result<(), RuntimeError> {
        engine.descriptor().validate()?;
        if descriptor.id.trim().is_empty()
            || !engine
                .descriptor()
                .capabilities
                .contains(&descriptor.produces)
        {
            return Err(RuntimeError::InvalidEnrichment(
                "stage ID is empty or its engine does not declare the produced capability".into(),
            ));
        }
        if self.stages.contains_key(&descriptor.id) {
            return Err(RuntimeError::InvalidEnrichment(format!(
                "duplicate enrichment stage {}",
                descriptor.id
            )));
        }
        self.stages.insert(
            descriptor.id.clone(),
            RegisteredStage {
                descriptor,
                engine: Box::new(engine),
            },
        );
        Ok(())
    }

    /// Plans only the missing atomic goals against the pinned materialized state.
    pub fn plan(
        &mut self,
        request: &EnrichmentRequest,
        document: &Document,
        manifest: &DocumentStateManifest,
    ) -> Result<EnrichmentPlanningOutcome, RuntimeError> {
        validate_request(request, document, manifest)?;
        let atomic_goals = request
            .goals
            .iter()
            .flat_map(|goal| {
                atomic_scopes(&goal.scope)
                    .into_iter()
                    .map(move |scope| (goal.capability, scope))
            })
            .filter(|(capability, scope)| {
                !coverage_complete(&manifest.coverage, *capability, scope)
            })
            .collect::<BTreeSet<_>>();
        if atomic_goals.is_empty() {
            return Ok(EnrichmentPlanningOutcome::AlreadySatisfied);
        }

        let mut choices = Vec::new();
        let mut reasons = Vec::new();
        for (capability, scope) in atomic_goals {
            let mut goal_choices = Vec::new();
            for registered in self.stages.values_mut() {
                if registered.descriptor.produces != capability {
                    continue;
                }
                if let Some(missing) = registered
                    .descriptor
                    .requires
                    .iter()
                    .find(|required| !coverage_complete(&manifest.coverage, **required, &scope))
                {
                    reasons.push(format!(
                        "stage {} requires {} complete over {}",
                        registered.descriptor.id,
                        missing,
                        scope_label(&scope)
                    ));
                    continue;
                }
                let page_index = scope_page_index(&scope, document)?;
                let mut parameters = BTreeMap::new();
                parameters.insert(
                    "ferrodoc.scope".into(),
                    serde_json::to_value(&scope)
                        .map_err(|error| RuntimeError::InvalidEnrichment(error.to_string()))?,
                );
                if capability == Capability::TableRecognize {
                    let source_text = scope_source_text(&scope, document)?;
                    parameters.insert(
                        SOURCE_TEXT_EVIDENCE_PARAMETER.into(),
                        serde_json::to_value(source_text)
                            .map_err(|error| RuntimeError::InvalidEnrichment(error.to_string()))?,
                    );
                }
                let semantic_scope = serde_json::to_vec(&scope)
                    .map_err(|error| RuntimeError::InvalidEnrichment(error.to_string()))?;
                let engine_request = EngineRequest {
                    request_id: RequestId::derive(&[
                        manifest.source_pdf_sha256.as_bytes(),
                        registered.descriptor.id.as_bytes(),
                        capability.to_string().as_bytes(),
                        &semantic_scope,
                    ]),
                    capability,
                    input: request.source.clone(),
                    page_index,
                    scope: Some(scope.clone()),
                    parameters,
                    deterministic_seed: None,
                    deadline: self.options.deadline,
                };
                let report = super::plan_request(
                    registered.engine.as_mut(),
                    &engine_request,
                    planner::ModelAvailability::NotRequired,
                    &self.options,
                    &self.inventory,
                )?;
                if let Some(candidate) = report.selected {
                    goal_choices.push(EnrichmentInvocation {
                        stage_id: registered.descriptor.id.clone(),
                        engine_id: registered.engine.descriptor().id.clone(),
                        capability,
                        scope: scope.clone(),
                        request: engine_request,
                        candidate,
                    });
                } else {
                    reasons.extend(report.decisions.into_iter().flat_map(|decision| {
                        let stage = registered.descriptor.id.clone();
                        decision.reasons.into_iter().map(move |reason| {
                            format!("stage {stage}: {:?}: {}", reason.code, reason.explanation)
                        })
                    }));
                }
            }
            if goal_choices.is_empty() {
                if !self
                    .stages
                    .values()
                    .any(|stage| stage.descriptor.produces == capability)
                {
                    reasons.push(format!(
                        "no registered stage produces {} over {}",
                        capability,
                        scope_label(&scope)
                    ));
                }
                reasons.sort();
                reasons.dedup();
                return Ok(EnrichmentPlanningOutcome::NoAdmissiblePlan { reasons });
            }
            goal_choices.sort_by(|left, right| {
                (&left.stage_id, &left.engine_id).cmp(&(&right.stage_id, &right.engine_id))
            });
            choices.push(goal_choices);
        }

        let mut combinations = vec![Vec::new()];
        for alternatives in choices {
            combinations = combinations
                .into_iter()
                .flat_map(|prefix| {
                    alternatives.iter().cloned().map(move |invocation| {
                        let mut plan = prefix.clone();
                        plan.push(invocation);
                        plan
                    })
                })
                .take(MAX_PARETO_PLANS)
                .collect();
        }
        let candidates = combinations
            .into_iter()
            .map(|invocations| EnrichmentCandidatePlan {
                enrichment_request_id: request.request_id.clone(),
                invocations,
            })
            .collect::<Vec<_>>();
        let pareto = candidates
            .iter()
            .enumerate()
            .filter(|(index, candidate)| {
                !candidates.iter().enumerate().any(|(other_index, other)| {
                    other_index != *index && plan_dominates(other, candidate)
                })
            })
            .map(|(_, candidate)| candidate.clone())
            .collect();
        Ok(EnrichmentPlanningOutcome::CandidatePlans { pareto })
    }

    /// Executes one returned plan and verifies its deltas by canonical materialization.
    pub fn execute(
        &mut self,
        request: &EnrichmentRequest,
        plan: &EnrichmentCandidatePlan,
        source_bytes: Vec<u8>,
        document: &Document,
        manifest: &DocumentStateManifest,
    ) -> Result<EnrichmentExecution, RuntimeError> {
        validate_request(request, document, manifest)?;
        if plan.enrichment_request_id != request.request_id || plan.invocations.is_empty() {
            return Err(RuntimeError::InvalidEnrichment(
                "selected plan does not belong to this request or has no invocations".into(),
            ));
        }
        let offered = match self.plan(request, document, manifest)? {
            EnrichmentPlanningOutcome::CandidatePlans { pareto } => pareto,
            _ => Vec::new(),
        };
        if !offered.iter().any(|candidate| candidate == plan) {
            return Err(RuntimeError::InvalidEnrichment(
                "selected plan is not in the current admissible frontier".into(),
            ));
        }
        let resolver = OneBlob::from_scoped(request.source.clone(), source_bytes)?;
        let mut resource_runtime = ResourceRuntime::new(
            self.options.clone(),
            self.inventory.clone(),
            self.cache.clone(),
        )?;
        let mut deltas = Vec::new();
        let mut durable_reuse = Vec::new();
        for invocation in &plan.invocations {
            let registered = self.stages.get_mut(&invocation.stage_id).ok_or_else(|| {
                RuntimeError::InvalidEnrichment(format!(
                    "selected stage {} is no longer registered",
                    invocation.stage_id
                ))
            })?;
            if registered.engine.descriptor().id != invocation.engine_id
                || registered.descriptor.produces != invocation.capability
                || invocation.request.scope.as_ref() != Some(&invocation.scope)
            {
                return Err(RuntimeError::InvalidEnrichment(
                    "selected invocation differs from the registered stage".into(),
                ));
            }
            let descriptor = registered.engine.descriptor().clone();
            let key_parameters = cache_parameters(&invocation.request);
            let key = refinement_key(RefinementKeyInput {
                stage: &registered.descriptor.id,
                stage_build: registered.descriptor.build,
                model_digest: registered.descriptor.model_digest,
                source_pdf_sha256: manifest.source_pdf_sha256,
                input_state_id: &request.input_state_id,
                engine_id: &descriptor.id,
                engine_version: &descriptor.version,
                schema_version: manifest.ir_schema,
                parameters: &key_parameters,
            })?;
            let cacheability = if descriptor.deterministic {
                Cacheability::Deterministic
            } else if let Some(seed) = invocation.request.deterministic_seed {
                Cacheability::Seeded { seed }
            } else {
                Cacheability::Uncacheable {
                    reason: "engine is nondeterministic and no deterministic seed is present"
                        .into(),
                }
            };
            let durable_hit = if matches!(cacheability, Cacheability::Uncacheable { .. }) {
                None
            } else {
                self.durable
                    .as_ref()
                    .map(|store| store.get_refinement(&key))
                    .transpose()?
                    .flatten()
            };
            let delta = if let Some(delta) = durable_hit {
                validate_cached_delta(
                    &delta,
                    manifest,
                    &registered.descriptor,
                    &descriptor.id,
                    &descriptor.version,
                    invocation,
                )?;
                resource_runtime.records.push(StageExecutionRecord {
                    page_index: invocation.request.page_index,
                    stage: registered.descriptor.id.clone(),
                    engine_id: invocation.candidate.engine_id.clone(),
                    device: invocation.candidate.device.clone(),
                    reservation: None,
                    cache: CacheDecision::Hit,
                    measurement: crate::scheduler::LeaseMeasurement::default(),
                });
                durable_reuse.push(DurableReuseRecord {
                    stage: registered.descriptor.id.clone(),
                    key_sha256: key.digest()?,
                    cache: CacheDecision::Hit,
                });
                delta
            } else {
                let response = execute_controlled(
                    registered.engine.as_mut(),
                    invocation.request.clone(),
                    &resolver,
                    planner::ModelAvailability::NotRequired,
                    &registered.descriptor.id,
                    &mut resource_runtime,
                )?;
                if response.request_id != invocation.request.request_id {
                    return Err(RuntimeError::InvalidEnrichment(
                        "engine response has the wrong request identity".into(),
                    ));
                }
                let delta = delta_from_response(
                    manifest,
                    document,
                    &registered.descriptor,
                    &descriptor.id,
                    &descriptor.version,
                    invocation,
                    response.evidence,
                )?;
                if let Some(store) = &self.durable
                    && !matches!(cacheability, Cacheability::Uncacheable { .. })
                {
                    store.put_refinement(&key, &cacheability, &delta)?;
                }
                durable_reuse.push(DurableReuseRecord {
                    stage: registered.descriptor.id.clone(),
                    key_sha256: key.digest()?,
                    cache: if self.durable.is_some() {
                        if matches!(cacheability, Cacheability::Uncacheable { .. }) {
                            CacheDecision::Uncacheable
                        } else {
                            CacheDecision::Miss
                        }
                    } else {
                        CacheDecision::NotConfigured
                    },
                });
                delta
            };
            deltas.push(delta);
        }

        let base_state_id = manifest.id()?;
        let mut state_manifest = manifest.clone();
        state_manifest.materialized_ir_checkpoint = None;
        state_manifest.parent_state_ids = BTreeSet::from([base_state_id]);
        for delta in &deltas {
            state_manifest.evidence_delta_ids.insert(delta.id()?);
            state_manifest.coverage.extend(delta.coverage_delta.clone());
        }
        let materialized =
            materialize_from_checkpoint(document, manifest, &deltas, &state_manifest)?;
        let checkpoint_bytes = materialized.to_canonical_json()?;
        let checkpoint_digest = Sha256Digest::of_bytes(&checkpoint_bytes);
        state_manifest.materialized_ir_checkpoint = Some(MaterializedIrCheckpoint {
            document_ir_logical_sha256: checkpoint_digest,
            artifact_id: ArtifactId::derive(&[
                b"ferrodoc-enrichment-checkpoint/1",
                checkpoint_digest.as_bytes(),
            ]),
            representation: "application/vnd.ferrodoc.document-ir+json;version=1".into(),
        });
        let persist_checkpoint = self.durable.is_none()
            || self
                .checkpoint_policy
                .should_checkpoint(CheckpointPolicyContext {
                    state_delta_count: state_manifest.evidence_delta_ids.len(),
                    tail_delta_count: deltas.len(),
                    canonical_document_bytes: checkpoint_bytes.len() as u64,
                });
        if !persist_checkpoint {
            state_manifest.materialized_ir_checkpoint = None;
        }
        let durable_artifacts = self
            .durable
            .as_ref()
            .map(|store| {
                let checkpoint = persist_checkpoint
                    .then(|| store.persist_checkpoint(&materialized))
                    .transpose()?;
                state_manifest.materialized_ir_checkpoint =
                    checkpoint
                        .as_ref()
                        .map(|checkpoint| MaterializedIrCheckpoint {
                            document_ir_logical_sha256: checkpoint_digest,
                            artifact_id: checkpoint.artifact_id.clone(),
                            representation: checkpoint.representation.clone(),
                        });
                let deltas = deltas
                    .iter()
                    .map(|delta| store.persist_delta(delta))
                    .collect::<Result<Vec<_>, _>>()?;
                let state_manifest_artifact = store.persist_manifest(&state_manifest)?;
                Ok::<_, crate::durable::DurableError>(DurableExecutionArtifacts {
                    deltas,
                    state_manifest: state_manifest_artifact,
                    checkpoint,
                })
            })
            .transpose()?;
        Ok(EnrichmentExecution {
            deltas,
            state_manifest,
            document: materialized,
            resources: ResourceExecutionTrace {
                stages: resource_runtime.records,
            },
            durable_artifacts,
            durable_reuse,
        })
    }
}

fn cache_parameters(request: &EngineRequest) -> BTreeMap<String, serde_json::Value> {
    let mut parameters = request.parameters.clone();
    if let Some(page_index) = request.page_index {
        parameters.insert("ferrodoc.page_index".into(), serde_json::json!(page_index));
    }
    if let Some(seed) = request.deterministic_seed {
        parameters.insert(
            "ferrodoc.deterministic_seed".into(),
            serde_json::json!(seed),
        );
    }
    parameters
}

fn validate_cached_delta(
    delta: &EvidenceDelta,
    manifest: &DocumentStateManifest,
    stage: &EnrichmentStageDescriptor,
    engine_id: &str,
    engine_version: &str,
    invocation: &EnrichmentInvocation,
) -> Result<(), RuntimeError> {
    let configuration_digest = Sha256Digest::of_bytes(
        &serde_json::to_vec(&evidence_parameters(&invocation.request))
            .map_err(|error| RuntimeError::InvalidEnrichment(error.to_string()))?,
    );
    if delta.source_pdf_sha256 != manifest.source_pdf_sha256
        || delta.ir_schema != manifest.ir_schema
        || delta.input_state_id.as_ref() != Some(&manifest.id()?)
        || delta.stage != stage.stage
        || delta.scope != invocation.scope
        || delta.producer.name != engine_id
        || delta.producer.version != engine_version
        || delta.producer.build != stage.build
        || delta.producer.model_digest != stage.model_digest
        || delta.producer.configuration_digest != configuration_digest
    {
        return Err(RuntimeError::InvalidEnrichment(
            "durable cached delta differs from the pinned source/state/stage/scope/producer".into(),
        ));
    }
    delta.to_canonical_json()?;
    Ok(())
}

fn validate_request(
    request: &EnrichmentRequest,
    document: &Document,
    manifest: &DocumentStateManifest,
) -> Result<(), RuntimeError> {
    let state_id = manifest.id()?;
    if request.goals.is_empty()
        || request.input_state_id != state_id
        || manifest.state_schema != DOCUMENT_STATE_SCHEMA
        || manifest.source_pdf_sha256 != document.input_digest
        || manifest.ir_schema != document.schema_version
        || request.source.expected_digest != Some(manifest.source_pdf_sha256)
    {
        return Err(RuntimeError::InvalidEnrichment(
            "request, source blob, materialized document, and base state do not agree".into(),
        ));
    }
    for goal in &request.goals {
        validate_scope(&goal.scope, document)?;
    }
    Ok(())
}

fn validate_scope(scope: &RefinementScope, document: &Document) -> Result<(), RuntimeError> {
    match scope {
        RefinementScope::Document => Ok(()),
        RefinementScope::Pages { page_ids } if page_ids.is_empty() => Err(
            RuntimeError::InvalidEnrichment("page scope is empty".into()),
        ),
        RefinementScope::Pages { page_ids } => {
            if page_ids
                .iter()
                .all(|id| document.pages.iter().any(|page| &page.id == id))
            {
                Ok(())
            } else {
                Err(RuntimeError::InvalidEnrichment(
                    "page scope contains an absent page".into(),
                ))
            }
        }
        RefinementScope::Regions { regions } if regions.is_empty() => Err(
            RuntimeError::InvalidEnrichment("region scope is empty".into()),
        ),
        RefinementScope::Regions { regions } => {
            for target in regions {
                let page = document
                    .pages
                    .iter()
                    .find(|page| page.id == target.page_id)
                    .ok_or_else(|| {
                        RuntimeError::InvalidEnrichment(
                            "region scope contains an absent containing page".into(),
                        )
                    })?;
                if !page
                    .regions
                    .iter()
                    .any(|region| region.id == target.region_id)
                {
                    return Err(RuntimeError::InvalidEnrichment(
                        "region scope contains an absent region on its qualified page".into(),
                    ));
                }
            }
            Ok(())
        }
    }
}

fn atomic_scopes(scope: &RefinementScope) -> Vec<RefinementScope> {
    match scope {
        RefinementScope::Document => vec![RefinementScope::Document],
        RefinementScope::Pages { page_ids } => page_ids
            .iter()
            .cloned()
            .map(|page_id| RefinementScope::Pages {
                page_ids: BTreeSet::from([page_id]),
            })
            .collect(),
        RefinementScope::Regions { regions } => regions
            .iter()
            .cloned()
            .map(|region| RefinementScope::Regions {
                regions: BTreeSet::from([region]),
            })
            .collect(),
    }
}

fn scope_page_index(
    scope: &RefinementScope,
    document: &Document,
) -> Result<Option<u32>, RuntimeError> {
    let page_id: Option<&PageId> = match scope {
        RefinementScope::Document => None,
        RefinementScope::Pages { page_ids } => page_ids.iter().next(),
        RefinementScope::Regions { regions } => regions.iter().next().map(|region| &region.page_id),
    };
    page_id
        .map(|page_id| {
            document
                .pages
                .iter()
                .find(|page| &page.id == page_id)
                .map(|page| page.index)
                .ok_or_else(|| RuntimeError::InvalidEnrichment("scope page is absent".into()))
        })
        .transpose()
}

fn scope_source_text(
    scope: &RefinementScope,
    document: &Document,
) -> Result<Vec<SourceTextEvidence>, RuntimeError> {
    let RefinementScope::Regions { regions } = scope else {
        return Ok(Vec::new());
    };
    let target = regions
        .iter()
        .next()
        .ok_or_else(|| RuntimeError::InvalidEnrichment("region scope is empty".into()))?;
    let page = document
        .pages
        .iter()
        .find(|page| page.id == target.page_id)
        .ok_or_else(|| RuntimeError::InvalidEnrichment("scope page is absent".into()))?;
    let region = page
        .regions
        .iter()
        .find(|region| region.id == target.region_id)
        .ok_or_else(|| RuntimeError::InvalidEnrichment("scope region is absent".into()))?;
    Ok(region
        .evidence
        .iter()
        .filter_map(|evidence| match &evidence.content {
            EvidenceContent::Text { text } => Some(SourceTextEvidence {
                evidence_id: evidence.id.clone(),
                text: text.clone(),
                geometry: evidence.geometry,
                geometry_quality: evidence.geometry_quality,
            }),
            _ => None,
        })
        .collect())
}

fn coverage_complete(
    coverage: &[CoverageEntry],
    capability: Capability,
    requested: &RefinementScope,
) -> bool {
    coverage.iter().any(|entry| {
        entry.capability == capability
            && entry.status == "complete"
            && scope_covers(&entry.scope, requested)
    })
}

fn scope_covers(covered: &RefinementScope, requested: &RefinementScope) -> bool {
    match (covered, requested) {
        (RefinementScope::Document, _) => true,
        (RefinementScope::Pages { page_ids: covered }, RefinementScope::Pages { page_ids }) => {
            page_ids.is_subset(covered)
        }
        (RefinementScope::Pages { page_ids }, RefinementScope::Regions { regions }) => regions
            .iter()
            .all(|region| page_ids.contains(&region.page_id)),
        (RefinementScope::Regions { regions: covered }, RefinementScope::Regions { regions }) => {
            regions.is_subset(covered)
        }
        _ => false,
    }
}

fn scope_label(scope: &RefinementScope) -> &'static str {
    match scope {
        RefinementScope::Document => "document",
        RefinementScope::Pages { .. } => "page",
        RefinementScope::Regions { .. } => "page-qualified region",
    }
}

#[derive(Clone, Copy)]
struct PlanMetrics {
    peak_ram: Option<u64>,
    warm_ram: Option<u64>,
    peak_vram: Option<u64>,
    warm_vram: Option<u64>,
    latency: Option<u64>,
    remote_cost: Option<u64>,
    quality: Option<f64>,
}

fn plan_metrics(plan: &EnrichmentCandidatePlan) -> PlanMetrics {
    let estimates = plan
        .invocations
        .iter()
        .map(|invocation| &invocation.candidate.resources)
        .collect::<Vec<_>>();
    let maximum = |values: Vec<Option<u64>>| {
        values
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .and_then(|values| values.into_iter().max())
    };
    let sum = |values: Vec<Option<u64>>| {
        values
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .map(|values| {
                values
                    .into_iter()
                    .fold(0_u64, |total, value| total.saturating_add(value))
            })
    };
    PlanMetrics {
        peak_ram: maximum(
            estimates
                .iter()
                .map(|item| item.peak_ram.known().map(|value| value.get()))
                .collect(),
        ),
        warm_ram: maximum(
            estimates
                .iter()
                .map(|item| item.warm_ram.known().map(|value| value.get()))
                .collect(),
        ),
        peak_vram: maximum(
            estimates
                .iter()
                .map(|item| item.peak_vram.known().map(|value| value.get()))
                .collect(),
        ),
        warm_vram: maximum(
            estimates
                .iter()
                .map(|item| item.warm_vram.known().map(|value| value.get()))
                .collect(),
        ),
        latency: sum(estimates
            .iter()
            .map(|item| item.latency.known().map(|value| value.get()))
            .collect()),
        remote_cost: sum(estimates
            .iter()
            .map(|item| item.remote_cost.known().map(|value| value.get()))
            .collect()),
        quality: estimates
            .iter()
            .map(|item| item.quality.known().map(|value| value.get()))
            .collect::<Option<Vec<_>>>()
            .and_then(|values| values.into_iter().reduce(f64::min)),
    }
}

fn plan_dominates(left: &EnrichmentCandidatePlan, right: &EnrichmentCandidatePlan) -> bool {
    let left = plan_metrics(left);
    let right = plan_metrics(right);
    let costs = [
        (left.peak_ram, right.peak_ram),
        (left.warm_ram, right.warm_ram),
        (left.peak_vram, right.peak_vram),
        (left.warm_vram, right.warm_vram),
        (left.latency, right.latency),
        (left.remote_cost, right.remote_cost),
    ];
    let Some(quality) = left.quality.zip(right.quality) else {
        return false;
    };
    if costs
        .iter()
        .any(|(left, right)| left.zip(*right).is_none_or(|(left, right)| left > right))
        || quality.0 < quality.1
    {
        return false;
    }
    costs
        .iter()
        .any(|(left, right)| left.zip(*right).is_some_and(|(left, right)| left < right))
        || quality.0 > quality.1
}

fn delta_from_response(
    manifest: &DocumentStateManifest,
    document: &Document,
    stage: &EnrichmentStageDescriptor,
    engine_id: &str,
    engine_version: &str,
    invocation: &EnrichmentInvocation,
    evidence: Vec<ferrodoc_ir::Evidence>,
) -> Result<EvidenceDelta, RuntimeError> {
    let configuration_digest = Sha256Digest::of_bytes(
        &serde_json::to_vec(&evidence_parameters(&invocation.request))
            .map_err(|error| RuntimeError::InvalidEnrichment(error.to_string()))?,
    );
    if evidence.iter().any(|item| {
        item.provenance.schema_version != manifest.ir_schema
            || item.provenance.input_digest != manifest.source_pdf_sha256
            || item.provenance.engine_id != engine_id
            || item.provenance.engine_version != engine_version
            || item.provenance.parameters != evidence_parameters(&invocation.request)
            || item.provenance.stage != stage.stage
            || item.provenance.model_digest != stage.model_digest
    }) {
        return Err(RuntimeError::InvalidEnrichment(
            "engine evidence provenance differs from the scoped invocation".into(),
        ));
    }
    if invocation.capability == Capability::TableRecognize
        && evidence
            .iter()
            .any(|item| !matches!(item.content, EvidenceContent::Table { .. }))
    {
        return Err(RuntimeError::InvalidEnrichment(
            "table recognition returned non-table evidence".into(),
        ));
    }
    let mut required_evidence_ids = BTreeSet::new();
    let mut page_additions = Vec::new();
    if !evidence.is_empty() {
        let target = match &invocation.scope {
            RefinementScope::Regions { regions } if regions.len() == 1 => {
                regions.iter().next().expect("singleton region scope")
            }
            _ => {
                return Err(RuntimeError::InvalidEnrichment(
                    "evidence-producing FP1 stages require a page-qualified region owner".into(),
                ));
            }
        };
        let page = document
            .pages
            .iter()
            .find(|page| page.id == target.page_id)
            .expect("validated target page");
        let region = page
            .regions
            .iter()
            .find(|region| region.id == target.region_id)
            .expect("validated target region");
        let target_text_ids = region
            .evidence
            .iter()
            .filter_map(|item| {
                matches!(item.content, EvidenceContent::Text { .. }).then_some(&item.id)
            })
            .collect::<BTreeSet<_>>();
        for span in evidence.iter().flat_map(|item| match &item.content {
            EvidenceContent::Table { cells, .. } => cells
                .iter()
                .flat_map(|cell| &cell.source_spans)
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        }) {
            if !target_text_ids.contains(&span.evidence_id) {
                return Err(RuntimeError::InvalidEnrichment(
                    "table cell source span does not reference text in its qualified target region"
                        .into(),
                ));
            }
            required_evidence_ids.insert(span.evidence_id.clone());
        }
        if evidence.iter().any(|item| {
            item.geometry
                .as_ref()
                .is_some_and(|geometry| geometry.page_index != page.index)
        }) {
            return Err(RuntimeError::InvalidEnrichment(
                "engine evidence geometry lies on a different page than its qualified target"
                    .into(),
            ));
        }
        let existing_layers = page
            .layers
            .iter()
            .map(|layer| layer.id.clone())
            .collect::<BTreeSet<_>>();
        let mut new_layers = BTreeMap::<LayerId, SourceLayer>::new();
        for item in &evidence {
            if !existing_layers.contains(&item.layer_id) {
                let layer = SourceLayer {
                    id: item.layer_id.clone(),
                    kind: SourceLayerKind::Unknown(invocation.capability.to_string()),
                    provenance: item.provenance.clone(),
                };
                if new_layers
                    .insert(item.layer_id.clone(), layer.clone())
                    .is_some_and(|prior| prior != layer)
                {
                    return Err(RuntimeError::InvalidEnrichment(
                        "engine reused a layer ID with incompatible provenance".into(),
                    ));
                }
            }
        }
        page_additions.push(PageDelta {
            page_id: target.page_id.clone(),
            source_layers: new_layers
                .into_values()
                .map(|layer| OwnedSourceLayer {
                    owner: LayerOwner::Region {
                        page_id: target.page_id.clone(),
                        region_id: target.region_id.clone(),
                    },
                    layer,
                })
                .collect(),
            render_artifacts: Vec::new(),
            regions: Vec::new(),
            region_evidence: vec![RegionEvidenceAddition {
                region_id: target.region_id.clone(),
                evidence,
            }],
            reading_order_edges: Vec::new(),
        });
    }
    let has_evidence = page_additions
        .iter()
        .any(|addition| !addition.region_evidence.is_empty());
    Ok(EvidenceDelta {
        delta_schema: EVIDENCE_DELTA_SCHEMA.into(),
        source_pdf_sha256: manifest.source_pdf_sha256,
        ir_schema: manifest.ir_schema,
        stage: stage.stage,
        producer: DeltaProducer {
            name: engine_id.into(),
            version: engine_version.into(),
            build: stage.build,
            model_digest: stage.model_digest,
            configuration_digest,
        },
        scope: invocation.scope.clone(),
        input_state_id: Some(manifest.id()?),
        required_evidence_ids,
        new_pages: Vec::new(),
        page_additions,
        selection_hints: Vec::new(),
        diagnostics: Vec::new(),
        coverage_delta: vec![CoverageEntry {
            capability: invocation.capability,
            scope: invocation.scope.clone(),
            status: if has_evidence {
                "complete"
            } else {
                "candidate"
            }
            .into(),
        }],
    })
}
