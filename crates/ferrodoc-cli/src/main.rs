mod args;
mod commands;
mod configuration;
mod output;

use std::process::ExitCode;

use ferrodoc::{CliErrorEnvelope, EXIT_ERROR};

fn main() -> ExitCode {
    match args::parse()
        .map_err(|error| ("arguments", error.to_string()))
        .and_then(|command| {
            commands::run(command).map_err(|error| (error.category(), error.to_string()))
        }) {
        Ok(()) => ExitCode::SUCCESS,
        Err((category, message)) => {
            let envelope = CliErrorEnvelope::new(category, message);
            eprintln!(
                "{}",
                serde_json::to_string(&envelope).expect("error envelope is serializable")
            );
            ExitCode::from(EXIT_ERROR)
        }
    }
}
