use std::{fs, path::Path};

use ferrodoc_pdf::{PdfDocument, PdfLimits};
use ferrodoc_render::render;
use ferrodoc_runtime::{ConversionOptions, Converter};
use serde::Serialize;
use thiserror::Error;

use crate::{
    args::{Command, ConvertArgs, PipelineArgs},
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
        }
    }
}

pub fn run(command: Command) -> Result<(), CommandError> {
    match command {
        Command::Version => println!("ferrodoc {}", env!("CARGO_PKG_VERSION")),
        Command::Status => println!("Ferrodoc Phase 2 offline PDF vertical slice"),
        Command::Hardware => print_json(&ferrodoc_runtime::hardware::inventory())?,
        Command::Inspect { input } => {
            let bytes = read(&input)?;
            let pdf = PdfDocument::from_bytes(bytes, PdfLimits::default())?;
            print_json(pdf.inspection())?;
        }
        Command::Plan(arguments) => {
            let (mut converter, bytes) = converter_and_input(arguments)?;
            print_json(&converter.plan(&bytes)?)?;
        }
        Command::Explain(arguments) => {
            let (mut converter, bytes) = converter_and_input(arguments)?;
            print_json(&converter.convert(bytes)?.trace)?;
        }
        Command::Convert(arguments) => convert(arguments)?,
    }
    Ok(())
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
