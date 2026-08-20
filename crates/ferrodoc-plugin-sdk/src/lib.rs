//! Thin framed-process wrapper for any transport-independent [`Engine`].

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read, Write},
    time::{Duration, Instant},
};

use ferrodoc_core::{BlobId, ScopedBlob, Sha256Digest};
use ferrodoc_engine_api::{
    BlobResolver, CancellationToken, Engine, EngineError, EngineErrorCategory, ExecutionContext,
    TraceSink,
};
use ferrodoc_protocol::{
    ClientHello, EngineMessage, HostMessage, MAX_FRAME_LENGTH, ProtocolError, RequestEnvelope,
    ResponseEnvelope, SUPPORTED_VERSIONS, ServerHello, read_frame, read_preamble, write_frame,
    write_preamble,
};
use thiserror::Error;

/// Fatal server-loop failure. Semantic engine errors are returned as protocol messages instead.
#[derive(Debug, Error)]
pub enum ServerError {
    /// Protocol framing or negotiation failed.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// Engine descriptor is invalid.
    #[error(transparent)]
    Engine(#[from] EngineError),
}

/// Runs an engine over stdin/stdout. Diagnostics are written only to stderr.
pub fn run_engine(engine: impl Engine) -> Result<(), ServerError> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    serve(engine, &mut stdin.lock(), &mut stdout.lock())
}

/// Runs the server loop over caller-supplied streams for wrappers and conformance tests.
pub fn serve(
    mut engine: impl Engine,
    reader: &mut impl Read,
    writer: &mut impl Write,
) -> Result<(), ServerError> {
    engine.descriptor().validate()?;
    read_preamble(reader)?;
    let hello: ClientHello = read_frame(reader, MAX_FRAME_LENGTH)?;
    let Some(version) = SUPPORTED_VERSIONS.negotiate(hello.versions) else {
        return Err(ProtocolError::VersionMismatch {
            local_min: SUPPORTED_VERSIONS.minimum.0,
            local_max: SUPPORTED_VERSIONS.maximum.0,
            peer_min: hello.versions.minimum.0,
            peer_max: hello.versions.maximum.0,
        }
        .into());
    };
    if hello.maximum_frame_length == 0 {
        return Err(ProtocolError::FrameLength {
            actual: 0,
            maximum: MAX_FRAME_LENGTH,
        }
        .into());
    }
    let maximum = hello.maximum_frame_length.min(MAX_FRAME_LENGTH);
    write_preamble(writer)?;
    write_frame(
        writer,
        &ServerHello {
            version,
            maximum_frame_length: maximum,
            descriptor: engine.descriptor().clone(),
        },
        maximum,
    )?;

    let mut blobs = BlobRegistry::default();
    let mut seen_requests = BTreeSet::new();
    loop {
        let request: RequestEnvelope = read_frame(reader, maximum)?;
        if request.version != version {
            return Err(ProtocolError::VersionMismatch {
                local_min: version.0,
                local_max: version.0,
                peer_min: request.version.0,
                peer_max: request.version.0,
            }
            .into());
        }
        let request_id = request.request_id.clone();
        let shutdown = matches!(request.message, HostMessage::Shutdown);
        let message = if !seen_requests.insert(request_id.clone()) {
            EngineMessage::Error(engine_error(
                EngineErrorCategory::Protocol,
                "duplicate request ID",
            ))
        } else {
            handle(&mut engine, &mut blobs, request.message)
        };
        write_frame(
            writer,
            &ResponseEnvelope {
                version,
                request_id,
                message,
            },
            maximum,
        )?;
        if shutdown {
            return Ok(());
        }
    }
}

fn handle(
    engine: &mut dyn Engine,
    blobs: &mut BlobRegistry,
    message: HostMessage,
) -> EngineMessage {
    match message {
        HostMessage::RegisterBlob { blob, bytes } => match blobs.register(blob, bytes) {
            Ok(id) => EngineMessage::BlobRegistered(id),
            Err(error) => EngineMessage::Error(error),
        },
        HostMessage::ReleaseBlob(id) => match blobs.release(&id) {
            Ok(()) => EngineMessage::BlobReleased(id),
            Err(error) => EngineMessage::Error(error),
        },
        HostMessage::Discover => EngineMessage::Descriptor(engine.descriptor().clone()),
        HostMessage::Health(request) => engine
            .health(request)
            .map_or_else(EngineMessage::Error, EngineMessage::Health),
        HostMessage::Estimate { request, inventory } => engine
            .estimate(&request, &inventory)
            .map_or_else(EngineMessage::Error, EngineMessage::Estimate),
        HostMessage::Execute(request) => {
            let deadline = request
                .deadline
                .map(|duration| Instant::now() + Duration::from_millis(duration.get()));
            let trace = NoTrace;
            let context = ExecutionContext {
                cancellation: CancellationToken::default(),
                deadline,
                blobs,
                trace: &trace,
            };
            engine
                .execute(request, &context)
                .map_or_else(EngineMessage::Error, EngineMessage::Execute)
        }
        HostMessage::Cancel(request_id) => EngineMessage::Cancelled(request_id),
        HostMessage::Ping => EngineMessage::Pong,
        HostMessage::Shutdown => EngineMessage::Shutdown,
    }
}

#[derive(Default)]
struct BlobRegistry {
    blobs: BTreeMap<BlobId, RegisteredBlob>,
}

struct RegisteredBlob {
    scope: ScopedBlob,
    bytes: Vec<u8>,
}

impl BlobRegistry {
    fn register(&mut self, blob: ScopedBlob, bytes: Vec<u8>) -> Result<BlobId, EngineError> {
        if blob.range.offset() != 0 || blob.range.len() != bytes.len() as u64 {
            return Err(engine_error(
                EngineErrorCategory::InvalidRequest,
                "registered bytes do not match normalized blob range",
            ));
        }
        if blob
            .expected_digest
            .is_some_and(|expected| expected != Sha256Digest::of_bytes(&bytes))
        {
            return Err(engine_error(
                EngineErrorCategory::InvalidRequest,
                "registered blob digest mismatch",
            ));
        }
        if self.blobs.contains_key(&blob.id) {
            return Err(engine_error(
                EngineErrorCategory::Protocol,
                "duplicate blob ID",
            ));
        }
        let id = blob.id.clone();
        self.blobs
            .insert(id.clone(), RegisteredBlob { scope: blob, bytes });
        Ok(id)
    }

    fn release(&mut self, id: &BlobId) -> Result<(), EngineError> {
        self.blobs
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| engine_error(EngineErrorCategory::InvalidRequest, "unknown blob ID"))
    }
}

impl BlobResolver for BlobRegistry {
    fn resolve(&self, blob: &ScopedBlob) -> Result<Vec<u8>, EngineError> {
        let registered = self
            .blobs
            .get(&blob.id)
            .ok_or_else(|| engine_error(EngineErrorCategory::InvalidRequest, "unknown blob ID"))?;
        if blob.media_type != registered.scope.media_type
            || blob.range.offset() < registered.scope.range.offset()
            || blob.range.end() > registered.scope.range.end()
        {
            return Err(engine_error(
                EngineErrorCategory::InvalidRequest,
                "requested blob is outside its registered scope",
            ));
        }
        let start = usize::try_from(blob.range.offset()).map_err(|_| {
            engine_error(EngineErrorCategory::InvalidRequest, "blob offset overflow")
        })?;
        let end = usize::try_from(blob.range.end())
            .map_err(|_| engine_error(EngineErrorCategory::InvalidRequest, "blob end overflow"))?;
        let bytes = registered.bytes.get(start..end).ok_or_else(|| {
            engine_error(EngineErrorCategory::InvalidRequest, "blob range overflow")
        })?;
        if blob
            .expected_digest
            .is_some_and(|expected| expected != Sha256Digest::of_bytes(bytes))
        {
            return Err(engine_error(
                EngineErrorCategory::InvalidRequest,
                "resolved blob digest mismatch",
            ));
        }
        Ok(bytes.to_vec())
    }
}

fn engine_error(category: EngineErrorCategory, message: &str) -> EngineError {
    EngineError::new(category, false, message)
}

struct NoTrace;

impl TraceSink for NoTrace {
    fn event(&self, _code: &str, _fields: &BTreeMap<String, String>) {}
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use ferrodoc_core::{BackendId, BlobRange, Capability, DeviceKind, MediaType, RequestId};
    use ferrodoc_engine_api::{
        EngineCandidate, EngineCompatibility, EngineDescriptor, EngineRequest, EngineResponse,
        HardwareInventory, HealthReport, HealthRequest, NetworkUse,
    };
    use ferrodoc_protocol::{
        CURRENT_PROTOCOL_VERSION, ClientHello, ProtocolVersion, RequestEnvelope, ResponseEnvelope,
        VersionRange,
    };

    use super::*;

    #[test]
    fn registry_enforces_digest_duplicate_and_range_scope() {
        let bytes = b"approved".to_vec();
        let scope = ScopedBlob {
            id: BlobId::new("blob").unwrap(),
            range: BlobRange::new(0, bytes.len() as u64).unwrap(),
            media_type: MediaType::new("text/plain").unwrap(),
            expected_digest: Some(Sha256Digest::of_bytes(&bytes)),
        };
        let mut registry = BlobRegistry::default();
        registry.register(scope.clone(), bytes).unwrap();
        assert_eq!(registry.resolve(&scope).unwrap(), b"approved");
        assert_eq!(
            registry
                .register(scope.clone(), b"approved".to_vec())
                .unwrap_err()
                .category,
            EngineErrorCategory::Protocol
        );
        let mut escaped = scope;
        escaped.range = BlobRange::new(7, 2).unwrap();
        assert_eq!(
            registry.resolve(&escaped).unwrap_err().category,
            EngineErrorCategory::InvalidRequest
        );
    }

    struct Noop {
        descriptor: EngineDescriptor,
    }

    impl Noop {
        fn new() -> Self {
            Self {
                descriptor: EngineDescriptor {
                    id: "test.noop".into(),
                    version: "1.0.0".into(),
                    capabilities: BTreeSet::from([Capability::OcrPage]),
                    compatibility: vec![EngineCompatibility {
                        backend: BackendId::new("noop").unwrap(),
                        devices: BTreeSet::from([DeviceKind::Cpu]),
                    }],
                    deterministic: true,
                    network_use: NetworkUse::None,
                    max_concurrency: 1,
                },
            }
        }
    }

    impl Engine for Noop {
        fn descriptor(&self) -> &EngineDescriptor {
            &self.descriptor
        }
        fn health(&mut self, _request: HealthRequest) -> Result<HealthReport, EngineError> {
            unreachable!()
        }
        fn estimate(
            &mut self,
            _request: &EngineRequest,
            _inventory: &HardwareInventory,
        ) -> Result<Vec<EngineCandidate>, EngineError> {
            unreachable!()
        }
        fn execute(
            &mut self,
            _request: EngineRequest,
            _context: &ExecutionContext<'_>,
        ) -> Result<EngineResponse, EngineError> {
            unreachable!()
        }
    }

    #[test]
    fn duplicate_request_ids_return_protocol_error_without_desynchronizing() {
        let request_id = RequestId::derive(&[b"duplicate"]);
        let shutdown_id = RequestId::derive(&[b"shutdown"]);
        let mut input = Vec::new();
        write_preamble(&mut input).unwrap();
        write_frame(
            &mut input,
            &ClientHello {
                versions: SUPPORTED_VERSIONS,
                maximum_frame_length: 4096,
            },
            4096,
        )
        .unwrap();
        for message in [HostMessage::Ping, HostMessage::Ping] {
            write_frame(
                &mut input,
                &RequestEnvelope {
                    version: CURRENT_PROTOCOL_VERSION,
                    request_id: request_id.clone(),
                    message,
                },
                4096,
            )
            .unwrap();
        }
        write_frame(
            &mut input,
            &RequestEnvelope {
                version: CURRENT_PROTOCOL_VERSION,
                request_id: shutdown_id.clone(),
                message: HostMessage::Shutdown,
            },
            4096,
        )
        .unwrap();
        let mut output = Vec::new();
        serve(Noop::new(), &mut Cursor::new(input), &mut output).unwrap();

        let mut output = Cursor::new(output);
        read_preamble(&mut output).unwrap();
        let _: ServerHello = read_frame(&mut output, 4096).unwrap();
        let first: ResponseEnvelope = read_frame(&mut output, 4096).unwrap();
        let duplicate: ResponseEnvelope = read_frame(&mut output, 4096).unwrap();
        let shutdown: ResponseEnvelope = read_frame(&mut output, 4096).unwrap();
        assert_eq!(first.message, EngineMessage::Pong);
        assert!(matches!(
            duplicate.message,
            EngineMessage::Error(EngineError {
                category: EngineErrorCategory::Protocol,
                ..
            })
        ));
        assert_eq!(shutdown.request_id, shutdown_id);
        assert_eq!(shutdown.message, EngineMessage::Shutdown);
    }

    #[test]
    fn negotiation_error_reports_both_version_ranges() {
        let mut input = Vec::new();
        write_preamble(&mut input).unwrap();
        write_frame(
            &mut input,
            &ClientHello {
                versions: VersionRange {
                    minimum: ProtocolVersion(9),
                    maximum: ProtocolVersion(10),
                },
                maximum_frame_length: 4096,
            },
            4096,
        )
        .unwrap();
        let error = serve(Noop::new(), &mut Cursor::new(input), &mut Vec::new()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("local 1-1"));
        assert!(message.contains("peer 9-10"));
    }
}
