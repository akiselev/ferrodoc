use std::{collections::BTreeMap, fs, path::PathBuf};

use ferrodoc_core::{
    BlobId, BlobRange, Capability, MediaType, RequestId, ScopedBlob, Sha256Digest,
};
use ferrodoc_engine_api::{EngineRequest, conformance};
use ferrodoc_engine_ocrs::{OcrsEngine, RGBA8_MEDIA_TYPE};
use ferrodoc_pdf::{PdfDocument, PdfLimits};

#[test]
fn ocrs_engine_passes_common_conformance_when_models_are_provided() {
    let Some(model_dir) = std::env::var_os("FERRODOC_TEST_OCRS_MODEL_DIR") else {
        return;
    };
    let model_dir = PathBuf::from(model_dir);
    let mut engine = OcrsEngine::from_model_bytes(
        fs::read(model_dir.join("text-detection.rten")).unwrap(),
        fs::read(model_dir.join("text-recognition.rten")).unwrap(),
    )
    .unwrap();
    let pdf = PdfDocument::from_bytes(
        include_bytes!("../../../fixtures/pdf/image-only.pdf").to_vec(),
        PdfLimits::default(),
    )
    .unwrap();
    let raster = pdf.render_page(0, 96).unwrap();
    let request = EngineRequest {
        request_id: RequestId::derive(&[b"ocrs-conformance"]),
        capability: Capability::OcrPage,
        input: ScopedBlob {
            id: BlobId::new("ocrs-conformance").unwrap(),
            range: BlobRange::new(0, raster.rgba.len() as u64).unwrap(),
            media_type: MediaType::new(RGBA8_MEDIA_TYPE).unwrap(),
            expected_digest: Some(Sha256Digest::of_bytes(&raster.rgba)),
        },
        page_index: Some(0),
        scope: None,
        parameters: BTreeMap::from([
            ("width".into(), serde_json::json!(raster.width)),
            ("height".into(), serde_json::json!(raster.height)),
            ("dpi".into(), serde_json::json!(96)),
        ]),
        deterministic_seed: None,
        deadline: None,
    };
    conformance::run(
        &mut engine,
        request,
        raster.rgba,
        &conformance::unknown_inventory(),
    )
    .unwrap();
}
