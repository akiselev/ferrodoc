//! Deterministic artifact provenance separated from run observations.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    BackendId, DeviceId, Millis, RequestId, ResourceEstimate, SchemaVersion, Sha256Digest,
};

/// A deterministic pipeline stage.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum Stage {
    /// Input acquisition and hashing.
    Acquire,
    /// Document inspection.
    Inspect,
    /// Native content extraction.
    NativeExtract,
    /// Page rasterization.
    Rasterize,
    /// Layout analysis.
    Layout,
    /// Optical character recognition.
    Ocr,
    /// Evidence reconciliation.
    Reconcile,
    /// Output rendering.
    Render,
}

/// Cache-relevant provenance. It intentionally contains no clock time, host, or run ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeterministicProvenance {
    /// Persistent schema version.
    pub schema_version: SchemaVersion,
    /// Digest of the input artifact.
    pub input_digest: Sha256Digest,
    /// Stable engine identifier.
    pub engine_id: String,
    /// Semantic engine version.
    pub engine_version: String,
    /// Optional model content digest.
    pub model_digest: Option<Sha256Digest>,
    /// Normalized parameters in sorted key order.
    pub parameters: BTreeMap<String, serde_json::Value>,
    /// Producing pipeline stage.
    pub stage: Stage,
}

impl DeterministicProvenance {
    /// Computes deterministic identity for caching and stable evidence IDs.
    pub fn identity_digest(&self) -> Result<Sha256Digest, serde_json::Error> {
        serde_json::to_vec(self).map(|bytes| Sha256Digest::of_bytes(&bytes))
    }
}

/// Non-deterministic facts about one execution. This never contributes to artifact identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Observation {
    /// Correlation identity for the request.
    pub request_id: RequestId,
    /// Optional wall-clock timestamp as an RFC 3339 string supplied by the runtime.
    pub timestamp: Option<String>,
    /// Optional host label.
    pub host: Option<String>,
    /// Selected physical device.
    pub device: Option<DeviceId>,
    /// Selected inference backend.
    pub backend: Option<BackendId>,
    /// Measured wall duration.
    pub duration: Option<Millis>,
    /// Measured or still-unknown resources.
    pub resources: ResourceEstimate,
    /// Structured diagnostic labels, excluding secrets and document content.
    pub labels: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CURRENT_SCHEMA_VERSION;

    #[test]
    fn deterministic_identity_does_not_need_observations() {
        let provenance = DeterministicProvenance {
            schema_version: CURRENT_SCHEMA_VERSION,
            input_digest: Sha256Digest::of_bytes(b"input"),
            engine_id: "native-pdf".into(),
            engine_version: "1.0.0".into(),
            model_digest: None,
            parameters: BTreeMap::new(),
            stage: Stage::NativeExtract,
        };
        assert_eq!(
            provenance.identity_digest().unwrap(),
            provenance.identity_digest().unwrap()
        );
        let json = serde_json::to_string(&provenance).unwrap();
        assert!(!json.contains("timestamp"));
        assert!(!json.contains("request_id"));
    }
}
