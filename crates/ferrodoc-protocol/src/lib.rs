//! Versioned process-protocol schema types without process I/O.

use ferrodoc_core::RequestId;
use ferrodoc_engine_api::{
    EngineCandidate, EngineDescriptor, EngineError, EngineRequest, EngineResponse,
    HardwareInventory, HealthReport, HealthRequest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Fixed bytes preceding every protocol stream.
pub const PREAMBLE: [u8; 8] = *b"FERRODOC";
/// Maximum encoded frame payload accepted by v1 hosts and engines.
pub const MAX_FRAME_LENGTH: u32 = 16 * 1024 * 1024;
/// Current process-protocol version.
pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion(1);

/// Integer wire-protocol version.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct ProtocolVersion(pub u16);

/// Inclusive supported protocol range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VersionRange {
    /// Oldest supported version.
    pub minimum: ProtocolVersion,
    /// Newest supported version.
    pub maximum: ProtocolVersion,
}

impl VersionRange {
    /// Selects the newest mutually supported version.
    pub fn negotiate(self, peer: Self) -> Option<ProtocolVersion> {
        let minimum = self.minimum.max(peer.minimum);
        let maximum = self.maximum.min(peer.maximum);
        (minimum <= maximum).then_some(maximum)
    }
}

/// Client negotiation preface following the fixed preamble.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClientHello {
    /// Host-supported versions.
    pub versions: VersionRange,
    /// Host maximum inbound frame size.
    pub maximum_frame_length: u32,
}

/// Engine negotiation response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServerHello {
    /// Selected mutually supported version.
    pub version: ProtocolVersion,
    /// Engine maximum inbound frame size.
    pub maximum_frame_length: u32,
    /// Engine descriptor available before other requests.
    pub descriptor: EngineDescriptor,
}

/// Host-to-engine semantic message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "body", rename_all = "snake_case")]
pub enum HostMessage {
    /// Request the descriptor again.
    Discover,
    /// Check readiness.
    Health(HealthRequest),
    /// Enumerate candidates against host inventory.
    Estimate {
        /// Semantic engine request.
        request: EngineRequest,
        /// Host hardware inventory.
        inventory: HardwareInventory,
    },
    /// Execute one semantic request.
    Execute(EngineRequest),
    /// Cancel another request ID.
    Cancel(RequestId),
    /// Liveness probe.
    Ping,
    /// Graceful shutdown request.
    Shutdown,
}

/// Engine-to-host semantic message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "body", rename_all = "snake_case")]
pub enum EngineMessage {
    /// Descriptor response.
    Descriptor(EngineDescriptor),
    /// Health response.
    Health(HealthReport),
    /// Candidate estimates.
    Estimate(Vec<EngineCandidate>),
    /// Execution response.
    Execute(EngineResponse),
    /// Cancellation acknowledgement for the target request.
    Cancelled(RequestId),
    /// Ping response.
    Pong,
    /// Shutdown acknowledgement.
    Shutdown,
    /// Structured semantic or transport error.
    Error(EngineError),
}

/// Request envelope with a unique correlation ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RequestEnvelope {
    /// Negotiated version used for this message.
    pub version: ProtocolVersion,
    /// Unique request ID.
    pub request_id: RequestId,
    /// Host message.
    pub message: HostMessage,
}

/// Response envelope echoing the request ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResponseEnvelope {
    /// Negotiated version used for this message.
    pub version: ProtocolVersion,
    /// Correlated request ID.
    pub request_id: RequestId,
    /// Engine message.
    pub message: EngineMessage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_negotiation_selects_newest_overlap() {
        let host = VersionRange {
            minimum: ProtocolVersion(1),
            maximum: ProtocolVersion(3),
        };
        let engine = VersionRange {
            minimum: ProtocolVersion(2),
            maximum: ProtocolVersion(4),
        };
        assert_eq!(host.negotiate(engine), Some(ProtocolVersion(3)));
        assert_eq!(
            host.negotiate(VersionRange {
                minimum: ProtocolVersion(4),
                maximum: ProtocolVersion(5),
            }),
            None
        );
    }

    #[test]
    fn protocol_has_fixed_bounds() {
        assert_eq!(&PREAMBLE, b"FERRODOC");
        assert_eq!(MAX_FRAME_LENGTH, 16_777_216);
    }
}
