use std::collections::BTreeMap;

use ferrodoc_core::{
    BlobId, BlobRange, Capability, MediaType, RequestId, ScopedBlob, Sha256Digest,
};
use ferrodoc_engine_api::{EngineRequest, conformance};
use ferrodoc_engine_mock::MockEngine;

#[test]
fn mock_engine_passes_common_conformance() {
    let input = b"fixture".to_vec();
    let request = EngineRequest {
        request_id: RequestId::derive(&[b"mock-conformance"]),
        capability: Capability::OcrPage,
        input: ScopedBlob {
            id: BlobId::new("mock-conformance").unwrap(),
            range: BlobRange::new(0, input.len() as u64).unwrap(),
            media_type: MediaType::new("text/plain").unwrap(),
            expected_digest: Some(Sha256Digest::of_bytes(&input)),
        },
        page_index: Some(0),
        parameters: BTreeMap::new(),
        deterministic_seed: None,
        deadline: None,
    };
    conformance::run(
        &mut MockEngine::new(),
        request,
        input,
        &conformance::unknown_inventory(),
    )
    .unwrap();
}
