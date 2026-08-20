use std::{collections::BTreeMap, fs, path::PathBuf, time::Duration};

use ferrodoc_core::{
    BlobId, BlobRange, Capability, MediaType, RequestId, ScopedBlob, Sha256Digest,
};
use ferrodoc_engine_api::{
    BlobResolver, CancellationToken, Engine, EngineError, EngineRequest, ExecutionContext,
    TraceSink,
};
use ferrodoc_engine_ocrs::{OcrsEngine, RGBA8_MEDIA_TYPE};
#[cfg(unix)]
use ferrodoc_engine_command::{Argument, CommandConfig, CommandEngine};
#[cfg(feature = "tesseract")]
use ferrodoc_engine_tesseract::TesseractEngine;
use ferrodoc_layout_rulebased::RuleBasedLayoutEngine;
use ferrodoc_pdf::{PdfDocument, PdfLimits};
use ferrodoc_runtime::{PluginCommand, ProcessConfig, ProcessEngine};

struct Resolver(Vec<u8>);

impl BlobResolver for Resolver {
    fn resolve(&self, _blob: &ScopedBlob) -> Result<Vec<u8>, EngineError> {
        Ok(self.0.clone())
    }
}

struct Trace;

impl TraceSink for Trace {
    fn event(&self, _code: &str, _fields: &BTreeMap<String, String>) {}
}

fn context(resolver: &Resolver) -> ExecutionContext<'_> {
    ExecutionContext {
        cancellation: CancellationToken::default(),
        deadline: None,
        blobs: resolver,
        trace: &Trace,
    }
}

fn scoped(bytes: &[u8], id: &str, media_type: &str) -> ScopedBlob {
    ScopedBlob {
        id: BlobId::new(id).unwrap(),
        range: BlobRange::new(0, bytes.len() as u64).unwrap(),
        media_type: MediaType::new(media_type).unwrap(),
        expected_digest: Some(Sha256Digest::of_bytes(bytes)),
    }
}

#[test]
fn layout_wrapper_matches_embedded_when_binary_is_provided() {
    let Some(binary) = std::env::var_os("FERRODOC_TEST_LAYOUT_BINARY") else {
        return;
    };
    let bytes = b"FIXTURE HEADING\nA wrapped paragraph\nends here.".to_vec();
    let request = EngineRequest {
        request_id: RequestId::derive(&[b"layout-process-parity"]),
        capability: Capability::LayoutDetect,
        input: scoped(&bytes, "layout-input", "text/plain"),
        page_index: Some(0),
        parameters: BTreeMap::from([
            ("page_width".into(), serde_json::json!(595.0)),
            ("page_height".into(), serde_json::json!(842.0)),
        ]),
        deterministic_seed: None,
        deadline: None,
    };
    let resolver = Resolver(bytes);
    let context = context(&resolver);
    let mut embedded = RuleBasedLayoutEngine::new();
    let command = PluginCommand::explicit(PathBuf::from(binary)).unwrap();
    let mut process = ProcessEngine::spawn(&command, ProcessConfig::default()).unwrap();
    assert_eq!(
        embedded.execute(request.clone(), &context).unwrap(),
        process.execute(request, &context).unwrap()
    );
}

#[test]
fn ocrs_wrapper_matches_embedded_when_binary_and_models_are_provided() {
    let (Some(binary), Some(model_dir)) = (
        std::env::var_os("FERRODOC_TEST_OCRS_BINARY"),
        std::env::var_os("FERRODOC_TEST_OCRS_MODEL_DIR"),
    ) else {
        return;
    };
    let model_dir = PathBuf::from(model_dir);
    let detection = fs::read(model_dir.join("text-detection.rten")).unwrap();
    let recognition = fs::read(model_dir.join("text-recognition.rten")).unwrap();
    let mut embedded = OcrsEngine::from_model_bytes(detection, recognition).unwrap();
    let pdf = PdfDocument::from_bytes(
        include_bytes!("../../../fixtures/pdf/image-only.pdf").to_vec(),
        PdfLimits::default(),
    )
    .unwrap();
    let raster = pdf.render_page(0, 144).unwrap();
    let request = EngineRequest {
        request_id: RequestId::derive(&[b"ocrs-process-parity"]),
        capability: Capability::OcrPage,
        input: scoped(&raster.rgba, "ocrs-input", RGBA8_MEDIA_TYPE),
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
    let context = context(&resolver);
    let command = PluginCommand::explicit(PathBuf::from(binary))
        .unwrap()
        .environment("FERRODOC_OCRS_MODEL_DIR", model_dir.into_os_string());
    let config = ProcessConfig {
        request_timeout: Duration::from_secs(180),
        ..ProcessConfig::default()
    };
    let mut process = ProcessEngine::spawn(&command, config).unwrap();
    assert_eq!(
        embedded.execute(request.clone(), &context).unwrap(),
        process.execute(request, &context).unwrap()
    );
}

#[cfg(feature = "tesseract")]
#[test]
fn tesseract_wrapper_matches_embedded_when_binary_and_dependency_are_provided() {
    let Some(binary) = std::env::var_os("FERRODOC_TEST_TESSERACT_BINARY") else {
        return;
    };
    let mut embedded = TesseractEngine::discover("eng");
    let pdf = PdfDocument::from_bytes(
        include_bytes!("../../../fixtures/pdf/image-only.pdf").to_vec(),
        PdfLimits::default(),
    )
    .unwrap();
    let raster = pdf.render_page(0, 144).unwrap();
    let request = EngineRequest {
        request_id: RequestId::derive(&[b"tesseract-process-parity"]),
        capability: Capability::OcrPage,
        input: scoped(&raster.rgba, "tesseract-input", RGBA8_MEDIA_TYPE),
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
    let context = context(&resolver);
    let command = PluginCommand::explicit(PathBuf::from(binary)).unwrap();
    let config = ProcessConfig {
        request_timeout: Duration::from_secs(180),
        ..ProcessConfig::default()
    };
    let mut process = ProcessEngine::spawn(&command, config).unwrap();
    assert_eq!(
        embedded.execute(request.clone(), &context).unwrap(),
        process.execute(request, &context).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn experimental_command_wrapper_matches_embedded_without_a_shell() {
    let Some(binary) = std::env::var_os("FERRODOC_TEST_COMMAND_BINARY") else {
        return;
    };
    let config = CommandConfig {
        engine_id: "experimental.command.parity".into(),
        executable: PathBuf::from("/bin/cat"),
        allowed_executables: vec![PathBuf::from("/bin/cat")],
        arguments: vec![Argument::InputPath],
        max_output_bytes: 4096,
        timeout_ms: 5_000,
        max_ram_bytes: 256 * 1024 * 1024,
        deterministic: true,
    };
    let mut embedded = CommandEngine::new(config.clone()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let config_path = directory.path().join("command.json");
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let bytes = b"command parity fixture".to_vec();
    let request = EngineRequest {
        request_id: RequestId::derive(&[b"command-process-parity"]),
        capability: Capability::OcrPage,
        input: scoped(&bytes, "command-input", "text/plain"),
        page_index: Some(0),
        parameters: BTreeMap::new(),
        deterministic_seed: None,
        deadline: None,
    };
    let resolver = Resolver(bytes);
    let context = context(&resolver);
    let command = PluginCommand::explicit(PathBuf::from(binary))
        .unwrap()
        .environment("FERRODOC_COMMAND_CONFIG", config_path.into_os_string());
    let mut process = ProcessEngine::spawn(&command, ProcessConfig::default()).unwrap();
    assert_eq!(
        embedded.execute(request.clone(), &context).unwrap(),
        process.execute(request, &context).unwrap()
    );
}
