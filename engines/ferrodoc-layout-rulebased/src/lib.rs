//! Deterministic, model-free layout segmentation for the CPU vertical slice.

use std::collections::{BTreeMap, BTreeSet};

use ferrodoc_core::{
    BackendId, Bytes, CURRENT_SCHEMA_VERSION, Capability, CoordinateSpace, CoordinateTransform,
    DeterministicProvenance, DeviceId, DeviceKind, Estimate, EstimateConfidence, EstimateSource,
    EvidenceId, LayerId, Millis, PageRect, Rect, ResourceEstimate, Sha256Digest, Stage, Unit,
};
use ferrodoc_engine_api::{
    Engine, EngineCandidate, EngineCompatibility, EngineDescriptor, EngineError,
    EngineErrorCategory, EngineRequest, EngineResponse, ExecutionContext, HardwareInventory,
    HealthReport, HealthRequest, HealthStatus, NetworkUse,
};
use ferrodoc_ir::{Evidence, EvidenceContent, RegionKind};

/// Stable engine identifier.
pub const ENGINE_ID: &str = "layout.rulebased";
/// Engine semantic version.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Basic deterministic layout engine for extracted UTF-8 page text.
pub struct RuleBasedLayoutEngine {
    descriptor: EngineDescriptor,
}

impl Default for RuleBasedLayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleBasedLayoutEngine {
    /// Constructs the always-available CPU engine.
    pub fn new() -> Self {
        Self {
            descriptor: EngineDescriptor {
                id: ENGINE_ID.into(),
                version: ENGINE_VERSION.into(),
                capabilities: BTreeSet::from([
                    Capability::LayoutDetect,
                    Capability::ReadingOrderDetect,
                ]),
                compatibility: vec![EngineCompatibility {
                    backend: BackendId::new("rules").expect("static backend"),
                    devices: BTreeSet::from([DeviceKind::Cpu]),
                }],
                deterministic: true,
                network_use: NetworkUse::None,
                max_concurrency: 64,
            },
        }
    }
}

impl Engine for RuleBasedLayoutEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }

    fn health(&mut self, _request: HealthRequest) -> Result<HealthReport, EngineError> {
        Ok(HealthReport {
            status: HealthStatus::Healthy,
            dependencies: Vec::new(),
            message: "model-free rule engine is ready".into(),
        })
    }

    fn estimate(
        &mut self,
        request: &EngineRequest,
        _inventory: &HardwareInventory,
    ) -> Result<Vec<EngineCandidate>, EngineError> {
        require_capability(request)?;
        Ok(vec![EngineCandidate {
            engine_id: ENGINE_ID.into(),
            backend: BackendId::new("rules").expect("static backend"),
            device: DeviceId::new(DeviceKind::Cpu, None).expect("static device"),
            resources: ResourceEstimate {
                peak_ram: Estimate::Known(Bytes::new(16 * Bytes::MIB)),
                warm_ram: Estimate::Known(Bytes::new(Bytes::MIB)),
                peak_vram: Estimate::Known(Bytes::new(0)),
                warm_vram: Estimate::Known(Bytes::new(0)),
                latency: Estimate::Known(Millis::new(10)),
                remote_cost: Estimate::Known(ferrodoc_core::MicroUsd::new(0)),
                quality: Estimate::Unknown,
                source: Estimate::Known(EstimateSource {
                    confidence: EstimateConfidence::Conservative,
                    method: "static text segmentation bound".into(),
                }),
            },
        }])
    }

    fn execute(
        &mut self,
        request: EngineRequest,
        context: &ExecutionContext<'_>,
    ) -> Result<EngineResponse, EngineError> {
        require_capability(&request)?;
        context.checkpoint()?;
        if request.input.media_type.as_str() != "text/plain" {
            return Err(invalid("layout input must have media type text/plain"));
        }
        let bytes = context.blobs.resolve(&request.input)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| invalid("layout input is not UTF-8"))?;
        let width = positive_parameter(&request, "page_width")?;
        let height = positive_parameter(&request, "page_height")?;
        let page_index = request
            .page_index
            .ok_or_else(|| invalid("page_index is required"))?;
        let input_digest = request
            .input
            .expected_digest
            .unwrap_or_else(|| Sha256Digest::of_bytes(&bytes));
        let provenance = DeterministicProvenance {
            schema_version: CURRENT_SCHEMA_VERSION,
            input_digest,
            engine_id: ENGINE_ID.into(),
            engine_version: ENGINE_VERSION.into(),
            model_digest: None,
            parameters: request.parameters.clone(),
            stage: Stage::Layout,
        };
        let provenance_digest = provenance
            .identity_digest()
            .map_err(|error| internal(error.to_string()))?;
        let layer_id = LayerId::derive(&[provenance_digest.as_bytes()]);
        let blocks = text_blocks(text);
        let block_count = blocks.len();
        let band_height = height / block_count.max(1) as f64;
        let evidence = blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| {
                let kind = classify(&block);
                let y = index as f64 * band_height;
                let region_height = if index + 1 == block_count {
                    height - y
                } else {
                    band_height
                };
                let geometry = PageRect {
                    page_index,
                    rect: Rect::new(
                        0.0,
                        y,
                        width,
                        region_height,
                        CoordinateSpace::Pdf,
                        Unit::Point,
                    )
                    .expect("validated positive page dimensions"),
                    source_transform: CoordinateTransform::IDENTITY,
                };
                let id = EvidenceId::derive(&[
                    provenance_digest.as_bytes(),
                    &(index as u64).to_be_bytes(),
                    block.as_bytes(),
                ]);
                let mut engine_metadata = BTreeMap::new();
                engine_metadata.insert("region_kind".into(), serde_json::json!(kind.to_string()));
                engine_metadata.insert("reading_order".into(), serde_json::json!(index));
                Evidence {
                    id,
                    layer_id: layer_id.clone(),
                    content: EvidenceContent::Text { text: block },
                    geometry: Some(geometry),
                    confidence: None,
                    provenance: provenance.clone(),
                    engine_metadata,
                }
            })
            .collect();
        context.checkpoint()?;
        Ok(EngineResponse {
            request_id: request.request_id,
            evidence,
            metadata: BTreeMap::from([("layer_id".into(), serde_json::json!(layer_id))]),
        })
    }
}

fn require_capability(request: &EngineRequest) -> Result<(), EngineError> {
    if matches!(
        request.capability,
        Capability::LayoutDetect | Capability::ReadingOrderDetect
    ) {
        Ok(())
    } else {
        Err(invalid(
            "rule-based engine only supports layout capabilities",
        ))
    }
}

fn positive_parameter(request: &EngineRequest, key: &str) -> Result<f64, EngineError> {
    request
        .parameters
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| invalid(format!("missing or invalid {key} parameter")))
}

fn text_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let flush = |paragraph: &mut String, blocks: &mut Vec<String>| {
        if !paragraph.is_empty() {
            blocks.push(std::mem::take(paragraph));
        }
    };
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            flush(&mut paragraph, &mut blocks);
            continue;
        }
        if looks_like_heading(line) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(line.to_owned());
            continue;
        }
        if !paragraph.is_empty() && !paragraph.ends_with('-') {
            paragraph.push(' ');
        }
        paragraph.push_str(line);
        if line.ends_with(['.', '!', '?']) {
            flush(&mut paragraph, &mut blocks);
        }
    }
    flush(&mut paragraph, &mut blocks);
    blocks
}

fn classify(text: &str) -> RegionKind {
    if looks_like_heading(text) {
        RegionKind::Heading
    } else {
        RegionKind::Paragraph
    }
}

fn looks_like_heading(text: &str) -> bool {
    let letters: Vec<_> = text
        .chars()
        .filter(|character| character.is_alphabetic())
        .collect();
    !letters.is_empty()
        && text.len() <= 160
        && letters.iter().all(|character| character.is_uppercase())
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::InvalidRequest, false, message)
}

fn internal(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::Internal, false, message)
}

#[cfg(test)]
mod tests {
    use ferrodoc_core::{BlobId, BlobRange, MediaType, RequestId, ScopedBlob};
    use ferrodoc_engine_api::{BlobResolver, CancellationToken, TraceSink};

    use super::*;

    struct Resolver(Vec<u8>);
    impl BlobResolver for Resolver {
        fn resolve(&self, _blob: &ScopedBlob) -> Result<Vec<u8>, EngineError> {
            Ok(self.0.clone())
        }
    }
    struct Trace;
    impl TraceSink for Trace {
        fn event(&self, _code: &str, _fields: &BTreeMap<String, String>) {}
    }

    #[test]
    fn segmentation_is_stable_and_classifies_heading() {
        let bytes = b"FIXTURE HEADING\nA complete paragraph ends here.".to_vec();
        let request = EngineRequest {
            request_id: RequestId::derive(&[b"layout-test"]),
            capability: Capability::LayoutDetect,
            input: ScopedBlob {
                id: BlobId::new("text").unwrap(),
                range: BlobRange::new(0, bytes.len() as u64).unwrap(),
                media_type: MediaType::new("text/plain").unwrap(),
                expected_digest: Some(Sha256Digest::of_bytes(&bytes)),
            },
            page_index: Some(0),
            parameters: BTreeMap::from([
                ("page_width".into(), serde_json::json!(595.0)),
                ("page_height".into(), serde_json::json!(842.0)),
            ]),
            deterministic_seed: None,
            deadline: None,
        };
        let context = ExecutionContext {
            cancellation: CancellationToken::default(),
            deadline: None,
            blobs: &Resolver(bytes),
            trace: &Trace,
        };
        let mut engine = RuleBasedLayoutEngine::new();
        let first = engine.execute(request.clone(), &context).unwrap();
        let second = engine.execute(request, &context).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.evidence.len(), 2);
        assert_eq!(first.evidence[0].engine_metadata["region_kind"], "heading");
    }

    #[test]
    fn final_fractional_band_does_not_cross_page_bounds() {
        let bytes = b"one\ntwo\nthree\nfour\nfive\nsix\nseven".to_vec();
        let request = EngineRequest {
            request_id: RequestId::derive(&[b"fractional-layout-test"]),
            capability: Capability::LayoutDetect,
            input: ScopedBlob {
                id: BlobId::new("text").unwrap(),
                range: BlobRange::new(0, bytes.len() as u64).unwrap(),
                media_type: MediaType::new("text/plain").unwrap(),
                expected_digest: Some(Sha256Digest::of_bytes(&bytes)),
            },
            page_index: Some(0),
            parameters: BTreeMap::from([
                ("page_width".into(), serde_json::json!(558.0)),
                ("page_height".into(), serde_json::json!(746.0)),
            ]),
            deterministic_seed: None,
            deadline: None,
        };
        let context = ExecutionContext {
            cancellation: CancellationToken::default(),
            deadline: None,
            blobs: &Resolver(bytes),
            trace: &Trace,
        };
        let mut engine = RuleBasedLayoutEngine::new();
        let response = engine.execute(request, &context).unwrap();
        let last = response.evidence.last().unwrap().geometry.unwrap().rect;
        assert_eq!(last.bottom(), 746.0);
    }

    #[test]
    fn wrapped_lines_form_paragraphs_until_sentence_end() {
        assert_eq!(
            text_blocks("A wrapped line\ncontinues to its end.\nNEXT HEADING\nFinal text."),
            [
                "A wrapped line continues to its end.",
                "NEXT HEADING",
                "Final text."
            ]
        );
    }
}
