//! Embedded runtime and deterministic Phase 2 conversion pipeline.

use std::collections::BTreeMap;

use ferrodoc_core::{
    ArtifactId, BlobId, BlobRange, CURRENT_SCHEMA_VERSION, Capability, CoordinateSpace,
    CoordinateTransform, DeterministicProvenance, DocumentId, EvidenceId, LayerId, MediaType,
    PageId, PageRect, Rect, RegionId, RequestId, ScopedBlob, Sha256Digest, Stage, Unit,
};
use ferrodoc_engine_api::{
    BlobResolver, CancellationToken, Engine, EngineDescriptor, EngineError, EngineErrorCategory,
    EngineRequest, EngineResponse, ExecutionContext, HealthReport, HealthRequest, TraceSink,
};
use ferrodoc_engine_ocrs::{OcrsEngine, RGBA8_MEDIA_TYPE};
use ferrodoc_ir::{
    Document, DocumentMetadata, Evidence, EvidenceContent, Page, ReadingOrderEdge, Region,
    RegionKind, RenderArtifact, SelectedView, SelectionReason, SourceLayer, SourceLayerKind,
};
use ferrodoc_layout_rulebased::RuleBasedLayoutEngine;
use ferrodoc_pdf::{PdfDocument, PdfError, PdfLimits};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod process;

pub use process::{PluginCommand, ProcessConfig, ProcessEngine};

/// Embedded runtime failure.
#[derive(Debug, Error)]
pub enum RuntimeError {
    /// Descriptor validation or engine execution failed.
    #[error(transparent)]
    Engine(#[from] EngineError),
    /// PDF acquisition or rendering failed.
    #[error(transparent)]
    Pdf(#[from] PdfError),
    /// Selected IR failed validation.
    #[error(transparent)]
    Ir(#[from] ferrodoc_ir::IrError),
    /// An engine ID was registered more than once.
    #[error("duplicate engine ID {0:?}")]
    DuplicateEngine(String),
    /// No registered engine has the requested ID.
    #[error("unknown engine ID {0:?}")]
    UnknownEngine(String),
    /// OCR is required by policy but its model pair is unavailable.
    #[error("OCR required for page {page_index}, but OCRS models are unavailable")]
    OcrUnavailable {
        /// Page which requires OCR.
        page_index: u32,
    },
}

/// Explicit registry of embedded engine implementations.
#[derive(Default)]
pub struct EmbeddedRegistry {
    engines: BTreeMap<String, Box<dyn Engine>>,
}

impl EmbeddedRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one engine after validating its descriptor.
    pub fn register(&mut self, engine: impl Engine + 'static) -> Result<(), RuntimeError> {
        engine.descriptor().validate()?;
        let id = engine.descriptor().id.clone();
        if self.engines.contains_key(&id) {
            return Err(RuntimeError::DuplicateEngine(id));
        }
        self.engines.insert(id, Box::new(engine));
        Ok(())
    }

    /// Returns descriptors in stable engine-ID order.
    pub fn descriptors(&self) -> Vec<&EngineDescriptor> {
        self.engines
            .values()
            .map(|engine| engine.descriptor())
            .collect()
    }

    /// Executes a health check on one embedded engine.
    pub fn health(
        &mut self,
        engine_id: &str,
        request: HealthRequest,
    ) -> Result<HealthReport, RuntimeError> {
        self.engines
            .get_mut(engine_id)
            .ok_or_else(|| RuntimeError::UnknownEngine(engine_id.into()))?
            .health(request)
            .map_err(Into::into)
    }

    /// Executes one request directly through the semantic engine API.
    pub fn execute(
        &mut self,
        engine_id: &str,
        request: EngineRequest,
        context: &ExecutionContext<'_>,
    ) -> Result<EngineResponse, RuntimeError> {
        self.engines
            .get_mut(engine_id)
            .ok_or_else(|| RuntimeError::UnknownEngine(engine_id.into()))?
            .execute(request, context)
            .map_err(Into::into)
    }
}

/// Deterministic conversion policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversionOptions {
    /// Minimum non-whitespace native characters needed to skip OCR.
    pub native_character_threshold: u32,
    /// Raster DPI used for OCR pages.
    pub ocr_dpi: u32,
    /// PDF parser and renderer limits.
    pub pdf_limits: PdfLimits,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            native_character_threshold: 80,
            ocr_dpi: 144,
            pdf_limits: PdfLimits::default(),
        }
    }
}

/// Whether one plan stage will execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum PlanDecision {
    /// Stage is selected.
    Selected,
    /// Stage is rejected by policy.
    Rejected,
    /// Stage is required but its dependency is unavailable.
    Unavailable,
}

/// Engine transport selected for a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Direct in-process trait call.
    Embedded,
    /// Framed isolated child process.
    Process,
}

/// One input-specific stage decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlannedStage {
    /// Page index, absent for document-wide work.
    pub page_index: Option<u32>,
    /// Stable stage name.
    pub stage: String,
    /// Selection result.
    pub decision: PlanDecision,
    /// Selected engine transport, absent for non-engine stages.
    pub execution: Option<ExecutionMode>,
    /// Deterministic explanation.
    pub explanation: String,
}

/// Actual plan derived after PDF inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversionPlan {
    /// Input digest.
    pub input_digest: Sha256Digest,
    /// Page count.
    pub page_count: u32,
    /// Ordered decisions.
    pub stages: Vec<PlannedStage>,
}

/// Deterministic trace event emitted by completed conversion work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TraceEvent {
    /// Page index, absent for document-wide work.
    pub page_index: Option<u32>,
    /// Stable event code.
    pub code: String,
    /// Engine transport used by the completed event.
    pub execution: Option<ExecutionMode>,
    /// Stable explanatory text.
    pub detail: String,
}

/// Conversion trace without timestamps or host observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversionTrace {
    /// Ordered trace events.
    pub events: Vec<TraceEvent>,
}

/// Selected IR plus the plan and trace that produced it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConversionResult {
    /// Evidence-bearing selected document.
    pub document: Document,
    /// Input-specific plan.
    pub plan: ConversionPlan,
    /// Completed deterministic events.
    pub trace: ConversionTrace,
}

/// Embedded CPU converter with an optional explicitly loaded OCR model pair.
pub struct Converter {
    layout: RuleBasedLayoutEngine,
    ocr: OcrsEngine,
    options: ConversionOptions,
}

impl Converter {
    /// Creates a converter without OCR models. Born-digital conversion remains available.
    pub fn new(options: ConversionOptions) -> Self {
        Self {
            layout: RuleBasedLayoutEngine::new(),
            ocr: OcrsEngine::without_models(),
            options,
        }
    }

    /// Creates a converter with an explicit verified OCRS model pair.
    pub fn with_ocrs_models(
        options: ConversionOptions,
        detection: Vec<u8>,
        recognition: Vec<u8>,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            layout: RuleBasedLayoutEngine::new(),
            ocr: OcrsEngine::from_model_bytes(detection, recognition)?,
            options,
        })
    }

    /// Produces an actual per-page plan without executing layout or OCR.
    pub fn plan(&mut self, bytes: &[u8]) -> Result<ConversionPlan, RuntimeError> {
        let pdf = PdfDocument::from_bytes(bytes.to_vec(), self.options.pdf_limits.clone())?;
        let ocr_ready = self.ocr.health(HealthRequest::Shallow)?.status
            == ferrodoc_engine_api::HealthStatus::Healthy;
        Ok(plan_for(&pdf, &self.options, ocr_ready))
    }

    /// Converts a PDF to validated evidence IR using embedded engines.
    pub fn convert(&mut self, bytes: Vec<u8>) -> Result<ConversionResult, RuntimeError> {
        let pdf = PdfDocument::from_bytes(bytes, self.options.pdf_limits.clone())?;
        let ocr_ready = self.ocr.health(HealthRequest::Shallow)?.status
            == ferrodoc_engine_api::HealthStatus::Healthy;
        let plan = plan_for(&pdf, &self.options, ocr_ready);
        let digest = pdf.inspection().digest;
        let document_id = DocumentId::derive(&[digest.as_bytes()]);
        let mut trace = vec![TraceEvent {
            page_index: None,
            code: "document.inspected".into(),
            execution: None,
            detail: format!(
                "{} pages accepted by PDF limits",
                pdf.inspection().pages.len()
            ),
        }];
        let mut pages = Vec::with_capacity(pdf.inspection().pages.len());
        for inspected in &pdf.inspection().pages {
            let page_index = inspected.index;
            let page_id =
                PageId::derive(&[document_id.as_str().as_bytes(), &page_index.to_be_bytes()]);
            let bounds = local_page_bounds(
                inspected.crop_box.width(),
                inspected.crop_box.height(),
                page_index,
            )?;
            let native_text = inspected
                .native_text
                .iter()
                .map(|span| span.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let native_characters = quality_characters(&native_text);
            let needs_ocr = native_characters < self.options.native_character_threshold as usize;
            trace.push(TraceEvent {
                page_index: Some(page_index),
                code: "native.quality".into(),
                execution: None,
                detail: format!(
                    "{native_characters} non-whitespace characters; threshold {}",
                    self.options.native_character_threshold
                ),
            });

            let layout_response = if native_text.is_empty() {
                None
            } else {
                Some(execute_layout(
                    &mut self.layout,
                    &native_text,
                    bounds,
                    digest,
                    page_index,
                )?)
            };
            let native_provenance = provenance(
                digest,
                "pdf.native",
                env!("CARGO_PKG_VERSION"),
                Stage::NativeExtract,
                BTreeMap::new(),
                None,
            );
            let native_layer_id = LayerId::derive(&[
                native_provenance
                    .identity_digest()
                    .map_err(ferrodoc_ir::IrError::from)?
                    .as_bytes(),
                &page_index.to_be_bytes(),
            ]);
            let mut layers = vec![SourceLayer {
                id: native_layer_id.clone(),
                kind: SourceLayerKind::NativePdf,
                provenance: native_provenance.clone(),
            }];

            let mut artifacts = Vec::new();
            let mut ocr_response = None;
            if needs_ocr {
                if !ocr_ready {
                    return Err(RuntimeError::OcrUnavailable { page_index });
                }
                let raster = pdf.render_page(page_index, self.options.ocr_dpi)?;
                let raster_digest = Sha256Digest::of_bytes(&raster.rgba);
                artifacts.push(RenderArtifact {
                    id: ArtifactId::derive(&[
                        digest.as_bytes(),
                        &page_index.to_be_bytes(),
                        raster_digest.as_bytes(),
                    ]),
                    blob_id: BlobId::new(format!("raster-{page_index}"))
                        .expect("generated blob ID"),
                    digest: raster_digest,
                    media_type: MediaType::new(RGBA8_MEDIA_TYPE).expect("static media type"),
                    width: Some(raster.width),
                    height: Some(raster.height),
                });
                ocr_response = Some(execute_ocr(
                    &mut self.ocr,
                    raster.rgba,
                    raster.width,
                    raster.height,
                    page_index,
                    self.options.ocr_dpi,
                )?);
                trace.push(TraceEvent {
                    page_index: Some(page_index),
                    code: "ocr.executed".into(),
                    execution: Some(ExecutionMode::Embedded),
                    detail: "native evidence was absent or below threshold".into(),
                });
            } else {
                trace.push(TraceEvent {
                    page_index: Some(page_index),
                    code: "ocr.rejected".into(),
                    execution: Some(ExecutionMode::Embedded),
                    detail: "native evidence met the quality threshold".into(),
                });
            }

            if let Some(response) = &layout_response
                && let Some(first) = response.evidence.first()
            {
                layers.push(SourceLayer {
                    id: first.layer_id.clone(),
                    kind: SourceLayerKind::Layout,
                    provenance: first.provenance.clone(),
                });
            }
            if let Some(response) = &ocr_response
                && let Some(first) = response.evidence.first()
            {
                layers.push(SourceLayer {
                    id: first.layer_id.clone(),
                    kind: SourceLayerKind::Ocr,
                    provenance: first.provenance.clone(),
                });
            }
            let (regions, reading_order) = reconcile_page(
                page_index,
                bounds,
                &native_text,
                &native_layer_id,
                &native_provenance,
                layout_response,
                ocr_response,
                needs_ocr,
            );
            trace.push(TraceEvent {
                page_index: Some(page_index),
                code: "evidence.reconciled".into(),
                execution: Some(ExecutionMode::Embedded),
                detail: format!("{} selected regions", regions.len()),
            });
            pages.push(Page {
                id: page_id,
                index: page_index,
                bounds,
                layers,
                artifacts,
                regions,
                reading_order,
            });
        }
        let document = Document {
            schema_version: CURRENT_SCHEMA_VERSION,
            id: document_id,
            input_digest: digest,
            metadata: DocumentMetadata::default(),
            pages,
        };
        document.validate()?;
        Ok(ConversionResult {
            document,
            plan,
            trace: ConversionTrace { events: trace },
        })
    }
}

fn plan_for(pdf: &PdfDocument, options: &ConversionOptions, ocr_ready: bool) -> ConversionPlan {
    let mut stages = vec![PlannedStage {
        page_index: None,
        stage: "pdf.inspect".into(),
        decision: PlanDecision::Selected,
        execution: None,
        explanation: "bounded parsing is required for this PDF".into(),
    }];
    for page in &pdf.inspection().pages {
        let native_characters: usize = page
            .native_text
            .iter()
            .map(|span| quality_characters(&span.text))
            .sum();
        stages.push(PlannedStage {
            page_index: Some(page.index),
            stage: "native.extract".into(),
            decision: PlanDecision::Selected,
            execution: None,
            explanation: format!("recovered {native_characters} non-whitespace characters"),
        });
        stages.push(PlannedStage {
            page_index: Some(page.index),
            stage: "layout.rulebased".into(),
            decision: if native_characters == 0 {
                PlanDecision::Rejected
            } else {
                PlanDecision::Selected
            },
            execution: Some(ExecutionMode::Embedded),
            explanation: if native_characters == 0 {
                "no native text is available for rule-based segmentation".into()
            } else {
                "native text will be segmented deterministically".into()
            },
        });
        let needs_ocr = native_characters < options.native_character_threshold as usize;
        stages.push(PlannedStage {
            page_index: Some(page.index),
            stage: "ocr.ocrs".into(),
            decision: if needs_ocr {
                if ocr_ready {
                    PlanDecision::Selected
                } else {
                    PlanDecision::Unavailable
                }
            } else {
                PlanDecision::Rejected
            },
            execution: Some(ExecutionMode::Embedded),
            explanation: if needs_ocr {
                format!(
                    "native evidence is below the {} character threshold{}",
                    options.native_character_threshold,
                    if ocr_ready {
                        ""
                    } else {
                        "; OCRS models are not loaded"
                    }
                )
            } else {
                "native evidence meets the configured threshold".into()
            },
        });
    }
    ConversionPlan {
        input_digest: pdf.inspection().digest,
        page_count: pdf.inspection().pages.len() as u32,
        stages,
    }
}

fn execute_layout(
    engine: &mut RuleBasedLayoutEngine,
    text: &str,
    bounds: PageRect,
    document_digest: Sha256Digest,
    page_index: u32,
) -> Result<EngineResponse, RuntimeError> {
    let resolver = OneBlob::new("native-text", text.as_bytes().to_vec(), "text/plain")?;
    let request = EngineRequest {
        request_id: RequestId::derive(&[
            document_digest.as_bytes(),
            b"layout",
            &page_index.to_be_bytes(),
        ]),
        capability: Capability::LayoutDetect,
        input: resolver.scoped.clone(),
        page_index: Some(page_index),
        parameters: BTreeMap::from([
            ("page_width".into(), serde_json::json!(bounds.rect.width())),
            (
                "page_height".into(),
                serde_json::json!(bounds.rect.height()),
            ),
        ]),
        deterministic_seed: None,
        deadline: None,
    };
    execute_embedded(engine, request, &resolver)
}

fn execute_ocr(
    engine: &mut OcrsEngine,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    page_index: u32,
    dpi: u32,
) -> Result<EngineResponse, RuntimeError> {
    let resolver = OneBlob::new("page-rgba", rgba, RGBA8_MEDIA_TYPE)?;
    let request = EngineRequest {
        request_id: RequestId::derive(&[
            resolver.digest.as_bytes(),
            b"ocr",
            &page_index.to_be_bytes(),
        ]),
        capability: Capability::OcrPage,
        input: resolver.scoped.clone(),
        page_index: Some(page_index),
        parameters: BTreeMap::from([
            ("width".into(), serde_json::json!(width)),
            ("height".into(), serde_json::json!(height)),
            ("dpi".into(), serde_json::json!(dpi)),
        ]),
        deterministic_seed: None,
        deadline: None,
    };
    execute_embedded(engine, request, &resolver)
}

fn execute_embedded(
    engine: &mut dyn Engine,
    request: EngineRequest,
    resolver: &OneBlob,
) -> Result<EngineResponse, RuntimeError> {
    let trace = NoTrace;
    let context = ExecutionContext {
        cancellation: CancellationToken::default(),
        deadline: None,
        blobs: resolver,
        trace: &trace,
    };
    engine.execute(request, &context).map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
fn reconcile_page(
    page_index: u32,
    bounds: PageRect,
    native_text: &str,
    native_layer_id: &LayerId,
    native_provenance: &DeterministicProvenance,
    layout: Option<EngineResponse>,
    ocr: Option<EngineResponse>,
    needs_ocr: bool,
) -> (Vec<Region>, Vec<ReadingOrderEdge>) {
    if !needs_ocr {
        let layout_evidence = layout.map_or_else(Vec::new, |response| response.evidence);
        let mut regions = Vec::new();
        for (index, layout_item) in layout_evidence.into_iter().enumerate() {
            let text = match &layout_item.content {
                EvidenceContent::Text { text } => text.clone(),
                _ => continue,
            };
            let geometry = layout_item.geometry.unwrap_or(bounds);
            let native = native_evidence(
                native_layer_id,
                native_provenance,
                &text,
                geometry,
                index as u64,
            );
            let kind = layout_item
                .engine_metadata
                .get("region_kind")
                .and_then(serde_json::Value::as_str)
                .and_then(|kind| kind.parse().ok())
                .unwrap_or(RegionKind::Paragraph);
            regions.push(Region {
                id: RegionId::derive(&[
                    native_provenance.input_digest.as_bytes(),
                    &page_index.to_be_bytes(),
                    &(index as u64).to_be_bytes(),
                ]),
                kind,
                geometry,
                selected: Some(SelectedView {
                    evidence_ids: vec![native.id.clone()],
                    reason: SelectionReason::NativeQuality,
                    explanation: "native evidence met the configured quality threshold".into(),
                }),
                evidence: vec![native, layout_item],
            });
        }
        if regions.is_empty() && !native_text.is_empty() {
            let native =
                native_evidence(native_layer_id, native_provenance, native_text, bounds, 0);
            regions.push(Region {
                id: RegionId::derive(&[
                    native_provenance.input_digest.as_bytes(),
                    &page_index.to_be_bytes(),
                ]),
                kind: RegionKind::Paragraph,
                geometry: bounds,
                evidence: vec![native.clone()],
                selected: Some(SelectedView {
                    evidence_ids: vec![native.id],
                    reason: SelectionReason::NativeQuality,
                    explanation: "native evidence met the configured quality threshold".into(),
                }),
            });
        }
        let reading_order = regions
            .windows(2)
            .map(|pair| ReadingOrderEdge {
                before: pair[0].id.clone(),
                after: pair[1].id.clone(),
            })
            .collect();
        return (regions, reading_order);
    }

    let mut evidence = layout.map_or_else(Vec::new, |response| response.evidence);
    let native = (!native_text.is_empty())
        .then(|| native_evidence(native_layer_id, native_provenance, native_text, bounds, 0));
    if let Some(native) = &native {
        evidence.push(native.clone());
    }
    let ocr_evidence = ocr.map_or_else(Vec::new, |response| response.evidence);
    let ocr_covers_native = !native_text.is_empty()
        && ocr_evidence.iter().any(|item| {
            matches!(
                &item.content,
                EvidenceContent::Text { text } if text.contains(native_text)
            )
        });
    let ocr_ids: Vec<_> = ocr_evidence.iter().map(|item| item.id.clone()).collect();
    evidence.extend(ocr_evidence);
    let mut selected_ids = Vec::new();
    if let Some(native) = &native
        && !ocr_covers_native
    {
        selected_ids.push(native.id.clone());
    }
    selected_ids.extend(ocr_ids);
    let selected = (!selected_ids.is_empty()).then(|| SelectedView {
        reason: if native.is_some() {
            SelectionReason::Reconciled
        } else {
            SelectionReason::NativeAbsent
        },
        explanation: if ocr_covers_native {
            "OCR covered the low-quality native text; native evidence remains available separately"
                .into()
        } else if native.is_some() {
            "native and OCR evidence were retained and selected in source order".into()
        } else {
            "OCR selected because native evidence was absent".into()
        },
        evidence_ids: selected_ids,
    });
    (
        vec![Region {
            id: RegionId::derive(&[
                native_provenance.input_digest.as_bytes(),
                &page_index.to_be_bytes(),
                b"reconciled",
            ]),
            kind: RegionKind::Paragraph,
            geometry: bounds,
            evidence,
            selected,
        }],
        Vec::new(),
    )
}

fn native_evidence(
    layer_id: &LayerId,
    provenance: &DeterministicProvenance,
    text: &str,
    geometry: PageRect,
    index: u64,
) -> Evidence {
    Evidence {
        id: EvidenceId::derive(&[
            provenance.input_digest.as_bytes(),
            &geometry.page_index.to_be_bytes(),
            &index.to_be_bytes(),
            text.as_bytes(),
        ]),
        layer_id: layer_id.clone(),
        content: EvidenceContent::Text { text: text.into() },
        geometry: Some(geometry),
        confidence: None,
        provenance: provenance.clone(),
        engine_metadata: BTreeMap::new(),
    }
}

fn local_page_bounds(width: f64, height: f64, page_index: u32) -> Result<PageRect, RuntimeError> {
    let rect = Rect::new(0.0, 0.0, width, height, CoordinateSpace::Pdf, Unit::Point)
        .map_err(|error| PdfError::Malformed(error.to_string()))?;
    Ok(PageRect {
        page_index,
        rect,
        source_transform: CoordinateTransform::IDENTITY,
    })
}

fn quality_characters(text: &str) -> usize {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .count()
}

fn provenance(
    input_digest: Sha256Digest,
    engine_id: &str,
    engine_version: &str,
    stage: Stage,
    parameters: BTreeMap<String, serde_json::Value>,
    model_digest: Option<Sha256Digest>,
) -> DeterministicProvenance {
    DeterministicProvenance {
        schema_version: CURRENT_SCHEMA_VERSION,
        input_digest,
        engine_id: engine_id.into(),
        engine_version: engine_version.into(),
        model_digest,
        parameters,
        stage,
    }
}

struct OneBlob {
    scoped: ScopedBlob,
    digest: Sha256Digest,
    bytes: Vec<u8>,
}

impl OneBlob {
    fn new(id: &str, bytes: Vec<u8>, media_type: &str) -> Result<Self, EngineError> {
        if bytes.is_empty() {
            return Err(EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "empty stage blob",
            ));
        }
        let digest = Sha256Digest::of_bytes(&bytes);
        let scoped = ScopedBlob {
            id: BlobId::new(id).map_err(core_engine_error)?,
            range: BlobRange::new(0, bytes.len() as u64).map_err(core_engine_error)?,
            media_type: MediaType::new(media_type).map_err(core_engine_error)?,
            expected_digest: Some(digest),
        };
        Ok(Self {
            scoped,
            digest,
            bytes,
        })
    }
}

impl BlobResolver for OneBlob {
    fn resolve(&self, blob: &ScopedBlob) -> Result<Vec<u8>, EngineError> {
        if blob.id != self.scoped.id || blob.range.end() > self.bytes.len() as u64 {
            return Err(EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "blob token or range is outside the registered scope",
            ));
        }
        let start = usize::try_from(blob.range.offset()).map_err(|_| {
            EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "range offset overflow",
            )
        })?;
        let end = usize::try_from(blob.range.end()).map_err(|_| {
            EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "range end overflow",
            )
        })?;
        let bytes = self.bytes[start..end].to_vec();
        if blob
            .expected_digest
            .is_some_and(|expected| Sha256Digest::of_bytes(&bytes) != expected)
        {
            return Err(EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "blob digest mismatch",
            ));
        }
        Ok(bytes)
    }
}

fn core_engine_error(error: ferrodoc_core::CoreError) -> EngineError {
    EngineError::new(
        EngineErrorCategory::InvalidRequest,
        false,
        error.to_string(),
    )
}

struct NoTrace;

impl TraceSink for NoTrace {
    fn event(&self, _code: &str, _fields: &BTreeMap<String, String>) {}
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ferrodoc_core::{BackendId, DeviceKind};
    use ferrodoc_engine_api::{
        EngineCandidate, EngineCompatibility, HardwareInventory, HealthStatus, NetworkUse,
    };

    use super::*;

    struct Mock {
        descriptor: EngineDescriptor,
    }

    impl Mock {
        fn new(id: &str) -> Self {
            Self {
                descriptor: EngineDescriptor {
                    id: id.into(),
                    version: "1.0.0".into(),
                    capabilities: BTreeSet::from([Capability::OcrPage]),
                    compatibility: vec![EngineCompatibility {
                        backend: BackendId::new("mock").unwrap(),
                        devices: BTreeSet::from([DeviceKind::Cpu]),
                    }],
                    deterministic: true,
                    network_use: NetworkUse::None,
                    max_concurrency: 1,
                },
            }
        }
    }

    impl Engine for Mock {
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
            _request: &EngineRequest,
            _inventory: &HardwareInventory,
        ) -> Result<Vec<EngineCandidate>, EngineError> {
            Ok(Vec::new())
        }
        fn execute(
            &mut self,
            request: EngineRequest,
            _context: &ExecutionContext<'_>,
        ) -> Result<EngineResponse, EngineError> {
            Ok(EngineResponse {
                request_id: request.request_id,
                evidence: Vec::new(),
                metadata: BTreeMap::new(),
            })
        }
    }

    #[test]
    fn registry_rejects_duplicate_ids() {
        let mut registry = EmbeddedRegistry::new();
        registry.register(Mock::new("mock")).unwrap();
        assert!(matches!(
            registry.register(Mock::new("mock")),
            Err(RuntimeError::DuplicateEngine(_))
        ));
        assert_eq!(registry.descriptors()[0].id, "mock");
    }

    #[test]
    fn born_digital_conversion_is_deterministic_and_native() {
        let bytes = include_bytes!("../../../fixtures/pdf/born-digital.pdf").to_vec();
        let mut converter = Converter::new(ConversionOptions::default());
        let first = converter.convert(bytes.clone()).unwrap();
        let second = converter.convert(bytes).unwrap();
        assert_eq!(first, second);
        assert!(
            first.document.pages[0]
                .layers
                .iter()
                .any(|layer| layer.kind == SourceLayerKind::NativePdf)
        );
        assert!(
            !first.document.pages[0]
                .layers
                .iter()
                .any(|layer| layer.kind == SourceLayerKind::Ocr)
        );
        assert!(
            first
                .trace
                .events
                .iter()
                .any(|event| event.code == "ocr.rejected")
        );
    }

    #[test]
    fn scan_plan_is_input_specific_and_reports_missing_models() {
        let bytes = include_bytes!("../../../fixtures/pdf/image-only.pdf");
        let mut converter = Converter::new(ConversionOptions::default());
        let plan = converter.plan(bytes).unwrap();
        assert!(plan.stages.iter().any(|stage| {
            stage.page_index == Some(0)
                && stage.stage == "ocr.ocrs"
                && stage.decision == PlanDecision::Unavailable
        }));
        assert!(matches!(
            converter.convert(bytes.to_vec()),
            Err(RuntimeError::OcrUnavailable { page_index: 0 })
        ));
    }
}
