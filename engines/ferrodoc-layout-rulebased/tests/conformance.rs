use std::collections::BTreeMap;

use ferrodoc_core::{
    BlobId, BlobRange, Capability, MediaType, RequestId, ScopedBlob, Sha256Digest,
};
use ferrodoc_engine_api::{EngineRequest, conformance};
use ferrodoc_layout_rulebased::RuleBasedLayoutEngine;

#[test]
fn rule_based_engine_passes_common_conformance() {
    let input = b"FIXTURE HEADING\nA deterministic paragraph.".to_vec();
    let request = EngineRequest {
        request_id: RequestId::derive(&[b"layout-conformance"]),
        capability: Capability::LayoutDetect,
        input: scoped(&input, "text/plain"),
        page_index: Some(0),
        scope: None,
        parameters: BTreeMap::from([
            ("page_width".into(), serde_json::json!(595.0)),
            ("page_height".into(), serde_json::json!(842.0)),
        ]),
        deterministic_seed: None,
        deadline: None,
    };
    conformance::run(
        &mut RuleBasedLayoutEngine::new(),
        request,
        input,
        &conformance::unknown_inventory(),
    )
    .unwrap();
}

fn scoped(bytes: &[u8], media_type: &str) -> ScopedBlob {
    ScopedBlob {
        id: BlobId::new("layout-conformance").unwrap(),
        range: BlobRange::new(0, bytes.len() as u64).unwrap(),
        media_type: MediaType::new(media_type).unwrap(),
        expected_digest: Some(Sha256Digest::of_bytes(bytes)),
    }
}
