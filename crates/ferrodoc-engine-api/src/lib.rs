//! Synchronous, transport-independent engine semantics.
//!
//! Engines are blocking by design. A runtime decides whether to invoke one on a
//! worker thread or through process transport. Serialized requests contain only
//! scoped blob tokens and checked ranges, never host paths.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use ferrodoc_core::{
    BackendId, Bytes, Capability, DeviceId, DeviceKind, Estimate, Millis, RequestId,
    ResourceEstimate, ScopedBlob,
};
use ferrodoc_ir::Evidence;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Engine compatibility for one backend across physical device families.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EngineCompatibility {
    /// Inference or implementation backend.
    pub backend: BackendId,
    /// Compatible physical device families.
    pub devices: BTreeSet<DeviceKind>,
}

/// Whether an engine can disclose document data over a network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum NetworkUse {
    /// Engine never performs network I/O.
    None,
    /// Network use is optional and policy-controlled.
    Optional,
    /// Engine cannot execute without network access.
    Required,
}

/// Discoverable semantic descriptor for an engine implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EngineDescriptor {
    /// Stable lowercase engine identifier.
    pub id: String,
    /// Engine semantic version.
    pub version: String,
    /// Capabilities the engine can execute.
    pub capabilities: BTreeSet<Capability>,
    /// Compatible backend and device axes.
    pub compatibility: Vec<EngineCompatibility>,
    /// Whether identical deterministic inputs are expected to yield identical responses.
    pub deterministic: bool,
    /// Declared network behavior.
    pub network_use: NetworkUse,
    /// Maximum safe concurrent requests for one instance.
    pub max_concurrency: u32,
}

impl EngineDescriptor {
    /// Validates that a descriptor is usable for candidate enumeration.
    pub fn validate(&self) -> Result<(), EngineError> {
        let valid_id = !self.id.is_empty()
            && self.id.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            });
        if !valid_id || self.version.trim().is_empty() {
            return Err(EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "engine descriptor has an invalid ID or empty version",
            ));
        }
        if self.capabilities.is_empty()
            || self.compatibility.is_empty()
            || self
                .compatibility
                .iter()
                .any(|compatibility| compatibility.devices.is_empty())
            || self.max_concurrency == 0
        {
            return Err(EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "engine descriptor has empty capabilities, compatibility, or concurrency",
            ));
        }
        Ok(())
    }
}

/// Runtime-independent facts about one physical device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeviceInventory {
    /// Physical device identity.
    pub id: DeviceId,
    /// Total device memory.
    pub memory_total: Estimate<Bytes>,
    /// Currently available device memory.
    pub memory_available: Estimate<Bytes>,
    /// Deterministically ordered provider metadata.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// Hardware inventory supplied by the runtime to candidate estimation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HardwareInventory {
    /// Logical CPU count when known.
    pub logical_cpus: Estimate<u32>,
    /// Total host RAM.
    pub ram_total: Estimate<Bytes>,
    /// Currently available host RAM.
    pub ram_available: Estimate<Bytes>,
    /// Physical compute devices.
    #[serde(default)]
    pub devices: Vec<DeviceInventory>,
}

/// Health-check depth requested by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum HealthRequest {
    /// Check only immediate readiness.
    Shallow,
    /// Check optional dependencies and model readiness without inference.
    Dependencies,
}

/// Categorized engine health.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Ready for declared operations.
    Healthy,
    /// Usable with a documented degradation.
    Degraded,
    /// Not currently usable.
    Unavailable,
}

/// One dependency readiness item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DependencyHealth {
    /// Stable dependency identifier.
    pub id: String,
    /// Readiness status.
    pub status: HealthStatus,
    /// Redacted diagnostic message.
    pub message: String,
}

/// Engine health result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HealthReport {
    /// Overall readiness.
    pub status: HealthStatus,
    /// Dependency-level results.
    #[serde(default)]
    pub dependencies: Vec<DependencyHealth>,
    /// Redacted summary.
    pub message: String,
}

/// Serializable engine execution request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EngineRequest {
    /// Correlation identity.
    pub request_id: RequestId,
    /// Requested capability.
    pub capability: Capability,
    /// Immutable host-scoped input.
    pub input: ScopedBlob,
    /// Optional zero-based page index.
    pub page_index: Option<u32>,
    /// Normalized deterministic parameters.
    #[serde(default)]
    pub parameters: BTreeMap<String, serde_json::Value>,
    /// Deterministic seed when an engine supports seeded behavior.
    pub deterministic_seed: Option<u64>,
    /// Relative deadline from receipt.
    pub deadline: Option<Millis>,
}

/// One compatible engine candidate and its conservative estimate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EngineCandidate {
    /// Engine identifier.
    pub engine_id: String,
    /// Selected backend.
    pub backend: BackendId,
    /// Selected physical device.
    pub device: DeviceId,
    /// Resource and quality estimate.
    pub resources: ResourceEstimate,
}

/// Serializable engine execution response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EngineResponse {
    /// Correlation identity copied from the request.
    pub request_id: RequestId,
    /// Evidence records appended by the engine.
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    /// Deterministic, non-secret response metadata.
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Stable engine failure category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum EngineErrorCategory {
    /// Request violates the semantic contract.
    InvalidRequest,
    /// Requested operation is not supported.
    Unsupported,
    /// Engine is not currently available.
    Unavailable,
    /// A dependency is absent or incompatible.
    Dependency,
    /// A required model is absent or invalid.
    Model,
    /// Resource admission or execution failed.
    ResourceExhausted,
    /// Caller cancellation was observed.
    Cancelled,
    /// Deadline expired.
    DeadlineExceeded,
    /// Process or framing protocol failed.
    Protocol,
    /// Unclassified engine failure.
    Internal,
}

/// Structured engine error with explicit retryability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Error)]
#[error("{category:?}: {message}")]
pub struct EngineError {
    /// Stable category.
    pub category: EngineErrorCategory,
    /// Whether retry under the same semantic request may succeed.
    pub retryable: bool,
    /// Redacted diagnostic message.
    pub message: String,
}

impl EngineError {
    /// Creates a structured error.
    pub fn new(category: EngineErrorCategory, retryable: bool, message: impl Into<String>) -> Self {
        Self {
            category,
            retryable,
            message: message.into(),
        }
    }
}

/// Cooperative cancellation flag shared with blocking engine code.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Requests cancellation.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Host-controlled resolver for immutable scoped blobs.
pub trait BlobResolver: Send + Sync {
    /// Resolves only the approved range after host-side scope and digest checks.
    fn resolve(&self, blob: &ScopedBlob) -> Result<Vec<u8>, EngineError>;
}

/// Structured trace sink that does not expose a logging runtime to engines.
pub trait TraceSink: Send + Sync {
    /// Records a deterministic event code and redacted fields.
    fn event(&self, code: &str, fields: &BTreeMap<String, String>);
}

/// Non-serializable execution controls supplied by the host.
pub struct ExecutionContext<'a> {
    /// Cooperative cancellation token.
    pub cancellation: CancellationToken,
    /// Absolute host deadline.
    pub deadline: Option<Instant>,
    /// Scoped blob resolver.
    pub blobs: &'a dyn BlobResolver,
    /// Structured trace sink.
    pub trace: &'a dyn TraceSink,
}

impl ExecutionContext<'_> {
    /// Fails when cancellation or the deadline has been observed.
    pub fn checkpoint(&self) -> Result<(), EngineError> {
        if self.cancellation.is_cancelled() {
            return Err(EngineError::new(
                EngineErrorCategory::Cancelled,
                false,
                "request cancelled",
            ));
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(EngineError::new(
                EngineErrorCategory::DeadlineExceeded,
                true,
                "request deadline exceeded",
            ));
        }
        Ok(())
    }
}

/// Blocking engine interface independent of embedded or process transport.
pub trait Engine: Send {
    /// Returns a stable descriptor.
    fn descriptor(&self) -> &EngineDescriptor;

    /// Reports readiness and dependency state.
    fn health(&mut self, request: HealthRequest) -> Result<HealthReport, EngineError>;

    /// Enumerates compatible placements and conservative estimates.
    fn estimate(
        &self,
        request: &EngineRequest,
        inventory: &HardwareInventory,
    ) -> Result<Vec<EngineCandidate>, EngineError>;

    /// Executes one request and appends evidence.
    fn execute(
        &mut self,
        request: EngineRequest,
        context: &ExecutionContext<'_>,
    ) -> Result<EngineResponse, EngineError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_is_observable_without_a_runtime() {
        struct NoBlobs;
        impl BlobResolver for NoBlobs {
            fn resolve(&self, _blob: &ScopedBlob) -> Result<Vec<u8>, EngineError> {
                unreachable!()
            }
        }
        struct NoTrace;
        impl TraceSink for NoTrace {
            fn event(&self, _code: &str, _fields: &BTreeMap<String, String>) {}
        }
        let cancellation = CancellationToken::default();
        let context = ExecutionContext {
            cancellation: cancellation.clone(),
            deadline: None,
            blobs: &NoBlobs,
            trace: &NoTrace,
        };
        assert!(context.checkpoint().is_ok());
        cancellation.cancel();
        assert_eq!(
            context.checkpoint().unwrap_err().category,
            EngineErrorCategory::Cancelled
        );
    }

    #[test]
    fn request_schema_contains_no_host_path() {
        let schema = serde_json::to_string(&schemars::schema_for!(EngineRequest)).unwrap();
        assert!(!schema.contains("PathBuf"));
        assert!(!schema.contains("host_path"));
        assert!(!schema.contains("\"path\""));
        assert!(schema.contains("BlobId"));
        assert!(schema.contains("BlobRange"));
    }
}
