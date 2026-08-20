//! Deterministic engine used for transport conformance and fault injection.

use std::collections::{BTreeMap, BTreeSet};

use ferrodoc_core::{
    BackendId, Bytes, CURRENT_SCHEMA_VERSION, Capability, CoordinateSpace, CoordinateTransform,
    DeterministicProvenance, DeviceId, DeviceKind, Estimate, EstimateConfidence, EstimateSource,
    EvidenceId, LayerId, MicroUsd, Millis, PageRect, Rect, ResourceEstimate, Sha256Digest, Stage,
    Unit,
};
use ferrodoc_engine_api::{
    Engine, EngineCandidate, EngineCompatibility, EngineDescriptor, EngineError,
    EngineErrorCategory, EngineRequest, EngineResponse, ExecutionContext, HardwareInventory,
    HealthReport, HealthRequest, HealthStatus, NetworkUse,
};
use ferrodoc_ir::{Evidence, EvidenceContent};

/// Stable mock engine ID.
pub const ENGINE_ID: &str = "test.mock";

/// Deterministic echo engine.
pub struct MockEngine {
    descriptor: EngineDescriptor,
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEngine {
    /// Constructs the test engine.
    pub fn new() -> Self {
        Self {
            descriptor: EngineDescriptor {
                id: ENGINE_ID.into(),
                version: env!("CARGO_PKG_VERSION").into(),
                capabilities: BTreeSet::from([Capability::OcrPage]),
                compatibility: vec![EngineCompatibility {
                    backend: BackendId::new("mock").expect("static backend"),
                    devices: BTreeSet::from([DeviceKind::Cpu]),
                }],
                deterministic: true,
                network_use: NetworkUse::None,
                max_concurrency: 1,
            },
        }
    }
}

impl Engine for MockEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }

    fn health(&mut self, _request: HealthRequest) -> Result<HealthReport, EngineError> {
        Ok(HealthReport {
            status: HealthStatus::Healthy,
            dependencies: Vec::new(),
            message: "deterministic mock ready".into(),
        })
    }

    fn estimate(
        &mut self,
        request: &EngineRequest,
        _inventory: &HardwareInventory,
    ) -> Result<Vec<EngineCandidate>, EngineError> {
        require_ocr(request)?;
        Ok(vec![EngineCandidate {
            engine_id: ENGINE_ID.into(),
            backend: BackendId::new("mock").expect("static backend"),
            device: DeviceId::new(DeviceKind::Cpu, None).expect("static device"),
            resources: ResourceEstimate {
                peak_ram: Estimate::Known(Bytes::new(Bytes::MIB)),
                warm_ram: Estimate::Known(Bytes::new(0)),
                peak_vram: Estimate::Known(Bytes::new(0)),
                warm_vram: Estimate::Known(Bytes::new(0)),
                latency: Estimate::Known(Millis::new(1)),
                remote_cost: Estimate::Known(MicroUsd::new(0)),
                quality: Estimate::Unknown,
                source: Estimate::Known(EstimateSource {
                    confidence: EstimateConfidence::Conservative,
                    method: "mock constant".into(),
                }),
            },
        }])
    }

    fn execute(
        &mut self,
        request: EngineRequest,
        context: &ExecutionContext<'_>,
    ) -> Result<EngineResponse, EngineError> {
        require_ocr(&request)?;
        context.checkpoint()?;
        if request
            .parameters
            .get("fault")
            .and_then(serde_json::Value::as_str)
            == Some("engine_error")
        {
            return Err(EngineError::new(
                EngineErrorCategory::Internal,
                false,
                "injected mock engine error",
            ));
        }
        if request
            .parameters
            .get("fault")
            .and_then(serde_json::Value::as_str)
            == Some("hang")
        {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
        let bytes = context.blobs.resolve(&request.input)?;
        let source = std::str::from_utf8(&bytes).map_err(|_| {
            EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "mock input must be UTF-8",
            )
        })?;
        let text = format!("mock:{source}");
        let input_digest = request
            .input
            .expected_digest
            .unwrap_or_else(|| Sha256Digest::of_bytes(&bytes));
        let provenance = DeterministicProvenance {
            schema_version: CURRENT_SCHEMA_VERSION,
            input_digest,
            engine_id: ENGINE_ID.into(),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            model_digest: None,
            parameters: request.parameters.clone(),
            stage: Stage::Ocr,
        };
        let identity = provenance.identity_digest().map_err(|error| {
            EngineError::new(EngineErrorCategory::Internal, false, error.to_string())
        })?;
        let layer_id = LayerId::derive(&[identity.as_bytes()]);
        let page_index = request.page_index.unwrap_or(0);
        Ok(EngineResponse {
            request_id: request.request_id,
            evidence: vec![Evidence {
                id: EvidenceId::derive(&[identity.as_bytes(), text.as_bytes()]),
                layer_id: layer_id.clone(),
                content: EvidenceContent::Text { text },
                geometry: Some(PageRect {
                    page_index,
                    rect: Rect::new(0.0, 0.0, 1.0, 1.0, CoordinateSpace::Normalized, Unit::Ratio)
                        .expect("static geometry"),
                    source_transform: CoordinateTransform::IDENTITY,
                }),
                confidence: None,
                provenance,
                engine_metadata: BTreeMap::new(),
            }],
            metadata: BTreeMap::from([("layer_id".into(), serde_json::json!(layer_id))]),
        })
    }
}

fn require_ocr(request: &EngineRequest) -> Result<(), EngineError> {
    if request.capability == Capability::OcrPage {
        Ok(())
    } else {
        Err(EngineError::new(
            EngineErrorCategory::Unsupported,
            false,
            "mock supports only ocr.page",
        ))
    }
}
