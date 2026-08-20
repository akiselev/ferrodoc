use std::{collections::BTreeMap, fs, path::Path};

use ferrodoc_core::{
    BlobId, BlobRange, Capability, MediaType, ModelManifest, RequestId, ScopedBlob, Sha256Digest,
};
use ferrodoc_engine_api::{EngineError, EngineRequest};
use ferrodoc_engine_ocrs::{OcrsEngine, RGBA8_MEDIA_TYPE};
use ferrodoc_layout_rulebased::RuleBasedLayoutEngine;
use ferrodoc_pdf::{PdfDocument, PdfLimits};
use ferrodoc_render::render;
use ferrodoc_runtime::{
    ConversionOptions, Converter, PluginCommand, ProcessConfig, ProcessEngine,
    doctor::{DoctorReport, InferenceProbe},
    model_store::{ModelStore, ModelStoreError},
};
use serde::Serialize;
use thiserror::Error;

use crate::{
    args::{Command, ConvertArgs, ModelsCommand, PipelineArgs, PluginsDoctorArgs},
    configuration::{Configuration, ConfigurationError},
    output::{self, OutputError},
};

#[derive(Debug, Error)]
pub enum CommandError {
    #[error(transparent)]
    Configuration(#[from] ConfigurationError),
    #[error("read {path:?}: {source}")]
    Read {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error(transparent)]
    Pdf(#[from] ferrodoc_pdf::PdfError),
    #[error(transparent)]
    Runtime(#[from] ferrodoc_runtime::RuntimeError),
    #[error(transparent)]
    Render(#[from] ferrodoc_render::RenderError),
    #[error("serialize command output: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error(transparent)]
    Output(#[from] OutputError),
    #[error(transparent)]
    ModelStore(#[from] ModelStoreError),
    #[error(transparent)]
    Engine(#[from] EngineError),
}

impl CommandError {
    pub fn category(&self) -> &'static str {
        match self {
            Self::Configuration(_) => "configuration",
            Self::Read { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
                "missing_input"
            }
            Self::Read { .. } | Self::Output(_) => "io",
            Self::Pdf(ferrodoc_pdf::PdfError::Malformed(_)) => "malformed_pdf",
            Self::Pdf(ferrodoc_pdf::PdfError::Encrypted) => "encrypted_pdf",
            Self::Pdf(ferrodoc_pdf::PdfError::Unsupported(_)) => "unsupported_pdf",
            Self::Pdf(ferrodoc_pdf::PdfError::LimitExceeded { .. }) => "limit_exceeded",
            Self::Pdf(ferrodoc_pdf::PdfError::PageOutOfRange(_)) => "page_out_of_range",
            Self::Runtime(ferrodoc_runtime::RuntimeError::OcrUnavailable { .. }) => {
                "model_unavailable"
            }
            Self::Runtime(_) => "runtime",
            Self::Render(_) => "render",
            Self::Serialize(_) => "serialization",
            Self::ModelStore(_) => "model_store",
            Self::Engine(_) => "engine",
        }
    }
}

pub fn run(command: Command) -> Result<(), CommandError> {
    match command {
        Command::Version => println!("ferrodoc {}", env!("CARGO_PKG_VERSION")),
        Command::Status => println!("Ferrodoc Phase 4 resource-aware runtime"),
        Command::Hardware => print_json(&ferrodoc_runtime::hardware::inventory())?,
        Command::Models(command) => run_models(command)?,
        Command::PluginsDoctor(arguments) => plugins_doctor(arguments)?,
        Command::Inspect { input } => {
            let bytes = read(&input)?;
            let pdf = PdfDocument::from_bytes(bytes, PdfLimits::default())?;
            print_json(pdf.inspection())?;
        }
        Command::Plan(arguments) => {
            let (mut converter, bytes) = converter_and_input(arguments)?;
            let pipeline = converter.plan(&bytes)?;
            let inventory = ferrodoc_runtime::hardware::inventory();
            let resource_plans = converter.resource_plans(&inventory)?;
            #[derive(Serialize)]
            struct PlanOutput<'a> {
                #[serde(flatten)]
                pipeline: &'a ferrodoc_runtime::ConversionPlan,
                inventory: &'a ferrodoc_engine_api::HardwareInventory,
                resource_plans: &'a [ferrodoc_runtime::planner::PlanningReport],
            }
            print_json(&PlanOutput {
                pipeline: &pipeline,
                inventory: &inventory,
                resource_plans: &resource_plans,
            })?;
        }
        Command::Explain(arguments) => {
            let (mut converter, bytes) = converter_and_input(arguments)?;
            let result = converter.convert(bytes)?;
            let inventory = ferrodoc_runtime::hardware::inventory();
            let resource_plans = converter.resource_plans(&inventory)?;
            let leases: Vec<_> = resource_plans
                .iter()
                .filter_map(|report| report.selected.as_ref())
                .map(|candidate| {
                    serde_json::json!({
                        "engine_id": candidate.engine_id,
                        "device": candidate.device,
                        "reservation": candidate.resources,
                    })
                })
                .collect();
            #[derive(Serialize)]
            struct ExplainOutput<'a> {
                #[serde(flatten)]
                trace: &'a ferrodoc_runtime::ConversionTrace,
                resource_plans: &'a [ferrodoc_runtime::planner::PlanningReport],
                leases: Vec<serde_json::Value>,
                cache_decisions: serde_json::Value,
                measurements: serde_json::Value,
            }
            print_json(&ExplainOutput {
                trace: &result.trace,
                resource_plans: &resource_plans,
                leases,
                cache_decisions: serde_json::json!({"status": "not_configured", "eligible_stages": ["layout.rulebased", "ocr.ocrs"]}),
                measurements: serde_json::json!({"status": "unknown", "reason": "platform process metrics are unavailable for embedded execution"}),
            })?;
        }
        Command::Convert(arguments) => convert(arguments)?,
    }
    Ok(())
}

fn run_models(command: ModelsCommand) -> Result<(), CommandError> {
    match command {
        ModelsCommand::List { store } => print_json(&ModelStore::open(store)?.list()?)?,
        ModelsCommand::Verify { store } => print_json(&ModelStore::open(store)?.verify_all()?)?,
        ModelsCommand::Gc { store } => print_json(&ModelStore::open(store)?.garbage_collect()?)?,
        ModelsCommand::Pull {
            store,
            manifest,
            source,
            accept,
        } => {
            let manifest: ModelManifest = serde_json::from_slice(&read(&manifest)?)?;
            let installed =
                ModelStore::open(store)?.install_from_directory(&manifest, &source, accept)?;
            print_json(&installed)?;
        }
    }
    Ok(())
}

fn plugins_doctor(arguments: PluginsDoctorArgs) -> Result<(), CommandError> {
    let mut report = DoctorReport::default();
    let mut layout = RuleBasedLayoutEngine::new();
    let layout_probe = arguments.inference.then(layout_probe).transpose()?;
    report.inspect_engine(&mut layout, layout_probe);

    let mut ocr = match arguments.model_dir {
        Some(directory) => OcrsEngine::from_model_bytes(
            read(&directory.join("text-detection.rten"))?,
            read(&directory.join("text-recognition.rten"))?,
        )?,
        None => OcrsEngine::without_models(),
    };
    let ocr_probe = arguments.inference.then(ocr_probe).transpose()?;
    report.inspect_engine(&mut ocr, ocr_probe);

    for executable in arguments.plugins {
        let identity = executable.display().to_string();
        match PluginCommand::explicit(executable)
            .and_then(|command| ProcessEngine::spawn(&command, ProcessConfig::default()))
        {
            Ok(mut engine) => report.inspect_engine(&mut engine, None),
            Err(error) => report.discovery_failure(identity, error.message),
        }
    }
    print_json(&report)?;
    Ok(())
}

fn layout_probe() -> Result<InferenceProbe, EngineError> {
    let bytes = b"DOCTOR HEADING\nA deterministic paragraph.".to_vec();
    InferenceProbe::new(
        probe_request(
            Capability::LayoutDetect,
            BTreeMap::from([
                ("page_width".into(), serde_json::json!(595.0)),
                ("page_height".into(), serde_json::json!(842.0)),
            ]),
        ),
        bytes,
        "text/plain",
    )
}

fn ocr_probe() -> Result<InferenceProbe, EngineError> {
    let pdf = PdfDocument::from_bytes(
        include_bytes!("../../../fixtures/pdf/image-only.pdf").to_vec(),
        PdfLimits::default(),
    )
    .map_err(|error| {
        EngineError::new(
            ferrodoc_engine_api::EngineErrorCategory::Internal,
            false,
            error.to_string(),
        )
    })?;
    let raster = pdf.render_page(0, 144).map_err(|error| {
        EngineError::new(
            ferrodoc_engine_api::EngineErrorCategory::Internal,
            false,
            error.to_string(),
        )
    })?;
    InferenceProbe::new(
        probe_request(
            Capability::OcrPage,
            BTreeMap::from([
                ("width".into(), serde_json::json!(raster.width)),
                ("height".into(), serde_json::json!(raster.height)),
                ("dpi".into(), serde_json::json!(144)),
            ]),
        ),
        raster.rgba,
        RGBA8_MEDIA_TYPE,
    )
}

fn probe_request(
    capability: Capability,
    parameters: BTreeMap<String, serde_json::Value>,
) -> EngineRequest {
    EngineRequest {
        request_id: RequestId::derive(&[b"cli-doctor", capability.to_string().as_bytes()]),
        capability,
        input: ScopedBlob {
            id: BlobId::new("placeholder").expect("static blob ID"),
            range: BlobRange::new(0, 1).expect("valid placeholder range"),
            media_type: MediaType::new("application/octet-stream").expect("static media type"),
            expected_digest: Some(Sha256Digest::of_bytes(&[0])),
        },
        page_index: Some(0),
        parameters,
        deterministic_seed: None,
        deadline: None,
    }
}

fn convert(arguments: ConvertArgs) -> Result<(), CommandError> {
    let format = arguments.format;
    let output_path = arguments.output;
    let (mut converter, bytes) = converter_and_input(arguments.pipeline)?;
    let result = converter.convert(bytes)?;
    let rendered = render(&result.document, format)?;
    output::write(&rendered, output_path.as_deref())?;
    Ok(())
}

fn converter_and_input(arguments: PipelineArgs) -> Result<(Converter, Vec<u8>), CommandError> {
    let configuration = Configuration::load(arguments)?;
    let bytes = read(&configuration.input)?;
    let converter = load_converter(configuration.options, configuration.model_dir.as_deref())?;
    Ok((converter, bytes))
}

fn load_converter(
    options: ConversionOptions,
    model_dir: Option<&Path>,
) -> Result<Converter, CommandError> {
    match model_dir {
        None => Ok(Converter::new(options)),
        Some(directory) => Ok(Converter::with_ocrs_models(
            options,
            read(&directory.join("text-detection.rten"))?,
            read(&directory.join("text-recognition.rten"))?,
        )?),
    }
}

fn read(path: &Path) -> Result<Vec<u8>, CommandError> {
    fs::read(path).map_err(|source| CommandError::Read {
        path: path.to_owned(),
        source,
    })
}

fn print_json(value: &impl Serialize) -> Result<(), CommandError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    output::write(&bytes, None)?;
    Ok(())
}
