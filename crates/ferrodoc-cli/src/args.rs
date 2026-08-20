use std::{ffi::OsString, path::PathBuf};

use ferrodoc_render::OutputFormat;
use thiserror::Error;

#[derive(Debug)]
pub enum Command {
    Version,
    Status,
    Convert(ConvertArgs),
    Inspect { input: PathBuf },
    Plan(PipelineArgs),
    Explain(PipelineArgs),
    Hardware,
}

#[derive(Debug, Default)]
pub struct PipelineArgs {
    pub input: PathBuf,
    pub model_dir: Option<PathBuf>,
    pub native_threshold: Option<u32>,
    pub ocr_dpi: Option<u32>,
    pub ocr_engine: Option<String>,
}

#[derive(Debug)]
pub struct ConvertArgs {
    pub pipeline: PipelineArgs,
    pub output: Option<PathBuf>,
    pub format: OutputFormat,
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ArgsError(String);

pub fn parse() -> Result<Command, ArgsError> {
    parse_from(std::env::args_os().skip(1))
}

fn parse_from(arguments: impl Iterator<Item = OsString>) -> Result<Command, ArgsError> {
    let values: Vec<_> = arguments.collect();
    let Some(command) = values.first().and_then(|value| value.to_str()) else {
        return Err(ArgsError(usage().into()));
    };
    match command {
        "--version" | "-V" if values.len() == 1 => Ok(Command::Version),
        "status" if values.len() == 1 => Ok(Command::Status),
        "hardware" if values.len() == 1 => Ok(Command::Hardware),
        "inspect" => {
            if values.len() != 2 {
                return Err(ArgsError("usage: ferrodoc inspect <input.pdf>".into()));
            }
            Ok(Command::Inspect {
                input: PathBuf::from(&values[1]),
            })
        }
        "convert" => parse_pipeline(&values[1..], true).map(|(pipeline, output, format)| {
            Command::Convert(ConvertArgs {
                pipeline,
                output,
                format,
            })
        }),
        "plan" => {
            parse_pipeline(&values[1..], false).map(|(pipeline, _, _)| Command::Plan(pipeline))
        }
        "explain" => {
            parse_pipeline(&values[1..], false).map(|(pipeline, _, _)| Command::Explain(pipeline))
        }
        _ => Err(ArgsError(usage().into())),
    }
}

fn parse_pipeline(
    values: &[OsString],
    allow_output: bool,
) -> Result<(PipelineArgs, Option<PathBuf>, OutputFormat), ArgsError> {
    let mut input = None;
    let mut model_dir = None;
    let mut native_threshold = None;
    let mut ocr_dpi = None;
    let mut ocr_engine = None;
    let mut output = None;
    let mut format = OutputFormat::Markdown;
    let mut index = 0;
    while index < values.len() {
        let value = values[index]
            .to_str()
            .ok_or_else(|| ArgsError("arguments must be valid UTF-8".into()))?;
        match value {
            "-o" | "--output" if allow_output => {
                output = Some(PathBuf::from(next_value(values, &mut index, value)?));
            }
            "--format" if allow_output => {
                format = match next_string(values, &mut index, value)?.as_str() {
                    "markdown" | "md" => OutputFormat::Markdown,
                    "html" => OutputFormat::Html,
                    "json" | "evidence-json" => OutputFormat::EvidenceJson,
                    other => return Err(ArgsError(format!("unsupported output format {other:?}"))),
                };
            }
            "--ocrs-model-dir" => {
                model_dir = Some(PathBuf::from(next_value(values, &mut index, value)?));
            }
            "--native-threshold" => {
                native_threshold = Some(parse_u32(next_string(values, &mut index, value)?, value)?);
            }
            "--ocr-dpi" => {
                ocr_dpi = Some(parse_u32(next_string(values, &mut index, value)?, value)?);
            }
            "--ocr-engine" => {
                ocr_engine = Some(next_string(values, &mut index, value)?);
            }
            flag if flag.starts_with('-') => {
                return Err(ArgsError(format!("unknown option {flag:?}")));
            }
            _ if input.is_none() => input = Some(PathBuf::from(&values[index])),
            _ => return Err(ArgsError(format!("unexpected argument {value:?}"))),
        }
        index += 1;
    }
    let input = input.ok_or_else(|| ArgsError("missing input PDF".into()))?;
    Ok((
        PipelineArgs {
            input,
            model_dir,
            native_threshold,
            ocr_dpi,
            ocr_engine,
        },
        output,
        format,
    ))
}

fn next_value<'a>(
    values: &'a [OsString],
    index: &mut usize,
    option: &str,
) -> Result<&'a OsString, ArgsError> {
    *index += 1;
    values
        .get(*index)
        .ok_or_else(|| ArgsError(format!("missing value for {option}")))
}

fn next_string(values: &[OsString], index: &mut usize, option: &str) -> Result<String, ArgsError> {
    next_value(values, index, option)?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| ArgsError(format!("value for {option} must be valid UTF-8")))
}

fn parse_u32(value: String, option: &str) -> Result<u32, ArgsError> {
    value
        .parse()
        .map_err(|_| ArgsError(format!("value for {option} must be an unsigned integer")))
}

fn usage() -> &'static str {
    "usage: ferrodoc --version | status | hardware | inspect <input.pdf> | plan <input.pdf> [options] | explain <input.pdf> [options] | convert <input.pdf> [-o output] [--format markdown|html|json] [options]"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_convert_options_in_any_position() {
        let command = parse_from(
            ["convert", "--format", "html", "input.pdf", "-o", "out.html"]
                .into_iter()
                .map(OsString::from),
        )
        .unwrap();
        let Command::Convert(arguments) = command else {
            panic!("wrong command")
        };
        assert_eq!(arguments.format, OutputFormat::Html);
        assert_eq!(arguments.pipeline.input, PathBuf::from("input.pdf"));
        assert_eq!(arguments.output, Some(PathBuf::from("out.html")));
    }
}
