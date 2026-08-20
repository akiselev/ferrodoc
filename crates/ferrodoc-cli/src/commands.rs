use std::{collections::BTreeMap, fs, path::Path};

use ferrodoc_core::{
    BlobId, BlobRange, Capability, MediaType, ModelManifest, RequestId, ScopedBlob, Sha256Digest,
};
use ferrodoc_engine_api::{EngineError, EngineRequest};
use ferrodoc_engine_ocrs::{OcrsEngine, RGBA8_MEDIA_TYPE};
#[cfg(feature = "tesseract")]
use ferrodoc_engine_tesseract::TesseractEngine;
use ferrodoc_layout_rulebased::RuleBasedLayoutEngine;
use ferrodoc_pdf::{PdfDocument, PdfLimits};
use ferrodoc_render::render;
use ferrodoc_research::ExperimentLedger;
use ferrodoc_router::{RouterModel, RoutingDataset, TrainingObjective};
use ferrodoc_runtime::{
    ConversionOptions, Converter, PluginCommand, ProcessConfig, ProcessEngine,
    doctor::{DoctorReport, InferenceProbe},
    model_store::{ModelStore, ModelStoreError},
};
use serde::Serialize;
use thiserror::Error;

use crate::{
    args::{
        Command, ConvertArgs, ModelsCommand, PipelineArgs, PluginsDoctorArgs, ResearchCommand,
        RouterCommand,
    },
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
    #[error(transparent)]
    Router(#[from] ferrodoc_router::RouterError),
    #[error(transparent)]
    Research(#[from] ferrodoc_research::ResearchError),
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
            Self::Router(_) => "router",
            Self::Research(_) => "research",
        }
    }
}

pub fn run(command: Command) -> Result<(), CommandError> {
    match command {
        Command::Version => println!("ferrodoc {}", env!("CARGO_PKG_VERSION")),
        Command::Status => println!("Ferrodoc Phase 7 guarded routing runtime"),
        Command::Hardware => print_json(&ferrodoc_runtime::hardware::inventory())?,
        Command::Models(command) => run_models(command)?,
        Command::PluginsDoctor(arguments) => plugins_doctor(arguments)?,
        Command::Router(command) => run_router(command)?,
        Command::Research(command) => run_research(command)?,
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
            let leases: Vec<_> = result
                .resources
                .stages
                .iter()
                .filter_map(|stage| {
                    stage
                        .reservation
                        .as_ref()
                        .map(|reservation| (stage, reservation))
                })
                .map(|(stage, reservation)| {
                    serde_json::json!({
                        "page_index": stage.page_index,
                        "stage": stage.stage,
                        "engine_id": stage.engine_id,
                        "device": stage.device,
                        "reservation": reservation,
                    })
                })
                .collect();
            let cache_decisions: Vec<_> = result
                .resources
                .stages
                .iter()
                .map(|stage| {
                    serde_json::json!({"page_index": stage.page_index, "stage": stage.stage, "decision": stage.cache})
                })
                .collect();
            let measurements: Vec<_> = result
                .resources
                .stages
                .iter()
                .map(|stage| {
                    serde_json::json!({"page_index": stage.page_index, "stage": stage.stage, "measurement": stage.measurement})
                })
                .collect();
            #[derive(Serialize)]
            struct ExplainOutput<'a> {
                #[serde(flatten)]
                trace: &'a ferrodoc_runtime::ConversionTrace,
                resource_plans: &'a [ferrodoc_runtime::planner::PlanningReport],
                leases: Vec<serde_json::Value>,
                cache_decisions: Vec<serde_json::Value>,
                measurements: Vec<serde_json::Value>,
            }
            print_json(&ExplainOutput {
                trace: &result.trace,
                resource_plans: &resource_plans,
                leases,
                cache_decisions,
                measurements,
            })?;
        }
        Command::Convert(arguments) => convert(arguments)?,
    }
    Ok(())
}

fn run_router(command: RouterCommand) -> Result<(), CommandError> {
    let (root, dataset_path) = match &command {
        RouterCommand::Inspect { root, dataset }
        | RouterCommand::Train { root, dataset, .. }
        | RouterCommand::Evaluate { root, dataset, .. }
        | RouterCommand::Compare { root, dataset, .. } => (root, dataset),
    };
    let dataset: RoutingDataset = serde_json::from_slice(&read(dataset_path)?)?;
    dataset.verify_sources(root)?;
    match command {
        RouterCommand::Inspect { .. } => {
            #[derive(Serialize)]
            struct Summary<'a> {
                dataset_version: &'a str,
                feature_schema_version: &'a str,
                corpus_digest: Sha256Digest,
                records: usize,
                partitions: BTreeMap<String, usize>,
                source_verification: &'static str,
            }
            let mut partitions = BTreeMap::new();
            for record in &dataset.records {
                *partitions
                    .entry(format!("{:?}", record.partition).to_lowercase())
                    .or_insert(0) += 1;
            }
            print_json(&Summary {
                dataset_version: &dataset.dataset_version,
                feature_schema_version: &dataset.feature_schema_version,
                corpus_digest: dataset.corpus_digest,
                records: dataset.records.len(),
                partitions,
                source_verification: "passed",
            })?;
        }
        RouterCommand::Train { model, .. } => {
            let trained =
                ferrodoc_router::train_and_evaluate(&dataset, TrainingObjective::default())?;
            write_json_file(&model, &trained)?;
            print_json(&trained)?;
        }
        RouterCommand::Evaluate { model, .. } => {
            let model: RouterModel = serde_json::from_slice(&read(&model)?)?;
            print_json(&ferrodoc_router::evaluate_model(&dataset, &model)?)?;
        }
        RouterCommand::Compare { model, .. } => {
            let model: RouterModel = serde_json::from_slice(&read(&model)?)?;
            print_json(&ferrodoc_router::compare_plans(&dataset, &model)?)?;
        }
    }
    Ok(())
}

fn run_research(command: ResearchCommand) -> Result<(), CommandError> {
    match command {
        ResearchCommand::Run { root, spec, ledger } => {
            print_json(&ferrodoc_research::run(&root, &spec, &ledger)?)?;
        }
        ResearchCommand::Status { ledger } => {
            let ledger: ExperimentLedger = serde_json::from_slice(&read(&ledger)?)?;
            print_json(&ledger)?;
        }
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
    let ocrs_probe = arguments.inference.then(ocr_probe).transpose()?;
    report.inspect_engine(&mut ocr, ocrs_probe);

    #[cfg(feature = "tesseract")]
    {
        let mut tesseract = TesseractEngine::discover("eng");
        let probe = arguments.inference.then(ocr_probe).transpose()?;
        report.inspect_engine(&mut tesseract, probe);
    }

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
    let mut converter = load_converter(
        configuration.options,
        &configuration.ocr_engine,
        configuration.model_dir.as_deref(),
    )?;
    if let Some(cache_dir) = configuration.cache_dir {
        converter.enable_cache(cache_dir)?;
    }
    Ok((converter, bytes))
}

fn load_converter(
    options: ConversionOptions,
    engine: &str,
    model_dir: Option<&Path>,
) -> Result<Converter, CommandError> {
    match (engine, model_dir) {
        ("ocrs", None) => Ok(Converter::new(options)),
        ("ocrs", Some(directory)) => Ok(Converter::with_ocrs_models(
            options,
            read(&directory.join("text-detection.rten"))?,
            read(&directory.join("text-recognition.rten"))?,
        )?),
        #[cfg(feature = "tesseract")]
        ("tesseract", None) => Ok(Converter::with_tesseract(
            options,
            TesseractEngine::discover("eng"),
        )),
        _ => unreachable!("configuration validated OCR engine and model combination"),
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

fn write_json_file(path: &Path, value: &impl Serialize) -> Result<(), CommandError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    output::write(&bytes, Some(path))?;
    Ok(())
}
