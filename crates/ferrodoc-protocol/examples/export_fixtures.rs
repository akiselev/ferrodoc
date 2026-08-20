//! Regenerates schema-versioned protocol conformance fixtures.

use std::{error::Error, fs, path::Path};

use ferrodoc_core::RequestId;
use ferrodoc_protocol::{
    CURRENT_PROTOCOL_VERSION, ClientHello, HostMessage, MAX_FRAME_LENGTH, RequestEnvelope,
    ResponseEnvelope, SUPPORTED_VERSIONS, write_frame, write_preamble,
};

fn main() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/protocol/v1");
    fs::create_dir_all(&root)?;

    let mut hello = Vec::new();
    write_preamble(&mut hello)?;
    write_frame(
        &mut hello,
        &ClientHello {
            versions: SUPPORTED_VERSIONS,
            maximum_frame_length: MAX_FRAME_LENGTH,
        },
        MAX_FRAME_LENGTH,
    )?;
    fs::write(root.join("client-hello.bin"), hello)?;

    let mut ping = Vec::new();
    write_frame(
        &mut ping,
        &RequestEnvelope {
            version: CURRENT_PROTOCOL_VERSION,
            request_id: RequestId::derive(&[b"protocol-v1-ping-fixture"]),
            message: HostMessage::Ping,
        },
        MAX_FRAME_LENGTH,
    )?;
    fs::write(root.join("ping-request.bin"), ping)?;

    fs::write(root.join("malformed-cbor.bin"), [0, 0, 0, 2, 0xff, 0xff])?;
    fs::write(
        root.join("oversized-prefix.bin"),
        (MAX_FRAME_LENGTH + 1).to_be_bytes(),
    )?;
    fs::write(root.join("partial-frame.bin"), [0, 0, 0, 8, 0xa1])?;

    let schema_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let mut request_schema = serde_json::to_vec_pretty(&schemars::schema_for!(RequestEnvelope))?;
    request_schema.push(b'\n');
    fs::write(schema_root.join("protocol-request-v1.json"), request_schema)?;
    let mut response_schema = serde_json::to_vec_pretty(&schemars::schema_for!(ResponseEnvelope))?;
    response_schema.push(b'\n');
    fs::write(
        schema_root.join("protocol-response-v1.json"),
        response_schema,
    )?;
    Ok(())
}
