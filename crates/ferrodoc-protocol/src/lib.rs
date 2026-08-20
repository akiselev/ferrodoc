//! Versioned, length-prefixed CBOR process protocol.

use std::io::{self, Cursor, Read, Write};

use ferrodoc_core::{BlobId, RequestId, ScopedBlob};
use ferrodoc_engine_api::{
    EngineCandidate, EngineDescriptor, EngineError, EngineRequest, EngineResponse,
    HardwareInventory, HealthReport, HealthRequest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Fixed bytes preceding every protocol stream.
pub const PREAMBLE: [u8; 8] = *b"FERRODOC";
/// Maximum encoded frame payload accepted by v1 hosts and engines.
pub const MAX_FRAME_LENGTH: u32 = 16 * 1024 * 1024;
/// Current process-protocol version.
pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion(1);
/// Versions supported by this implementation.
pub const SUPPORTED_VERSIONS: VersionRange = VersionRange {
    minimum: CURRENT_PROTOCOL_VERSION,
    maximum: CURRENT_PROTOCOL_VERSION,
};

/// Framing and negotiation failure.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// Stream I/O failed or ended partway through a required field.
    #[error("protocol I/O: {0}")]
    Io(#[from] io::Error),
    /// Peer did not start its stream with the fixed preamble.
    #[error("invalid protocol preamble")]
    InvalidPreamble,
    /// Frame length was zero or exceeded the negotiated hard limit.
    #[error("invalid frame length {actual}; maximum {maximum}")]
    FrameLength {
        /// Length declared by the peer.
        actual: u32,
        /// Effective negotiated maximum.
        maximum: u32,
    },
    /// CBOR payload is malformed, trailing, or incompatible with the requested message.
    #[error("malformed CBOR frame: {0}")]
    MalformedCbor(String),
    /// Host and engine version ranges do not overlap.
    #[error("protocol version mismatch: local {local_min}-{local_max}, peer {peer_min}-{peer_max}")]
    VersionMismatch {
        /// Local minimum.
        local_min: u16,
        /// Local maximum.
        local_max: u16,
        /// Peer minimum.
        peer_min: u16,
        /// Peer maximum.
        peer_max: u16,
    },
}

/// Writes the stream preamble exactly once before any frames.
pub fn write_preamble(writer: &mut impl Write) -> Result<(), ProtocolError> {
    writer.write_all(&PREAMBLE)?;
    writer.flush()?;
    Ok(())
}

/// Reads and validates the stream preamble without scanning for it.
pub fn read_preamble(reader: &mut impl Read) -> Result<(), ProtocolError> {
    let mut preamble = [0_u8; PREAMBLE.len()];
    reader.read_exact(&mut preamble)?;
    if preamble == PREAMBLE {
        Ok(())
    } else {
        Err(ProtocolError::InvalidPreamble)
    }
}

/// Serializes one CBOR item behind a four-byte big-endian length prefix.
pub fn write_frame<T: Serialize>(
    writer: &mut impl Write,
    value: &T,
    negotiated_maximum: u32,
) -> Result<(), ProtocolError> {
    let maximum = negotiated_maximum.min(MAX_FRAME_LENGTH);
    let mut payload = Vec::new();
    ciborium::ser::into_writer(value, &mut payload)
        .map_err(|error| ProtocolError::MalformedCbor(error.to_string()))?;
    let length = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    if length == 0 || length > maximum {
        return Err(ProtocolError::FrameLength {
            actual: length,
            maximum,
        });
    }
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

/// Reads one bounded length-prefixed CBOR item, rejecting size before allocation.
pub fn read_frame<T: for<'de> Deserialize<'de>>(
    reader: &mut impl Read,
    negotiated_maximum: u32,
) -> Result<T, ProtocolError> {
    let maximum = negotiated_maximum.min(MAX_FRAME_LENGTH);
    let mut prefix = [0_u8; 4];
    reader.read_exact(&mut prefix)?;
    let length = u32::from_be_bytes(prefix);
    if length == 0 || length > maximum {
        return Err(ProtocolError::FrameLength {
            actual: length,
            maximum,
        });
    }
    let mut payload = vec![0_u8; length as usize];
    reader.read_exact(&mut payload)?;
    let mut cursor = Cursor::new(payload.as_slice());
    let value = ciborium::de::from_reader(&mut cursor)
        .map_err(|error| ProtocolError::MalformedCbor(error.to_string()))?;
    if cursor.position() != u64::from(length) {
        return Err(ProtocolError::MalformedCbor(
            "trailing bytes after CBOR value".into(),
        ));
    }
    Ok(value)
}

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
    /// Register one already host-scoped immutable byte range for later execution.
    RegisterBlob {
        /// Normalized child-local scope. Its range starts at zero.
        blob: ScopedBlob,
        /// Exact bytes covered by the scope.
        bytes: Vec<u8>,
    },
    /// Release child-local bytes after request completion.
    ReleaseBlob(BlobId),
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
    /// Blob registration succeeded.
    BlobRegistered(BlobId),
    /// Blob release succeeded.
    BlobReleased(BlobId),
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
    use std::io::Cursor;

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

    #[test]
    fn frames_round_trip_and_reject_trailing_cbor() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &HostMessage::Ping, 1024).unwrap();
        assert_eq!(
            read_frame::<HostMessage>(&mut Cursor::new(&bytes), 1024).unwrap(),
            HostMessage::Ping
        );

        let mut trailing = Vec::new();
        ciborium::ser::into_writer(&HostMessage::Ping, &mut trailing).unwrap();
        trailing.push(0);
        let mut framed = (trailing.len() as u32).to_be_bytes().to_vec();
        framed.extend(trailing);
        assert!(matches!(
            read_frame::<HostMessage>(&mut Cursor::new(framed), 1024),
            Err(ProtocolError::MalformedCbor(_))
        ));
    }

    #[test]
    fn oversized_length_is_rejected_before_payload_read() {
        let prefix = (MAX_FRAME_LENGTH + 1).to_be_bytes();
        assert!(matches!(
            read_frame::<HostMessage>(&mut Cursor::new(prefix), MAX_FRAME_LENGTH),
            Err(ProtocolError::FrameLength { .. })
        ));
    }

    #[test]
    fn partial_and_malformed_frames_are_distinct_failures() {
        let mut partial = 8_u32.to_be_bytes().to_vec();
        partial.extend([0xa1, 0x01]);
        assert!(matches!(
            read_frame::<HostMessage>(&mut Cursor::new(partial), 1024),
            Err(ProtocolError::Io(error)) if error.kind() == io::ErrorKind::UnexpectedEof
        ));

        let mut malformed = 2_u32.to_be_bytes().to_vec();
        malformed.extend([0xff, 0xff]);
        assert!(matches!(
            read_frame::<HostMessage>(&mut Cursor::new(malformed), 1024),
            Err(ProtocolError::MalformedCbor(_))
        ));
    }

    #[test]
    fn unframed_output_cannot_pass_the_preamble() {
        assert!(matches!(
            read_preamble(&mut Cursor::new(b"diagnostic text")),
            Err(ProtocolError::InvalidPreamble)
        ));
    }

    #[test]
    fn checked_in_v1_fixtures_have_expected_behavior() {
        let mut hello = Cursor::new(include_bytes!(
            "../../../fixtures/protocol/v1/client-hello.bin"
        ));
        read_preamble(&mut hello).unwrap();
        let hello: ClientHello = read_frame(&mut hello, MAX_FRAME_LENGTH).unwrap();
        assert_eq!(hello.versions, SUPPORTED_VERSIONS);

        let ping: RequestEnvelope = read_frame(
            &mut Cursor::new(include_bytes!(
                "../../../fixtures/protocol/v1/ping-request.bin"
            )),
            MAX_FRAME_LENGTH,
        )
        .unwrap();
        assert_eq!(ping.message, HostMessage::Ping);

        assert!(matches!(
            read_frame::<HostMessage>(
                &mut Cursor::new(include_bytes!(
                    "../../../fixtures/protocol/v1/malformed-cbor.bin"
                )),
                MAX_FRAME_LENGTH
            ),
            Err(ProtocolError::MalformedCbor(_))
        ));
        assert!(matches!(
            read_frame::<HostMessage>(
                &mut Cursor::new(include_bytes!(
                    "../../../fixtures/protocol/v1/oversized-prefix.bin"
                )),
                MAX_FRAME_LENGTH
            ),
            Err(ProtocolError::FrameLength { .. })
        ));
        assert!(matches!(
            read_frame::<HostMessage>(
                &mut Cursor::new(include_bytes!(
                    "../../../fixtures/protocol/v1/partial-frame.bin"
                )),
                MAX_FRAME_LENGTH
            ),
            Err(ProtocolError::Io(error)) if error.kind() == io::ErrorKind::UnexpectedEof
        ));
    }
}
