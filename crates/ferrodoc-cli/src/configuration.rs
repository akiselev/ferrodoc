use std::{env, path::PathBuf};

use ferrodoc_runtime::ConversionOptions;
use thiserror::Error;

use crate::args::PipelineArgs;

#[derive(Debug)]
pub struct Configuration {
    pub input: PathBuf,
    pub model_dir: Option<PathBuf>,
    pub options: ConversionOptions,
    pub cache_dir: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum ConfigurationError {
    #[error("environment variable {name} is not valid Unicode")]
    NonUnicode { name: &'static str },
    #[error("invalid {name} value {value:?}: {reason}")]
    Invalid {
        name: &'static str,
        value: String,
        reason: &'static str,
    },
}

impl Configuration {
    pub fn load(arguments: PipelineArgs) -> Result<Self, ConfigurationError> {
        let env_threshold = optional_env("FERRODOC_NATIVE_CHARACTER_THRESHOLD")?
            .map(|value| parse_u32("FERRODOC_NATIVE_CHARACTER_THRESHOLD", value))
            .transpose()?;
        let env_dpi = optional_env("FERRODOC_OCR_DPI")?
            .map(|value| parse_u32("FERRODOC_OCR_DPI", value))
            .transpose()?;
        let env_model_dir = optional_env("FERRODOC_OCRS_MODEL_DIR")?.map(PathBuf::from);
        let env_cache_dir = optional_env("FERRODOC_CACHE_DIR")?.map(PathBuf::from);
        let engine = arguments
            .ocr_engine
            .or(optional_env("FERRODOC_OCR_ENGINE")?)
            .unwrap_or_else(|| "ocrs".into());
        if engine != "ocrs" {
            return Err(ConfigurationError::Invalid {
                name: "OCR engine",
                value: engine,
                reason: "Phase 2 supports only ocrs",
            });
        }
        let mut options = ConversionOptions::default();
        options.native_character_threshold = arguments
            .native_threshold
            .or(env_threshold)
            .unwrap_or(options.native_character_threshold);
        options.ocr_dpi = arguments.ocr_dpi.or(env_dpi).unwrap_or(options.ocr_dpi);
        options.profile = arguments.profile.unwrap_or(options.profile);
        options.max_ram = arguments.max_ram;
        options.max_vram = arguments.max_vram;
        options.max_remote_cost = arguments.max_remote_cost;
        options.deadline = arguments.deadline;
        options.allow_unknown_hard_estimates = arguments.allow_unknown_estimates;
        if !(72..=300).contains(&options.ocr_dpi) {
            return Err(ConfigurationError::Invalid {
                name: "OCR DPI",
                value: options.ocr_dpi.to_string(),
                reason: "expected an integer from 72 through 300",
            });
        }
        Ok(Self {
            input: arguments.input,
            model_dir: arguments.model_dir.or(env_model_dir),
            options,
            cache_dir: arguments.cache_dir.or(env_cache_dir),
        })
    }
}

fn optional_env(name: &'static str) -> Result<Option<String>, ConfigurationError> {
    match env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigurationError::NonUnicode { name }),
    }
}

fn parse_u32(name: &'static str, value: String) -> Result<u32, ConfigurationError> {
    value.parse().map_err(|_| ConfigurationError::Invalid {
        name,
        value,
        reason: "expected an unsigned integer",
    })
}
