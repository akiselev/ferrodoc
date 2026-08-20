use std::{env, fs, path::Path, process::ExitCode};

use ferrodoc_engine_ocrs::OcrsEngine;

fn main() -> ExitCode {
    let engine = match env::var_os("FERRODOC_OCRS_MODEL_DIR") {
        None => Ok(OcrsEngine::without_models()),
        Some(directory) => load_engine(Path::new(&directory)),
    };
    let engine = match engine {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("OCRS model initialization failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    match ferrodoc_plugin_sdk::run_engine(engine) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("OCRS engine protocol failure: {error}");
            ExitCode::FAILURE
        }
    }
}

fn load_engine(directory: &Path) -> Result<OcrsEngine, String> {
    let detection = fs::read(directory.join("text-detection.rten"))
        .map_err(|error| format!("read text-detection.rten: {error}"))?;
    let recognition = fs::read(directory.join("text-recognition.rten"))
        .map_err(|error| format!("read text-recognition.rten: {error}"))?;
    OcrsEngine::from_model_bytes(detection, recognition).map_err(|error| error.to_string())
}
