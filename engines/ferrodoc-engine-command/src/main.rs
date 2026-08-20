use std::{fs, path::Path, process::ExitCode};

use ferrodoc_engine_command::{CommandConfig, CommandEngine};

fn main() -> ExitCode {
    let Some(config_path) = std::env::var_os("FERRODOC_COMMAND_CONFIG") else {
        eprintln!("experimental command engine requires FERRODOC_COMMAND_CONFIG");
        return ExitCode::from(2);
    };
    let engine = fs::read(Path::new(&config_path))
        .map_err(|error| format!("read command engine config: {error}"))
        .and_then(|bytes| {
            serde_json::from_slice::<CommandConfig>(&bytes)
                .map_err(|error| format!("parse command engine config: {error}"))
        })
        .and_then(|config| CommandEngine::new(config).map_err(|error| error.to_string()));
    let engine = match engine {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("experimental command engine initialization failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    match ferrodoc_plugin_sdk::run_engine(engine) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("experimental command engine protocol failure: {error}");
            ExitCode::FAILURE
        }
    }
}
