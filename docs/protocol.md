# Process protocol v1

Ferrodoc protocol v1 carries the same `Engine` semantics used by embedded execution over a bounded child process. It is a pre-release protocol: v1 is supported by Ferrodoc 0.2, and incompatible wire changes require a new integer version plus retained conformance fixtures for every supported version.

## Stream and framing

Host stdin and engine stdout are independent streams. Each begins exactly once with the eight bytes `FERRODOC`. Every subsequent item is a four-byte unsigned big-endian payload length followed by one CBOR value. The v1 hard maximum is 16 MiB, further reduced to the smaller maximum announced during negotiation. Zero and oversized lengths are rejected before payload allocation. Truncated, malformed, and trailing CBOR are errors.

The host writes a framed `ClientHello` after its preamble. The engine writes its own preamble and framed `ServerHello`. Both advertise bounds; the newest overlapping version is selected. A mismatch reports both supported ranges. Unframed engine stdout fails the preamble and cannot be interpreted as a message. Engine diagnostics belong only on stderr, which the host drains continuously while retaining a bounded tail.

## Message lifecycle

Every request and response has a version and correlation ID. IDs may appear only once per process session. The host registers an already approved blob range before execution, normalizing the child-visible range to start at zero. The child rejects duplicate tokens, digest mismatch, media-type mismatch, range overflow, and unknown release. It receives no host path. The host releases the token after execution.

Health, estimate, execute, ping, cancellation acknowledgement, and shutdown have typed messages. Semantic engine failures remain structured `EngineError` values. Framing, negotiation, correlation, and child-lifecycle failures map to protocol errors. The host never automatically retries an in-flight semantic request. An explicit restart is allowed only within `ProcessConfig::maximum_restarts`.

## Lifecycle bounds

Startup, request, semantic deadline, cancellation polling, and graceful shutdown are bounded. A startup hang, execution hang, cancellation, malformed response, crash, or disconnected stream terminates and waits for the child. After termination the engine is unavailable until an explicit bounded restart. Drop attempts graceful shutdown and then kills as a final bound.

## Discovery policy

`PluginCommand::explicit` requires an absolute executable file. Trusted discovery checks one exact filename under explicit canonical roots and rejects traversal or symlink escape. There is no PATH or current-directory plugin discovery. Child environments are cleared and populated only with host-approved entries.

## Conformance artifacts

`fixtures/protocol/v1/` contains the exact valid hello/ping frames and malformed, oversized, and partial inputs. `schemas/protocol-request-v1.json` and `schemas/protocol-response-v1.json` snapshot the message envelopes. Regenerate them with:

```bash
cargo run -p ferrodoc-protocol --example export_fixtures
```

The mock process suite covers embedded/process parity, crash, startup and execution hangs, cancellation, garbage stdout, partial and oversized frames, stderr flooding, shutdown, bounded restart, traversal, and symlink escape. Layout and OCRS have separate embedded/process parity checks; OCRS remains in the verified-model CI job.
