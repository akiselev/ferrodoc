//! Pure-Rust CPU OCR through the direct `ocrs` and `rten` APIs.
//!
//! Model bytes are injected explicitly. Construction and default builds never
//! download models or consult the network.

use std::collections::{BTreeMap, BTreeSet};

use ferrodoc_core::{
    BackendId, Bytes, CURRENT_SCHEMA_VERSION, Capability, CoordinateSpace, CoordinateTransform,
    DeterministicProvenance, DeviceId, DeviceKind, Estimate, EstimateConfidence, EstimateSource,
    EvidenceId, LayerId, MediaType, MicroUsd, PageRect, Rect, ResourceEstimate, Sha256Digest,
    Stage, Unit,
};
use ferrodoc_engine_api::{
    DependencyHealth, Engine, EngineCandidate, EngineCompatibility, EngineDescriptor, EngineError,
    EngineErrorCategory, EngineRequest, EngineResponse, ExecutionContext, HardwareInventory,
    HealthReport, HealthRequest, HealthStatus, NetworkUse,
};
use ferrodoc_ir::{Evidence, EvidenceContent};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;

/// Stable engine identifier.
pub const ENGINE_ID: &str = "ocr.ocrs";
/// Engine semantic version.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Raw pixel input type accepted across the engine API.
pub const RGBA8_MEDIA_TYPE: &str = "application/vnd.ferrodoc.rgba8";
const MAXIMUM_PIXELS: u64 = 200_000_000;

/// OCRS engine with explicitly provisioned model state.
pub struct OcrsEngine {
    descriptor: EngineDescriptor,
    engine: Option<OcrEngine>,
    model_digest: Option<Sha256Digest>,
    model_error: Option<String>,
}

impl Default for OcrsEngine {
    fn default() -> Self {
        Self::without_models()
    }
}

impl OcrsEngine {
    /// Creates an unavailable engine descriptor without acquiring models.
    pub fn without_models() -> Self {
        Self {
            descriptor: descriptor(),
            engine: None,
            model_digest: None,
            model_error: None,
        }
    }

    /// Loads verified model bytes directly into RTen.
    pub fn from_model_bytes(detection: Vec<u8>, recognition: Vec<u8>) -> Result<Self, EngineError> {
        let model_digest = combined_model_digest(&detection, &recognition);
        let detection_model =
            Model::load(detection).map_err(|error| model_error(error.to_string()))?;
        let recognition_model =
            Model::load(recognition).map_err(|error| model_error(error.to_string()))?;
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(detection_model),
            recognition_model: Some(recognition_model),
            ..OcrEngineParams::default()
        })
        .map_err(|error| model_error(error.to_string()))?;
        Ok(Self {
            descriptor: descriptor(),
            engine: Some(engine),
            model_digest: Some(model_digest),
            model_error: None,
        })
    }

    /// Returns the combined detection/recognition model digest when loaded.
    pub const fn model_digest(&self) -> Option<Sha256Digest> {
        self.model_digest
    }
}

impl Engine for OcrsEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }

    fn health(&mut self, _request: HealthRequest) -> Result<HealthReport, EngineError> {
        let ready = self.engine.is_some();
        let message = self.model_error.as_deref().unwrap_or(if ready {
            "OCRS models are loaded"
        } else {
            "OCRS detection and recognition models are not installed"
        });
        Ok(HealthReport {
            status: if ready {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unavailable
            },
            dependencies: vec![DependencyHealth {
                id: "ocrs-model-pair".into(),
                status: if ready {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Unavailable
                },
                message: message.into(),
            }],
            message: message.into(),
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
            backend: BackendId::new("rten").expect("static backend"),
            device: DeviceId::new(DeviceKind::Cpu, None).expect("static device"),
            resources: ResourceEstimate {
                peak_ram: Estimate::Known(Bytes::new(768 * Bytes::MIB)),
                warm_ram: Estimate::Known(Bytes::new(256 * Bytes::MIB)),
                peak_vram: Estimate::Known(Bytes::new(0)),
                warm_vram: Estimate::Known(Bytes::new(0)),
                latency: Estimate::Unknown,
                remote_cost: Estimate::Known(MicroUsd::new(0)),
                quality: Estimate::Unknown,
                source: Estimate::Known(EstimateSource {
                    confidence: EstimateConfidence::Conservative,
                    method: "OCRS CPU static memory envelope".into(),
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
        if request.input.media_type != MediaType::new(RGBA8_MEDIA_TYPE).expect("static media type")
        {
            return Err(invalid(format!(
                "OCRS input must have media type {RGBA8_MEDIA_TYPE}"
            )));
        }
        let page_index = request
            .page_index
            .ok_or_else(|| invalid("page_index is required"))?;
        let width = dimension_parameter(&request, "width")?;
        let height = dimension_parameter(&request, "height")?;
        let pixels = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or_else(|| resource("image dimensions overflow"))?;
        if pixels > MAXIMUM_PIXELS {
            return Err(resource("image exceeds OCRS pixel limit"));
        }
        let expected_len = pixels
            .checked_mul(4)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| resource("RGBA byte count overflow"))?;
        let bytes = context.blobs.resolve(&request.input)?;
        if bytes.len() != expected_len {
            return Err(invalid("RGBA byte length does not match width and height"));
        }
        let engine = self.engine.as_ref().ok_or_else(|| {
            model_error("OCRS models are unavailable; install an explicit verified model pair")
        })?;
        let image = ImageSource::from_bytes(&bytes, (width, height))
            .map_err(|error| invalid(error.to_string()))?;
        let input = engine
            .prepare_input(image)
            .map_err(|error| internal(error.to_string()))?;
        let text = engine
            .get_text(&input)
            .map_err(|error| internal(error.to_string()))?
            .trim()
            .to_string();
        context.checkpoint()?;

        let input_digest = request
            .input
            .expected_digest
            .unwrap_or_else(|| Sha256Digest::of_bytes(&bytes));
        let model_digest = self.model_digest.expect("loaded engine has model digest");
        let provenance = DeterministicProvenance {
            schema_version: CURRENT_SCHEMA_VERSION,
            input_digest,
            engine_id: ENGINE_ID.into(),
            engine_version: ENGINE_VERSION.into(),
            model_digest: Some(model_digest),
            parameters: request.parameters.clone(),
            stage: Stage::Ocr,
        };
        let provenance_digest = provenance
            .identity_digest()
            .map_err(|error| internal(error.to_string()))?;
        let layer_id = LayerId::derive(&[provenance_digest.as_bytes()]);
        let evidence = if text.is_empty() {
            Vec::new()
        } else {
            vec![Evidence {
                id: EvidenceId::derive(&[provenance_digest.as_bytes(), text.as_bytes()]),
                layer_id: layer_id.clone(),
                content: EvidenceContent::Text { text },
                geometry: Some(PageRect {
                    page_index,
                    rect: Rect::new(
                        0.0,
                        0.0,
                        f64::from(width),
                        f64::from(height),
                        CoordinateSpace::Image,
                        Unit::Pixel,
                    )
                    .expect("validated image dimensions"),
                    source_transform: CoordinateTransform::IDENTITY,
                }),
                confidence: None,
                provenance,
                engine_metadata: BTreeMap::from([(
                    "recognizer".into(),
                    serde_json::json!("ocrs-0.12"),
                )]),
            }]
        };
        Ok(EngineResponse {
            request_id: request.request_id,
            evidence,
            metadata: BTreeMap::from([
                ("layer_id".into(), serde_json::json!(layer_id)),
                ("model_digest".into(), serde_json::json!(model_digest)),
            ]),
        })
    }
}

fn descriptor() -> EngineDescriptor {
    EngineDescriptor {
        id: ENGINE_ID.into(),
        version: ENGINE_VERSION.into(),
        capabilities: BTreeSet::from([Capability::OcrPage, Capability::OcrRegion]),
        compatibility: vec![EngineCompatibility {
            backend: BackendId::new("rten").expect("static backend"),
            devices: BTreeSet::from([DeviceKind::Cpu]),
        }],
        deterministic: true,
        network_use: NetworkUse::None,
        max_concurrency: 1,
    }
}

fn combined_model_digest(detection: &[u8], recognition: &[u8]) -> Sha256Digest {
    let detection_digest = Sha256Digest::of_bytes(detection);
    let recognition_digest = Sha256Digest::of_bytes(recognition);
    let mut identity = b"ferrodoc-ocrs-model-pair\0".to_vec();
    identity.extend_from_slice(detection_digest.as_bytes());
    identity.extend_from_slice(recognition_digest.as_bytes());
    Sha256Digest::of_bytes(&identity)
}

fn require_capability(request: &EngineRequest) -> Result<(), EngineError> {
    if matches!(
        request.capability,
        Capability::OcrPage | Capability::OcrRegion
    ) {
        Ok(())
    } else {
        Err(invalid("OCRS engine only supports OCR capabilities"))
    }
}

fn dimension_parameter(request: &EngineRequest, key: &str) -> Result<u32, EngineError> {
    request
        .parameters
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| invalid(format!("missing or invalid {key} parameter")))
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::InvalidRequest, false, message)
}

fn model_error(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::Model, false, message)
}

fn resource(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::ResourceExhausted, false, message)
}

fn internal(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::Internal, false, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_engine_is_honestly_unavailable() {
        let mut engine = OcrsEngine::without_models();
        let report = engine.health(HealthRequest::Dependencies).unwrap();
        assert_eq!(report.status, HealthStatus::Unavailable);
        assert!(engine.model_digest().is_none());
        assert_eq!(engine.descriptor().network_use, NetworkUse::None);
    }

    #[test]
    fn invalid_model_bytes_are_categorized() {
        let error = match OcrsEngine::from_model_bytes(vec![1, 2], vec![3, 4]) {
            Ok(_) => panic!("invalid models unexpectedly loaded"),
            Err(error) => error,
        };
        assert_eq!(error.category, EngineErrorCategory::Model);
    }
}
