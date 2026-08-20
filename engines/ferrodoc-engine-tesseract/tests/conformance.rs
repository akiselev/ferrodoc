use std::collections::BTreeMap;

use ferrodoc_core::{
    BlobId, BlobRange, Capability, MediaType, RequestId, ScopedBlob, Sha256Digest,
};
use ferrodoc_engine_api::{Engine, EngineRequest, HealthRequest, HealthStatus, conformance};
use ferrodoc_engine_tesseract::{RGBA8_MEDIA_TYPE, TesseractEngine};
use ferrodoc_pdf::{PdfDocument, PdfLimits};

#[test]
fn tesseract_engine_passes_common_conformance_when_dependency_is_available() {
    let mut engine = TesseractEngine::discover("eng");
    let health = engine.health(HealthRequest::Dependencies).unwrap();
    if health.status != HealthStatus::Healthy {
        assert!(
            std::env::var_os("FERRODOC_REQUIRE_TESSERACT").is_none(),
            "required Tesseract unavailable: {}",
            health.message
        );
        return;
    }
    let pdf = PdfDocument::from_bytes(
        include_bytes!("../../../fixtures/pdf/image-only.pdf").to_vec(),
        PdfLimits::default(),
    )
    .unwrap();
    let raster = pdf.render_page(0, 144).unwrap();
    let request = EngineRequest {
        request_id: RequestId::derive(&[b"tesseract-conformance"]),
        capability: Capability::OcrPage,
        input: ScopedBlob {
            id: BlobId::new("tesseract-conformance").unwrap(),
            range: BlobRange::new(0, raster.rgba.len() as u64).unwrap(),
            media_type: MediaType::new(RGBA8_MEDIA_TYPE).unwrap(),
            expected_digest: Some(Sha256Digest::of_bytes(&raster.rgba)),
        },
        page_index: Some(0),
        parameters: BTreeMap::from([
            ("width".into(), serde_json::json!(raster.width)),
            ("height".into(), serde_json::json!(raster.height)),
            ("dpi".into(), serde_json::json!(144)),
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
