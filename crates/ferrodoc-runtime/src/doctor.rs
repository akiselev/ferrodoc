//! Structured plugin readiness diagnostics.

use std::collections::BTreeMap;

use ferrodoc_core::{BlobId, BlobRange, MediaType, ScopedBlob, Sha256Digest};
use ferrodoc_engine_api::{
    BlobResolver, CancellationToken, Engine, EngineError, EngineErrorCategory, EngineRequest,
    ExecutionContext, HealthRequest, HealthStatus, TraceSink,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Diagnostic stage, kept separate so callers can distinguish failure classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum DoctorStage {
    /// Executable or embedded implementation discovery.
    Discovery,
    /// Non-model runtime dependency readiness.
    Dependency,
    /// Model readiness.
    Model,
    /// Engine-level health.
    Health,
    /// Optional minimal semantic execution.
    Inference,
}

/// Outcome of one diagnostic stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum DoctorStatus {
    /// Stage completed successfully.
    Passed,
    /// Stage completed with a categorized failure.
    Failed,
    /// Stage was not requested or could not safely run.
    Skipped,
}

/// One structured plugin diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DoctorCheck {
    /// Engine or attempted plugin identity.
    pub engine_id: String,
    /// Diagnostic class.
    pub stage: DoctorStage,
    /// Outcome.
    pub status: DoctorStatus,
    /// Redacted explanation.
    pub message: String,
}

/// Ordered plugin-doctor output.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DoctorReport {
    /// Individual checks.
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    /// Records discovery failure before an `Engine` instance exists.
    pub fn discovery_failure(&mut self, identity: impl Into<String>, message: impl Into<String>) {
        self.checks.push(DoctorCheck {
            engine_id: identity.into(),
            stage: DoctorStage::Discovery,
            status: DoctorStatus::Failed,
            message: message.into(),
        });
    }

    /// Appends health, dependency/model, and optional inference checks.
    pub fn inspect_engine(&mut self, engine: &mut dyn Engine, inference: Option<InferenceProbe>) {
        let engine_id = engine.descriptor().id.clone();
        self.checks.push(DoctorCheck {
            engine_id: engine_id.clone(),
            stage: DoctorStage::Discovery,
            status: DoctorStatus::Passed,
            message: "engine descriptor is available".into(),
        });
        match engine.health(HealthRequest::Dependencies) {
            Ok(health) => {
                for dependency in health.dependencies {
                    self.checks.push(DoctorCheck {
                        engine_id: engine_id.clone(),
                        stage: if dependency.id.contains("model") {
                            DoctorStage::Model
                        } else {
                            DoctorStage::Dependency
                        },
                        status: health_status(dependency.status),
                        message: format!("{}: {}", dependency.id, dependency.message),
                    });
                }
                self.checks.push(DoctorCheck {
                    engine_id: engine_id.clone(),
                    stage: DoctorStage::Health,
                    status: health_status(health.status),
                    message: health.message,
                });
            }
            Err(error) => self.checks.push(DoctorCheck {
                engine_id: engine_id.clone(),
                stage: error_stage(error.category),
                status: DoctorStatus::Failed,
                message: error.message,
            }),
        }
        match inference {
            None => self.checks.push(DoctorCheck {
                engine_id,
                stage: DoctorStage::Inference,
                status: DoctorStatus::Skipped,
                message: "inference probe was not requested or unavailable for this plugin".into(),
            }),
            Some(probe) => {
                let resolver = ProbeResolver {
                    scoped: probe.request.input.clone(),
                    bytes: probe.bytes,
                };
                let context = ExecutionContext {
                    cancellation: CancellationToken::default(),
                    deadline: None,
                    blobs: &resolver,
                    trace: &NoTrace,
                };
                let result = engine.execute(probe.request, &context);
                self.checks.push(DoctorCheck {
                    engine_id,
                    stage: DoctorStage::Inference,
                    status: if result.is_ok() {
                        DoctorStatus::Passed
                    } else {
                        DoctorStatus::Failed
                    },
                    message: result.map_or_else(
                        |error| error.message,
                        |_| "minimal semantic inference succeeded".into(),
                    ),
                });
            }
        }
    }
}

/// Input bytes and matching semantic request for an optional inference check.
pub struct InferenceProbe {
    /// Request passed to the engine.
    pub request: EngineRequest,
    /// Exact registered blob bytes.
    pub bytes: Vec<u8>,
}

impl InferenceProbe {
    /// Creates a probe and replaces its input scope with a verified local blob.
    pub fn new(
        mut request: EngineRequest,
        bytes: Vec<u8>,
        media_type: &str,
    ) -> Result<Self, EngineError> {
        if bytes.is_empty() {
            return Err(EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "doctor inference blob cannot be empty",
            ));
        }
        request.input = ScopedBlob {
            id: BlobId::new("doctor-probe").map_err(core_error)?,
            range: BlobRange::new(0, bytes.len() as u64).map_err(core_error)?,
            media_type: MediaType::new(media_type).map_err(core_error)?,
            expected_digest: Some(Sha256Digest::of_bytes(&bytes)),
        };
        Ok(Self { request, bytes })
    }
}

fn health_status(status: HealthStatus) -> DoctorStatus {
    match status {
        HealthStatus::Healthy => DoctorStatus::Passed,
        HealthStatus::Degraded | HealthStatus::Unavailable => DoctorStatus::Failed,
    }
}

fn error_stage(category: EngineErrorCategory) -> DoctorStage {
    match category {
        EngineErrorCategory::Dependency => DoctorStage::Dependency,
        EngineErrorCategory::Model => DoctorStage::Model,
        _ => DoctorStage::Health,
    }
}

fn core_error(error: ferrodoc_core::CoreError) -> EngineError {
    EngineError::new(
        EngineErrorCategory::InvalidRequest,
        false,
        error.to_string(),
    )
}

struct ProbeResolver {
    scoped: ScopedBlob,
    bytes: Vec<u8>,
}

impl BlobResolver for ProbeResolver {
    fn resolve(&self, blob: &ScopedBlob) -> Result<Vec<u8>, EngineError> {
        if blob != &self.scoped {
            return Err(EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "doctor probe requested an unregistered blob",
            ));
        }
        Ok(self.bytes.clone())
    }
}

struct NoTrace;

impl TraceSink for NoTrace {
    fn event(&self, _code: &str, _fields: &BTreeMap<String, String>) {}
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ferrodoc_core::{BackendId, Capability, DeviceKind, RequestId};
    use ferrodoc_engine_api::{
        DependencyHealth, EngineCandidate, EngineCompatibility, EngineDescriptor, EngineResponse,
        HardwareInventory, HealthReport, NetworkUse,
    };

    use super::*;

    struct Unavailable {
        descriptor: EngineDescriptor,
    }

    impl Unavailable {
        fn new() -> Self {
            Self {
                descriptor: EngineDescriptor {
                    id: "doctor.fixture".into(),
                    version: "1".into(),
                    capabilities: BTreeSet::from([Capability::OcrPage]),
                    compatibility: vec![EngineCompatibility {
                        backend: BackendId::new("fixture").unwrap(),
                        devices: BTreeSet::from([DeviceKind::Cpu]),
                    }],
                    deterministic: true,
                    network_use: NetworkUse::None,
                    max_concurrency: 1,
                },
            }
        }
    }

    impl Engine for Unavailable {
        fn descriptor(&self) -> &EngineDescriptor {
            &self.descriptor
        }

        fn health(&mut self, _: HealthRequest) -> Result<HealthReport, EngineError> {
            Ok(HealthReport {
                status: HealthStatus::Unavailable,
                dependencies: vec![
                    DependencyHealth {
                        id: "fixture-model".into(),
                        status: HealthStatus::Unavailable,
                        message: "missing".into(),
                    },
                    DependencyHealth {
                        id: "native-runtime".into(),
                        status: HealthStatus::Unavailable,
                        message: "missing".into(),
                    },
                ],
                message: "not ready".into(),
            })
        }

        fn estimate(
            &mut self,
            _: &EngineRequest,
            _: &HardwareInventory,
        ) -> Result<Vec<EngineCandidate>, EngineError> {
            Ok(Vec::new())
        }

        fn execute(
            &mut self,
            _: EngineRequest,
            _: &ExecutionContext<'_>,
        ) -> Result<EngineResponse, EngineError> {
            Err(EngineError::new(
                EngineErrorCategory::Model,
                false,
                "missing",
            ))
        }
    }

    #[test]
    fn report_distinguishes_model_health_and_inference() {
        let mut report = DoctorReport::default();
        let request = EngineRequest {
            request_id: RequestId::derive(&[b"doctor"]),
            capability: Capability::OcrPage,
            input: ScopedBlob {
                id: BlobId::new("placeholder").unwrap(),
                range: BlobRange::new(0, 1).unwrap(),
                media_type: MediaType::new("application/octet-stream").unwrap(),
                expected_digest: None,
            },
            page_index: Some(0),
            parameters: BTreeMap::new(),
            deterministic_seed: None,
            deadline: None,
        };
        report.inspect_engine(
            &mut Unavailable::new(),
            Some(InferenceProbe::new(request, vec![0], "application/octet-stream").unwrap()),
        );
        assert!(report.checks.iter().any(|check| {
            check.stage == DoctorStage::Model && check.status == DoctorStatus::Failed
        }));
        assert!(report.checks.iter().any(|check| {
            check.stage == DoctorStage::Health && check.status == DoctorStatus::Failed
        }));
        assert!(report.checks.iter().any(|check| {
            check.stage == DoctorStage::Dependency && check.status == DoctorStatus::Failed
        }));
        assert!(report.checks.iter().any(|check| {
            check.stage == DoctorStage::Inference && check.status == DoctorStatus::Failed
        }));
    }
}
