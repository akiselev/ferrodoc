//! Runs the real OCRS engine against the first page of a PDF for development validation.

use std::{collections::BTreeMap, env, error::Error, fs, path::Path};

use ferrodoc_core::{
    BlobId, BlobRange, Capability, MediaType, RequestId, ScopedBlob, Sha256Digest,
};
use ferrodoc_engine_api::{
    BlobResolver, CancellationToken, Engine, EngineError, EngineRequest, ExecutionContext,
    TraceSink,
};
use ferrodoc_engine_ocrs::{OcrsEngine, RGBA8_MEDIA_TYPE};
use ferrodoc_pdf::{PdfDocument, PdfLimits};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let model_dir = arguments
        .next()
        .ok_or("usage: ocr_pdf <model-dir> <file.pdf>")?;
    let pdf_path = arguments
        .next()
        .ok_or("usage: ocr_pdf <model-dir> <file.pdf>")?;
    if arguments.next().is_some() {
        return Err("usage: ocr_pdf <model-dir> <file.pdf>".into());
    }
    let model_dir = Path::new(&model_dir);
    let mut engine = OcrsEngine::from_model_bytes(
        fs::read(model_dir.join("text-detection.rten"))?,
        fs::read(model_dir.join("text-recognition.rten"))?,
    )?;
    let pdf = PdfDocument::from_bytes(fs::read(pdf_path)?, PdfLimits::default())?;
    let raster = pdf.render_page(0, 144)?;
    let digest = Sha256Digest::of_bytes(&raster.rgba);
    let request = EngineRequest {
        request_id: RequestId::derive(&[b"ocrs-pdf-probe"]),
        capability: Capability::OcrPage,
        input: ScopedBlob {
            id: BlobId::new("page-rgba").expect("static blob ID"),
            range: BlobRange::new(0, raster.rgba.len() as u64)?,
            media_type: MediaType::new(RGBA8_MEDIA_TYPE)?,
            expected_digest: Some(digest),
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
    let resolver = Resolver(raster.rgba);
    let context = ExecutionContext {
        cancellation: CancellationToken::default(),
        deadline: None,
        blobs: &resolver,
        trace: &Trace,
    };
    let response = engine.execute(request, &context)?;
    for evidence in response.evidence {
        if let ferrodoc_ir::EvidenceContent::Text { text } = evidence.content {
            println!("{text}");
        }
    }
    Ok(())
}

struct Resolver(Vec<u8>);

impl BlobResolver for Resolver {
    fn resolve(&self, blob: &ScopedBlob) -> Result<Vec<u8>, EngineError> {
        let start = usize::try_from(blob.range.offset()).expect("probe range fits usize");
        let end = usize::try_from(blob.range.end()).expect("probe range fits usize");
        self.0.get(start..end).map(<[u8]>::to_vec).ok_or_else(|| {
            EngineError::new(
                ferrodoc_engine_api::EngineErrorCategory::InvalidRequest,
                false,
                "blob range is outside probe data",
            )
        })
    }
}

struct Trace;

impl TraceSink for Trace {
    fn event(&self, _code: &str, _fields: &BTreeMap<String, String>) {}
}
