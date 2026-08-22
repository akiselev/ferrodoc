//! Immutable evidence deltas and content-identifiable document states.

use std::collections::{BTreeMap, BTreeSet};

use ferrodoc_core::{
    ArtifactId, Capability, DocumentStateId, EvidenceDeltaId, EvidenceId, PageId, RegionId,
    SchemaVersion, Sha256Digest, Stage,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    Document, Evidence, IrError, Page, ReadingOrderEdge, Region, RenderArtifact, SelectedView,
    SourceLayer,
};

/// Persistent evidence-delta schema.
pub const EVIDENCE_DELTA_SCHEMA: &str = "ferrodoc-evidence-delta/1";
/// Persistent document-state schema.
pub const DOCUMENT_STATE_SCHEMA: &str = "ferrodoc-document-state/1";

/// Deterministic producer identity for one refinement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeltaProducer {
    /// Producer name.
    pub name: String,
    /// Producer semantic version.
    pub version: String,
    /// Immutable source/build identity.
    pub build: Sha256Digest,
    /// Optional model identity.
    pub model_digest: Option<Sha256Digest>,
    /// Normalized configuration identity.
    pub configuration_digest: Sha256Digest,
}

/// A page-qualified region address. Region IDs are only page-local.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct PageRegionRef {
    /// Containing page.
    pub page_id: PageId,
    /// Page-local region.
    pub region_id: RegionId,
}

/// Scope actually processed by a refinement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RefinementScope {
    /// Whole document.
    Document,
    /// Explicit pages.
    Pages { page_ids: BTreeSet<PageId> },
    /// Explicit page-qualified regions.
    Regions { regions: BTreeSet<PageRegionRef> },
}

/// Owner of a source layer added to a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayerOwner {
    /// Layer describes the entire containing page.
    Page { page_id: PageId },
    /// Layer was produced for one page-qualified region.
    Region {
        page_id: PageId,
        region_id: RegionId,
    },
}

/// A source layer with explicit page/region ownership.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OwnedSourceLayer {
    /// Explicit owner.
    pub owner: LayerOwner,
    /// Immutable layer.
    pub layer: SourceLayer,
}

/// Evidence appended to one page-qualified region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegionEvidenceAddition {
    /// Page-local region receiving the evidence.
    pub region_id: RegionId,
    /// Append-only evidence records.
    pub evidence: Vec<Evidence>,
}

/// Reconciliation input for a page-qualified region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelectionHint {
    /// Region receiving the hint.
    pub region: PageRegionRef,
    /// Proposed deterministic selected view.
    pub selected: SelectedView,
}

/// Deterministic diagnostic retained with a delta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeltaDiagnostic {
    /// Machine-readable diagnostic code.
    pub code: String,
    /// Bounded deterministic message.
    pub message: String,
}

/// One logical capability coverage observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CoverageEntry {
    /// Capability covered.
    pub capability: Capability,
    /// Scope of the observation.
    pub scope: RefinementScope,
    /// Deterministic coverage status or qualifier.
    pub status: String,
}

/// Additions owned by one existing page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PageDelta {
    /// Existing page receiving additions.
    pub page_id: PageId,
    /// Page- or region-owned source layers.
    #[serde(default)]
    pub source_layers: Vec<OwnedSourceLayer>,
    /// Page-owned render artifacts.
    #[serde(default)]
    pub render_artifacts: Vec<RenderArtifact>,
    /// New page-owned semantic regions.
    #[serde(default)]
    pub regions: Vec<Region>,
    /// Evidence additions grouped by their page-qualified region owner.
    #[serde(default)]
    pub region_evidence: Vec<RegionEvidenceAddition>,
    /// Page-owned reading-order edges.
    #[serde(default)]
    pub reading_order_edges: Vec<ReadingOrderEdge>,
}

/// Immutable append-only evidence produced by one deterministic refinement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceDelta {
    /// Persistent delta schema tag.
    pub delta_schema: String,
    /// Exact source PDF identity.
    pub source_pdf_sha256: Sha256Digest,
    /// Document IR schema interpreted by this delta.
    pub ir_schema: SchemaVersion,
    /// Logical pipeline stage.
    pub stage: Stage,
    /// Deterministic producer identity.
    pub producer: DeltaProducer,
    /// Scope actually processed.
    pub scope: RefinementScope,
    /// Optional state against which execution was planned. This is a precondition, not state lineage.
    pub input_state_id: Option<DocumentStateId>,
    /// Exact evidence prerequisites.
    #[serde(default)]
    pub required_evidence_ids: BTreeSet<EvidenceId>,
    /// Entire new pages, used by survey/baseline deltas.
    #[serde(default)]
    pub new_pages: Vec<Page>,
    /// Additions to existing pages.
    #[serde(default)]
    pub page_additions: Vec<PageDelta>,
    /// Deterministic reconciliation inputs.
    #[serde(default)]
    pub selection_hints: Vec<SelectionHint>,
    /// Deterministic diagnostics.
    #[serde(default)]
    pub diagnostics: Vec<DeltaDiagnostic>,
    /// Additive coverage observations.
    #[serde(default)]
    pub coverage_delta: Vec<CoverageEntry>,
}

impl EvidenceDelta {
    /// Validates the persistent envelope and returns its logical content identity.
    pub fn id(&self) -> Result<EvidenceDeltaId, IrError> {
        self.validate_envelope()?;
        let projection = (
            self.source_pdf_sha256,
            self.ir_schema,
            self.stage,
            &self.producer,
            &self.new_pages,
            &self.page_additions,
            &self.selection_hints,
        );
        let bytes = serde_json::to_vec(&projection)?;
        Ok(EvidenceDeltaId::derive(&[
            EVIDENCE_DELTA_SCHEMA.as_bytes(),
            &bytes,
        ]))
    }

    /// Digests the exact retained artifact, including preconditions, diagnostics, and coverage.
    pub fn artifact_digest(&self) -> Result<Sha256Digest, IrError> {
        Ok(Sha256Digest::of_bytes(&self.to_canonical_json()?))
    }

    /// Serializes the validated delta as deterministic compact JSON.
    pub fn to_canonical_json(&self) -> Result<Vec<u8>, IrError> {
        self.validate_envelope()?;
        Ok(serde_json::to_vec(self)?)
    }

    fn validate_envelope(&self) -> Result<(), IrError> {
        if self.delta_schema != EVIDENCE_DELTA_SCHEMA {
            return Err(IrError::Invalid("unsupported evidence-delta schema".into()));
        }
        if self.producer.name.trim().is_empty()
            || self.producer.version.trim().is_empty()
            || self
                .diagnostics
                .iter()
                .any(|item| item.code.trim().is_empty())
            || self
                .coverage_delta
                .iter()
                .any(|item| item.status.trim().is_empty())
        {
            return Err(IrError::Invalid(
                "delta producer, diagnostic, or coverage identity is empty".into(),
            ));
        }
        let page_ids: BTreeSet<_> = self.new_pages.iter().map(|page| &page.id).collect();
        if page_ids.len() != self.new_pages.len() {
            return Err(IrError::Invalid(
                "delta contains duplicate new pages".into(),
            ));
        }
        let addition_ids: BTreeSet<_> = self
            .page_additions
            .iter()
            .map(|addition| &addition.page_id)
            .collect();
        if addition_ids.len() != self.page_additions.len() {
            return Err(IrError::Invalid(
                "delta contains duplicate page additions".into(),
            ));
        }
        Ok(())
    }
}

/// Optional physical checkpoint reference. It is never part of logical state identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MaterializedIrCheckpoint {
    /// Digest of canonical DocumentIR JSON.
    pub document_ir_logical_sha256: Sha256Digest,
    /// Physical artifact identity.
    pub artifact_id: ArtifactId,
    /// Physical representation media type or codec label.
    pub representation: String,
}

/// Immutable manifest for one logical set of evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DocumentStateManifest {
    /// Persistent state schema tag.
    pub state_schema: String,
    /// Exact source PDF identity.
    pub source_pdf_sha256: Sha256Digest,
    /// Document IR schema interpreted by this state.
    pub ir_schema: SchemaVersion,
    /// Canonical set of evidence-delta identities.
    pub evidence_delta_ids: BTreeSet<EvidenceDeltaId>,
    /// Deterministic reconciliation-policy identity.
    pub reconciliation_policy_id: Sha256Digest,
    /// Retained summary, excluded from logical identity.
    #[serde(default)]
    pub coverage: Vec<CoverageEntry>,
    /// Optional physical checkpoint, excluded from logical identity.
    pub materialized_ir_checkpoint: Option<MaterializedIrCheckpoint>,
    /// Construction/merge lineage, excluded from logical identity.
    #[serde(default)]
    pub parent_state_ids: BTreeSet<DocumentStateId>,
}

impl DocumentStateManifest {
    /// Validates and derives the logical state identity projection.
    pub fn id(&self) -> Result<DocumentStateId, IrError> {
        if self.state_schema != DOCUMENT_STATE_SCHEMA {
            return Err(IrError::Invalid("unsupported document-state schema".into()));
        }
        let projection = (
            DOCUMENT_STATE_SCHEMA,
            self.source_pdf_sha256,
            self.ir_schema,
            &self.evidence_delta_ids,
            self.reconciliation_policy_id,
        );
        Ok(DocumentStateId::derive(&[
            DOCUMENT_STATE_SCHEMA.as_bytes(),
            &serde_json::to_vec(&projection)?,
        ]))
    }

    /// Serializes the retained manifest. Logical identity remains the explicit projection in `id`.
    pub fn to_canonical_json(&self) -> Result<Vec<u8>, IrError> {
        self.id()?;
        Ok(serde_json::to_vec(self)?)
    }
}

/// A page-qualified refinement request against one immutable base state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RegionRefinementTarget {
    /// Pinned base state.
    pub base_document_state_id: DocumentStateId,
    /// Containing page.
    pub page_id: PageId,
    /// Page-local region.
    pub region_id: RegionId,
    /// Requested generic Ferrodoc capabilities.
    pub requested_capabilities: BTreeSet<Capability>,
}

impl RegionRefinementTarget {
    /// Proves the state and page-qualified region exist.
    pub fn validate(&self, document: &Document, state_id: &DocumentStateId) -> Result<(), IrError> {
        if &self.base_document_state_id != state_id || self.requested_capabilities.is_empty() {
            return Err(IrError::Invalid(
                "refinement target has the wrong base state or no capabilities".into(),
            ));
        }
        let page = document
            .pages
            .iter()
            .find(|page| page.id == self.page_id)
            .ok_or_else(|| IrError::Invalid("refinement target page is absent".into()))?;
        if !page
            .regions
            .iter()
            .any(|region| region.id == self.region_id)
        {
            return Err(IrError::Invalid(
                "refinement target region is absent from the qualified page".into(),
            ));
        }
        Ok(())
    }
}

/// Materializes a state from an initial document and its complete delta set.
pub fn materialize_state(
    initial: &Document,
    deltas: &[EvidenceDelta],
    manifest: &DocumentStateManifest,
) -> Result<Document, IrError> {
    materialize_with_prefix(initial, &[], deltas, manifest)
}

/// Materializes from a canonical checkpoint plus the tail deltas not represented by it.
pub fn materialize_from_checkpoint(
    checkpoint: &Document,
    checkpoint_delta_ids: &[EvidenceDeltaId],
    tail: &[EvidenceDelta],
    manifest: &DocumentStateManifest,
) -> Result<Document, IrError> {
    materialize_with_prefix(checkpoint, checkpoint_delta_ids, tail, manifest)
}

fn materialize_with_prefix(
    initial: &Document,
    prefix_ids: &[EvidenceDeltaId],
    deltas: &[EvidenceDelta],
    manifest: &DocumentStateManifest,
) -> Result<Document, IrError> {
    initial.validate()?;
    manifest.id()?;
    if initial.input_digest != manifest.source_pdf_sha256
        || initial.schema_version != manifest.ir_schema
    {
        return Err(IrError::Invalid(
            "state source or IR schema differs from materialization input".into(),
        ));
    }
    let mut represented: BTreeSet<_> = prefix_ids.iter().cloned().collect();
    let mut document = initial.clone();
    for delta in deltas {
        represented.insert(delta.id()?);
        apply_delta(&mut document, delta)?;
    }
    if represented != manifest.evidence_delta_ids {
        return Err(IrError::Invalid(
            "materialization delta identities do not match the state manifest".into(),
        ));
    }
    canonicalize_document(&mut document);
    document.validate_evidence_grade()?;
    Ok(document)
}

fn apply_delta(document: &mut Document, delta: &EvidenceDelta) -> Result<(), IrError> {
    delta.validate_envelope()?;
    if delta.source_pdf_sha256 != document.input_digest
        || delta.ir_schema != document.schema_version
    {
        return Err(IrError::Invalid(
            "delta source or IR schema differs from the document".into(),
        ));
    }
    let existing_evidence: BTreeSet<_> = document
        .pages
        .iter()
        .flat_map(|page| &page.regions)
        .flat_map(|region| &region.evidence)
        .map(|evidence| evidence.id.clone())
        .collect();
    if !delta.required_evidence_ids.is_subset(&existing_evidence) {
        return Err(IrError::Invalid(
            "delta required evidence is absent from the input document".into(),
        ));
    }
    if !delta.new_pages.is_empty() && !matches!(delta.scope, RefinementScope::Document) {
        return Err(IrError::Invalid(
            "only a document-scoped delta may introduce pages".into(),
        ));
    }
    for addition in &delta.page_additions {
        let page_is_in_scope = match &delta.scope {
            RefinementScope::Document => true,
            RefinementScope::Pages { page_ids } => page_ids.contains(&addition.page_id),
            RefinementScope::Regions { regions } => regions
                .iter()
                .any(|region| region.page_id == addition.page_id),
        };
        if !page_is_in_scope {
            return Err(IrError::Invalid(
                "page addition lies outside the declared refinement scope".into(),
            ));
        }
    }
    for page in &delta.new_pages {
        if document
            .pages
            .iter()
            .any(|candidate| candidate.id == page.id)
        {
            return Err(IrError::Invalid("delta attempts to replace a page".into()));
        }
        document.pages.push(page.clone());
    }
    for addition in &delta.page_additions {
        let page = document
            .pages
            .iter_mut()
            .find(|page| page.id == addition.page_id)
            .ok_or_else(|| IrError::Invalid("delta page addition target is absent".into()))?;
        for owned in &addition.source_layers {
            match &owned.owner {
                LayerOwner::Page { page_id } if page_id == &page.id => {}
                LayerOwner::Region { page_id, region_id }
                    if page_id == &page.id
                        && (page.regions.iter().any(|region| &region.id == region_id)
                            || addition
                                .regions
                                .iter()
                                .any(|region| &region.id == region_id)) => {}
                _ => {
                    return Err(IrError::Invalid(
                        "source-layer owner does not resolve on its containing page".into(),
                    ));
                }
            }
            page.layers.push(owned.layer.clone());
        }
        page.artifacts.extend(addition.render_artifacts.clone());
        page.regions.extend(addition.regions.clone());
        for evidence_addition in &addition.region_evidence {
            let region = page
                .regions
                .iter_mut()
                .find(|region| region.id == evidence_addition.region_id)
                .ok_or_else(|| IrError::Invalid("evidence owner region is absent".into()))?;
            region.evidence.extend(evidence_addition.evidence.clone());
        }
        page.reading_order
            .extend(addition.reading_order_edges.clone());
    }
    for hint in &delta.selection_hints {
        let page = document
            .pages
            .iter_mut()
            .find(|page| page.id == hint.region.page_id)
            .ok_or_else(|| IrError::Invalid("selection-hint page is absent".into()))?;
        let region = page
            .regions
            .iter_mut()
            .find(|region| region.id == hint.region.region_id)
            .ok_or_else(|| IrError::Invalid("selection-hint region is absent".into()))?;
        if region
            .selected
            .as_ref()
            .is_some_and(|selected| selected != &hint.selected)
        {
            return Err(IrError::Invalid(
                "conflicting selection hints require a different reconciliation policy".into(),
            ));
        }
        region.selected = Some(hint.selected.clone());
    }
    Ok(())
}

fn canonicalize_document(document: &mut Document) {
    document
        .pages
        .sort_by(|left, right| (left.index, &left.id).cmp(&(right.index, &right.id)));
    for page in &mut document.pages {
        page.layers.sort_by(|left, right| left.id.cmp(&right.id));
        page.artifacts.sort_by(|left, right| left.id.cmp(&right.id));
        page.regions.sort_by(|left, right| left.id.cmp(&right.id));
        page.reading_order.sort();
        for region in &mut page.regions {
            region
                .evidence
                .sort_by(|left, right| left.id.cmp(&right.id));
            if let Some(selected) = &mut region.selected {
                selected.evidence_ids.sort();
            }
        }
    }
}

/// Returns all page-qualified evidence identities in a materialized state.
pub fn evidence_index(document: &Document) -> BTreeMap<PageRegionRef, BTreeSet<EvidenceId>> {
    document
        .pages
        .iter()
        .flat_map(|page| {
            page.regions.iter().map(move |region| {
                (
                    PageRegionRef {
                        page_id: page.id.clone(),
                        region_id: region.id.clone(),
                    },
                    region
                        .evidence
                        .iter()
                        .map(|evidence| evidence.id.clone())
                        .collect(),
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use ferrodoc_core::{
        BlobId, CURRENT_SCHEMA_VERSION, CoordinateSpace, CoordinateTransform,
        DeterministicProvenance, DocumentId, LayerId, MediaType, PageRect, Rect, Unit,
    };

    use super::*;
    use crate::{
        DocumentMetadata, EvidenceContent, GeometryQuality, RegionKind, TableCell, TextSourceSpan,
    };

    struct ContractFixture {
        initial: Document,
        baseline: EvidenceDelta,
        table: EvidenceDelta,
        precision: EvidenceDelta,
        baseline_manifest: DocumentStateManifest,
        final_manifest: DocumentStateManifest,
        page_id: PageId,
        other_page_id: PageId,
        region_id: RegionId,
        source_evidence_id: EvidenceId,
    }

    fn page_rect(index: u32, x: f64, y: f64, width: f64, height: f64) -> PageRect {
        PageRect {
            page_index: index,
            rect: Rect::new(x, y, width, height, CoordinateSpace::Pdf, Unit::Point).unwrap(),
            source_transform: CoordinateTransform::IDENTITY,
        }
    }

    fn producer(name: &str) -> DeltaProducer {
        DeltaProducer {
            name: name.into(),
            version: "1.0.0".into(),
            build: Sha256Digest::of_bytes(format!("{name}-build").as_bytes()),
            model_digest: None,
            configuration_digest: Sha256Digest::of_bytes(format!("{name}-config").as_bytes()),
        }
    }

    fn provenance(input: Sha256Digest, stage: Stage, engine: &str) -> DeterministicProvenance {
        DeterministicProvenance {
            schema_version: CURRENT_SCHEMA_VERSION,
            input_digest: input,
            engine_id: engine.into(),
            engine_version: "1.0.0".into(),
            model_digest: None,
            parameters: BTreeMap::new(),
            stage,
        }
    }

    fn manifest(input: Sha256Digest, deltas: &[&EvidenceDelta]) -> DocumentStateManifest {
        DocumentStateManifest {
            state_schema: DOCUMENT_STATE_SCHEMA.into(),
            source_pdf_sha256: input,
            ir_schema: CURRENT_SCHEMA_VERSION,
            evidence_delta_ids: deltas.iter().map(|delta| delta.id().unwrap()).collect(),
            reconciliation_policy_id: Sha256Digest::of_bytes(b"reconciliation-policy-v1"),
            coverage: Vec::new(),
            materialized_ir_checkpoint: None,
            parent_state_ids: BTreeSet::new(),
        }
    }

    fn fixture() -> ContractFixture {
        let input = Sha256Digest::of_bytes(b"fp0-fdx0-source-pdf");
        let document_id = DocumentId::derive(&[input.as_bytes()]);
        let page_id = PageId::derive(&[document_id.as_str().as_bytes(), b"page-0"]);
        let other_page_id = PageId::derive(&[document_id.as_str().as_bytes(), b"page-1"]);
        let region_id = RegionId::derive(&[page_id.as_str().as_bytes(), b"shared-local-id"]);
        let source_layer_id = LayerId::derive(&[page_id.as_str().as_bytes(), b"native"]);
        let source_evidence_id =
            EvidenceId::derive(&[region_id.as_str().as_bytes(), b"source-voltage"]);
        let bounds = page_rect(0, 0.0, 0.0, 612.0, 792.0);
        let region_geometry = page_rect(0, 72.0, 72.0, 240.0, 24.0);
        let initial = Document {
            schema_version: CURRENT_SCHEMA_VERSION,
            id: document_id,
            input_digest: input,
            metadata: DocumentMetadata::default(),
            pages: Vec::new(),
        };
        let baseline = EvidenceDelta {
            delta_schema: EVIDENCE_DELTA_SCHEMA.into(),
            source_pdf_sha256: input,
            ir_schema: CURRENT_SCHEMA_VERSION,
            stage: Stage::NativeExtract,
            producer: producer("native-baseline"),
            scope: RefinementScope::Document,
            input_state_id: None,
            required_evidence_ids: BTreeSet::new(),
            new_pages: vec![
                Page {
                    id: page_id.clone(),
                    index: 0,
                    bounds,
                    layers: vec![SourceLayer {
                        id: source_layer_id.clone(),
                        kind: crate::SourceLayerKind::NativePdf,
                        provenance: provenance(input, Stage::NativeExtract, "native-pdf"),
                    }],
                    artifacts: Vec::new(),
                    regions: vec![Region {
                        id: region_id.clone(),
                        kind: RegionKind::Paragraph,
                        geometry: region_geometry,
                        evidence: vec![Evidence {
                            id: source_evidence_id.clone(),
                            layer_id: source_layer_id,
                            content: EvidenceContent::Text { text: "5 V".into() },
                            geometry: Some(region_geometry),
                            geometry_quality: GeometryQuality::Line,
                            confidence: None,
                            provenance: provenance(input, Stage::NativeExtract, "native-pdf"),
                            engine_metadata: BTreeMap::new(),
                        }],
                        selected: None,
                    }],
                    reading_order: Vec::new(),
                },
                Page {
                    id: other_page_id.clone(),
                    index: 1,
                    bounds: page_rect(1, 0.0, 0.0, 612.0, 792.0),
                    layers: Vec::new(),
                    artifacts: Vec::new(),
                    regions: Vec::new(),
                    reading_order: Vec::new(),
                },
            ],
            page_additions: Vec::new(),
            selection_hints: Vec::new(),
            diagnostics: Vec::new(),
            coverage_delta: Vec::new(),
        };
        let baseline_manifest = manifest(input, &[&baseline]);
        let baseline_state_id = baseline_manifest.id().unwrap();
        let table_region_id = RegionId::derive(&[page_id.as_str().as_bytes(), b"table"]);
        let table_layer_id = LayerId::derive(&[table_region_id.as_str().as_bytes(), b"table"]);
        let raster_layer_id = LayerId::derive(&[page_id.as_str().as_bytes(), b"raster"]);
        let table_evidence_id =
            EvidenceId::derive(&[table_region_id.as_str().as_bytes(), b"table-evidence"]);
        let artifact_id = ArtifactId::derive(&[page_id.as_str().as_bytes(), b"render-144dpi"]);
        let table_geometry = page_rect(0, 72.0, 120.0, 240.0, 48.0);
        let table = EvidenceDelta {
            delta_schema: EVIDENCE_DELTA_SCHEMA.into(),
            source_pdf_sha256: input,
            ir_schema: CURRENT_SCHEMA_VERSION,
            stage: Stage::Layout,
            producer: producer("table-refinement"),
            scope: RefinementScope::Regions {
                regions: BTreeSet::from([PageRegionRef {
                    page_id: page_id.clone(),
                    region_id: region_id.clone(),
                }]),
            },
            input_state_id: Some(baseline_state_id.clone()),
            required_evidence_ids: BTreeSet::from([source_evidence_id.clone()]),
            new_pages: Vec::new(),
            page_additions: vec![PageDelta {
                page_id: page_id.clone(),
                source_layers: vec![
                    OwnedSourceLayer {
                        owner: LayerOwner::Region {
                            page_id: page_id.clone(),
                            region_id: table_region_id.clone(),
                        },
                        layer: SourceLayer {
                            id: table_layer_id.clone(),
                            kind: crate::SourceLayerKind::Layout,
                            provenance: provenance(input, Stage::Layout, "table-rule"),
                        },
                    },
                    OwnedSourceLayer {
                        owner: LayerOwner::Page {
                            page_id: page_id.clone(),
                        },
                        layer: SourceLayer {
                            id: raster_layer_id.clone(),
                            kind: crate::SourceLayerKind::Raster,
                            provenance: provenance(input, Stage::Rasterize, "pdf-render"),
                        },
                    },
                ],
                render_artifacts: vec![RenderArtifact {
                    id: artifact_id.clone(),
                    blob_id: BlobId::new("page-0-render").unwrap(),
                    digest: Sha256Digest::of_bytes(b"render-bytes"),
                    media_type: MediaType::new("image/png").unwrap(),
                    width: Some(1224),
                    height: Some(1584),
                }],
                regions: vec![Region {
                    id: table_region_id.clone(),
                    kind: RegionKind::Table,
                    geometry: table_geometry,
                    evidence: Vec::new(),
                    selected: None,
                }],
                region_evidence: vec![
                    RegionEvidenceAddition {
                        region_id: table_region_id.clone(),
                        evidence: vec![Evidence {
                            id: table_evidence_id,
                            layer_id: table_layer_id,
                            content: EvidenceContent::Table {
                                rows: 1,
                                columns: 1,
                                cells: vec![TableCell {
                                    row: 0,
                                    column: 0,
                                    row_span: 1,
                                    column_span: 1,
                                    text: "5 V".into(),
                                    geometry: Some(table_geometry),
                                    geometry_quality: GeometryQuality::Region,
                                    source_spans: vec![TextSourceSpan {
                                        evidence_id: source_evidence_id.clone(),
                                        start: 0,
                                        end: 3,
                                    }],
                                }],
                            },
                            geometry: Some(table_geometry),
                            geometry_quality: GeometryQuality::Region,
                            confidence: None,
                            provenance: provenance(input, Stage::Layout, "table-rule"),
                            engine_metadata: BTreeMap::new(),
                        }],
                    },
                    RegionEvidenceAddition {
                        region_id: region_id.clone(),
                        evidence: vec![Evidence {
                            id: EvidenceId::derive(&[
                                region_id.as_str().as_bytes(),
                                b"page-render",
                            ]),
                            layer_id: raster_layer_id,
                            content: EvidenceContent::Image {
                                artifact: artifact_id,
                                alt: None,
                            },
                            geometry: Some(bounds),
                            geometry_quality: GeometryQuality::PageOnly,
                            confidence: None,
                            provenance: provenance(input, Stage::Rasterize, "pdf-render"),
                            engine_metadata: BTreeMap::new(),
                        }],
                    },
                ],
                reading_order_edges: vec![ReadingOrderEdge {
                    before: region_id.clone(),
                    after: table_region_id,
                }],
            }],
            selection_hints: Vec::new(),
            diagnostics: Vec::new(),
            coverage_delta: vec![CoverageEntry {
                capability: Capability::TableRecognize,
                scope: RefinementScope::Regions {
                    regions: BTreeSet::from([PageRegionRef {
                        page_id: page_id.clone(),
                        region_id: region_id.clone(),
                    }]),
                },
                status: "satisfied".into(),
            }],
        };
        let precision_layer_id = LayerId::derive(&[region_id.as_str().as_bytes(), b"precision"]);
        let precision = EvidenceDelta {
            delta_schema: EVIDENCE_DELTA_SCHEMA.into(),
            source_pdf_sha256: input,
            ir_schema: CURRENT_SCHEMA_VERSION,
            stage: Stage::Ocr,
            producer: producer("precision-refinement"),
            scope: RefinementScope::Regions {
                regions: BTreeSet::from([PageRegionRef {
                    page_id: page_id.clone(),
                    region_id: region_id.clone(),
                }]),
            },
            input_state_id: Some(baseline_state_id),
            required_evidence_ids: BTreeSet::new(),
            new_pages: Vec::new(),
            page_additions: vec![PageDelta {
                page_id: page_id.clone(),
                source_layers: vec![OwnedSourceLayer {
                    owner: LayerOwner::Region {
                        page_id: page_id.clone(),
                        region_id: region_id.clone(),
                    },
                    layer: SourceLayer {
                        id: precision_layer_id.clone(),
                        kind: crate::SourceLayerKind::Ocr,
                        provenance: provenance(input, Stage::Ocr, "precision-ocr"),
                    },
                }],
                render_artifacts: Vec::new(),
                regions: Vec::new(),
                region_evidence: vec![RegionEvidenceAddition {
                    region_id: region_id.clone(),
                    evidence: vec![Evidence {
                        id: EvidenceId::derive(&[region_id.as_str().as_bytes(), b"precision"]),
                        layer_id: precision_layer_id,
                        content: EvidenceContent::Text { text: "5 V".into() },
                        geometry: Some(region_geometry),
                        geometry_quality: GeometryQuality::Glyph,
                        confidence: None,
                        provenance: provenance(input, Stage::Ocr, "precision-ocr"),
                        engine_metadata: BTreeMap::new(),
                    }],
                }],
                reading_order_edges: Vec::new(),
            }],
            selection_hints: Vec::new(),
            diagnostics: Vec::new(),
            coverage_delta: Vec::new(),
        };
        let final_manifest = manifest(input, &[&baseline, &table, &precision]);
        ContractFixture {
            initial,
            baseline,
            table,
            precision,
            baseline_manifest,
            final_manifest,
            page_id,
            other_page_id,
            region_id,
            source_evidence_id,
        }
    }

    #[test]
    fn full_deltas_equal_checkpoint_plus_tail_canonically() {
        let fixture = fixture();
        let full = materialize_state(
            &fixture.initial,
            &[
                fixture.baseline.clone(),
                fixture.table.clone(),
                fixture.precision.clone(),
            ],
            &fixture.final_manifest,
        )
        .unwrap();
        let checkpoint = materialize_state(
            &fixture.initial,
            std::slice::from_ref(&fixture.baseline),
            &fixture.baseline_manifest,
        )
        .unwrap();
        let from_checkpoint = materialize_from_checkpoint(
            &checkpoint,
            &[fixture.baseline.id().unwrap()],
            &[fixture.table, fixture.precision],
            &fixture.final_manifest,
        )
        .unwrap();
        assert_eq!(
            full.to_canonical_json().unwrap(),
            from_checkpoint.to_canonical_json().unwrap()
        );
    }

    #[test]
    fn independent_delta_order_has_one_state_and_materialization() {
        let fixture = fixture();
        let first = materialize_state(
            &fixture.initial,
            &[
                fixture.baseline.clone(),
                fixture.table.clone(),
                fixture.precision.clone(),
            ],
            &fixture.final_manifest,
        )
        .unwrap();
        let second = materialize_state(
            &fixture.initial,
            &[fixture.baseline, fixture.precision, fixture.table],
            &fixture.final_manifest,
        )
        .unwrap();
        assert_eq!(
            first.to_canonical_json().unwrap(),
            second.to_canonical_json().unwrap()
        );
    }

    #[test]
    fn logical_state_identity_excludes_lineage_coverage_and_checkpoint() {
        let fixture = fixture();
        let expected = fixture.final_manifest.id().unwrap();
        let mut realized = fixture.final_manifest;
        realized
            .parent_state_ids
            .insert(fixture.baseline_manifest.id().unwrap());
        realized.coverage.push(CoverageEntry {
            capability: Capability::TableRecognize,
            scope: RefinementScope::Document,
            status: "summary-only".into(),
        });
        realized.materialized_ir_checkpoint = Some(MaterializedIrCheckpoint {
            document_ir_logical_sha256: Sha256Digest::of_bytes(b"canonical-json"),
            artifact_id: ArtifactId::derive(&[b"physical-checkpoint"]),
            representation: "application/json+zstd".into(),
        });
        assert_eq!(realized.id().unwrap(), expected);
    }

    #[test]
    fn logical_delta_identity_excludes_execution_preconditions_and_summaries() {
        let fixture = fixture();
        let expected = fixture.table.id().unwrap();
        let expected_artifact = fixture.table.artifact_digest().unwrap();
        let mut realized = fixture.table;
        realized.input_state_id = Some(DocumentStateId::derive(&[b"alternate-parent"]));
        realized.required_evidence_ids.clear();
        realized.diagnostics.push(DeltaDiagnostic {
            code: "summary".into(),
            message: "different retained derivation detail".into(),
        });
        realized.coverage_delta.clear();
        assert_eq!(realized.id().unwrap(), expected);
        assert_ne!(realized.artifact_digest().unwrap(), expected_artifact);
    }

    #[test]
    fn wrong_page_for_page_local_region_is_rejected() {
        let fixture = fixture();
        let baseline = materialize_state(
            &fixture.initial,
            std::slice::from_ref(&fixture.baseline),
            &fixture.baseline_manifest,
        )
        .unwrap();
        let valid = RegionRefinementTarget {
            base_document_state_id: fixture.baseline_manifest.id().unwrap(),
            page_id: fixture.page_id.clone(),
            region_id: fixture.region_id.clone(),
            requested_capabilities: BTreeSet::from([Capability::TableRecognize]),
        };
        valid
            .validate(&baseline, &fixture.baseline_manifest.id().unwrap())
            .unwrap();
        let target = RegionRefinementTarget {
            base_document_state_id: fixture.baseline_manifest.id().unwrap(),
            page_id: fixture.other_page_id,
            region_id: fixture.region_id,
            requested_capabilities: BTreeSet::from([Capability::TableRecognize]),
        };
        assert!(
            target
                .validate(&baseline, &fixture.baseline_manifest.id().unwrap())
                .is_err()
        );
    }

    #[test]
    fn older_evidence_anchor_remains_resolvable_after_refinement() {
        let fixture = fixture();
        let refined = materialize_state(
            &fixture.initial,
            &[fixture.baseline, fixture.table, fixture.precision],
            &fixture.final_manifest,
        )
        .unwrap();
        let index = evidence_index(&refined);
        assert!(
            index[&PageRegionRef {
                page_id: fixture.page_id,
                region_id: fixture.region_id,
            }]
                .contains(&fixture.source_evidence_id)
        );
    }

    #[test]
    fn table_cells_require_exact_reconciling_text_spans() {
        let mut fixture = fixture();
        let EvidenceContent::Table { cells, .. } =
            &mut fixture.table.page_additions[0].region_evidence[0].evidence[0].content
        else {
            panic!("fixture table evidence");
        };
        cells[0].text = "invented".into();
        fixture.final_manifest = manifest(
            fixture.initial.input_digest,
            &[&fixture.baseline, &fixture.table, &fixture.precision],
        );
        let error = materialize_state(
            &fixture.initial,
            &[fixture.baseline, fixture.table, fixture.precision],
            &fixture.final_manifest,
        )
        .unwrap_err();
        assert!(error.to_string().contains("differs from its source spans"));
    }

    #[test]
    fn state_materialization_rejects_legacy_cells_without_source_spans() {
        let mut fixture = fixture();
        let EvidenceContent::Table { cells, .. } =
            &mut fixture.table.page_additions[0].region_evidence[0].evidence[0].content
        else {
            panic!("fixture table evidence");
        };
        cells[0].source_spans.clear();
        fixture.final_manifest = manifest(
            fixture.initial.input_digest,
            &[&fixture.baseline, &fixture.table, &fixture.precision],
        );
        let error = materialize_state(
            &fixture.initial,
            &[fixture.baseline, fixture.table, fixture.precision],
            &fixture.final_manifest,
        )
        .unwrap_err();
        assert!(error.to_string().contains("no source-span evidence"));
    }

    #[test]
    fn future_delta_and_state_major_versions_fail_closed() {
        let mut fixture = fixture();
        fixture.baseline.delta_schema = "ferrodoc-evidence-delta/2".into();
        assert!(fixture.baseline.id().is_err());
        fixture.final_manifest.state_schema = "ferrodoc-document-state/2".into();
        assert!(fixture.final_manifest.id().is_err());
    }
}
