use std::{ffi::OsString, path::PathBuf};

use ferrodoc_core::{Bytes, MicroUsd, Millis, Profile};
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
    Models(ModelsCommand),
    PluginsDoctor(PluginsDoctorArgs),
}

#[derive(Debug)]
pub enum ModelsCommand {
    List {
        store: PathBuf,
    },
    Verify {
        store: PathBuf,
    },
    Pull {
        store: PathBuf,
        manifest: PathBuf,
        source: PathBuf,
        accept: bool,
    },
    Gc {
        store: PathBuf,
    },
}

#[derive(Debug, Default)]
pub struct PluginsDoctorArgs {
    pub plugins: Vec<PathBuf>,
    pub model_dir: Option<PathBuf>,
    pub inference: bool,
}

#[derive(Debug, Default)]
pub struct PipelineArgs {
    pub input: PathBuf,
    pub model_dir: Option<PathBuf>,
    pub native_threshold: Option<u32>,
    pub ocr_dpi: Option<u32>,
    pub ocr_engine: Option<String>,
    pub profile: Option<Profile>,
    pub max_ram: Option<Bytes>,
    pub max_vram: Option<Bytes>,
    pub max_remote_cost: Option<MicroUsd>,
    pub deadline: Option<Millis>,
    pub allow_unknown_estimates: bool,
    pub cache_dir: Option<PathBuf>,
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
        "models" => parse_models(&values[1..]).map(Command::Models),
        "plugins" if values.get(1).and_then(|value| value.to_str()) == Some("doctor") => {
            parse_plugins_doctor(&values[2..]).map(Command::PluginsDoctor)
        }
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

fn parse_models(values: &[OsString]) -> Result<ModelsCommand, ArgsError> {
    let Some(action) = values.first().and_then(|value| value.to_str()) else {
        return Err(ArgsError(
            "usage: ferrodoc models <list|verify|pull|gc> [options]".into(),
        ));
    };
    let default_store = std::env::var_os("FERRODOC_MODEL_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".ferrodoc/models"));
    let mut store = default_store;
    let mut manifest = None;
    let mut source = None;
    let mut accept = false;
    let mut index = 1;
    while index < values.len() {
        let value = values[index]
            .to_str()
            .ok_or_else(|| ArgsError("arguments must be valid UTF-8".into()))?;
        match value {
            "--store" => store = PathBuf::from(next_value(values, &mut index, value)?),
            "--manifest" if action == "pull" => {
                manifest = Some(PathBuf::from(next_value(values, &mut index, value)?));
            }
            "--source" if action == "pull" => {
                source = Some(PathBuf::from(next_value(values, &mut index, value)?));
            }
            "--accept" if action == "pull" => accept = true,
            flag if flag.starts_with('-') => {
                return Err(ArgsError(format!("unknown option {flag:?}")));
            }
            _ => return Err(ArgsError(format!("unexpected argument {value:?}"))),
        }
        index += 1;
    }
    match action {
        "list" => Ok(ModelsCommand::List { store }),
        "verify" => Ok(ModelsCommand::Verify { store }),
        "gc" => Ok(ModelsCommand::Gc { store }),
        "pull" => Ok(ModelsCommand::Pull {
            store,
            manifest: manifest
                .ok_or_else(|| ArgsError("models pull requires --manifest".into()))?,
            source: source.ok_or_else(|| ArgsError("models pull requires --source".into()))?,
            accept,
        }),
        _ => Err(ArgsError(format!("unknown models action {action:?}"))),
    }
}

fn parse_plugins_doctor(values: &[OsString]) -> Result<PluginsDoctorArgs, ArgsError> {
    let mut result = PluginsDoctorArgs::default();
    let mut index = 0;
    while index < values.len() {
        let value = values[index]
            .to_str()
            .ok_or_else(|| ArgsError("arguments must be valid UTF-8".into()))?;
        match value {
            "--plugin" => result
                .plugins
                .push(PathBuf::from(next_value(values, &mut index, value)?)),
            "--ocrs-model-dir" => {
                result.model_dir = Some(PathBuf::from(next_value(values, &mut index, value)?));
            }
            "--inference" => result.inference = true,
            flag if flag.starts_with('-') => {
                return Err(ArgsError(format!("unknown option {flag:?}")));
            }
            _ => return Err(ArgsError(format!("unexpected argument {value:?}"))),
        }
        index += 1;
    }
    Ok(result)
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
    let mut profile = None;
    let mut max_ram = None;
    let mut max_vram = None;
    let mut max_remote_cost = None;
    let mut deadline = None;
    let mut allow_unknown_estimates = false;
    let mut cache_dir = None;
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
            "--profile" => {
                let selected = next_string(values, &mut index, value)?;
                profile = Some(
                    selected
                        .parse()
                        .map_err(|error: ferrodoc_core::CoreError| ArgsError(error.to_string()))?,
                );
            }
            "--max-ram" => {
                let selected = next_string(values, &mut index, value)?;
                max_ram = Some(
                    selected
                        .parse()
                        .map_err(|error: ferrodoc_core::CoreError| ArgsError(error.to_string()))?,
                );
            }
            "--max-vram" => {
                let selected = next_string(values, &mut index, value)?;
                max_vram = Some(
                    selected
                        .parse()
                        .map_err(|error: ferrodoc_core::CoreError| ArgsError(error.to_string()))?,
                );
            }
            "--max-cost-microusd" => {
                max_remote_cost = Some(MicroUsd::new(parse_u64(
                    next_string(values, &mut index, value)?,
                    value,
                )?));
            }
            "--deadline-ms" => {
                deadline = Some(Millis::new(parse_u64(
                    next_string(values, &mut index, value)?,
                    value,
                )?));
            }
            "--allow-unknown-estimates" => allow_unknown_estimates = true,
            "--cache-dir" => {
                cache_dir = Some(PathBuf::from(next_value(values, &mut index, value)?));
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
            profile,
            max_ram,
            max_vram,
            max_remote_cost,
            deadline,
            allow_unknown_estimates,
            cache_dir,
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

fn parse_u64(value: String, option: &str) -> Result<u64, ArgsError> {
    value
        .parse()
        .map_err(|_| ArgsError(format!("value for {option} must be an unsigned integer")))
}

fn usage() -> &'static str {
    "usage: ferrodoc --version | status | hardware | models <list|verify|pull|gc> | plugins doctor | inspect <input.pdf> | plan <input.pdf> [options] | explain <input.pdf> [options] | convert <input.pdf> [-o output] [--format markdown|html|json] [options]"
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

    #[test]
    fn parses_model_pull_and_plugin_doctor() {
        let Command::Models(ModelsCommand::Pull {
            manifest,
            source,
            accept,
            ..
        }) = parse_from(
            [
                "models",
                "pull",
                "--manifest",
                "model.json",
                "--source",
                "files",
                "--accept",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .unwrap()
        else {
            panic!("wrong command")
        };
        assert_eq!(manifest, PathBuf::from("model.json"));
        assert_eq!(source, PathBuf::from("files"));
        assert!(accept);

        let Command::PluginsDoctor(arguments) = parse_from(
            [
                "plugins",
                "doctor",
                "--plugin",
                "/opt/engine",
                "--inference",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .unwrap() else {
            panic!("wrong command")
        };
        assert_eq!(arguments.plugins, vec![PathBuf::from("/opt/engine")]);
        assert!(arguments.inference);
    }
}
