#![cfg(unix)]

use std::{collections::BTreeMap, path::PathBuf};

use ferrodoc_core::{
    BlobId, BlobRange, Capability, MediaType, RequestId, ScopedBlob, Sha256Digest,
};
use ferrodoc_engine_api::{EngineRequest, conformance};
use ferrodoc_engine_command::{Argument, CommandConfig, CommandEngine};

#[test]
fn allowlisted_cat_passes_common_conformance() {
    let input = b"command fixture".to_vec();
    let request = request(&input);
    conformance::run(
        &mut CommandEngine::new(config(
            PathBuf::from("/bin/cat"),
            vec![Argument::InputPath],
            4096,
        ))
        .unwrap(),
        request,
        input,
        &conformance::unknown_inventory(),
    )
    .unwrap();
}

fn config(executable: PathBuf, arguments: Vec<Argument>, max_output_bytes: u64) -> CommandConfig {
    CommandConfig {
        engine_id: "experimental.command.fixture".into(),
        allowed_executables: vec![executable.clone()],
        executable,
        arguments,
        max_output_bytes,
        timeout_ms: 5_000,
        max_ram_bytes: 256 * 1024 * 1024,
        deterministic: true,
    }
}

fn request(input: &[u8]) -> EngineRequest {
    EngineRequest {
        request_id: RequestId::derive(&[b"command-conformance"]),
        capability: Capability::OcrPage,
        input: ScopedBlob {
            id: BlobId::new("command-conformance").unwrap(),
            range: BlobRange::new(0, input.len() as u64).unwrap(),
            media_type: MediaType::new("text/plain").unwrap(),
            expected_digest: Some(Sha256Digest::of_bytes(input)),
        },
        page_index: Some(0),
        scope: None,
        parameters: BTreeMap::new(),
        deterministic_seed: None,
        deadline: None,
    }
}
