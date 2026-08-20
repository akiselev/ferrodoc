mod args;
mod commands;
mod configuration;
mod output;

use std::process::ExitCode;

use serde::Serialize;

fn main() -> ExitCode {
    match args::parse()
        .map_err(|error| ("arguments", error.to_string()))
        .and_then(|command| {
            commands::run(command).map_err(|error| (error.category(), error.to_string()))
        }) {
        Ok(()) => ExitCode::SUCCESS,
        Err((category, message)) => {
            #[derive(Serialize)]
            struct ErrorEnvelope<'a> {
                error: ErrorBody<'a>,
            }
            #[derive(Serialize)]
            struct ErrorBody<'a> {
                category: &'a str,
                message: &'a str,
            }
            let envelope = ErrorEnvelope {
                error: ErrorBody {
                    category,
                    message: &message,
                },
            };
            eprintln!(
                "{}",
                serde_json::to_string(&envelope).expect("error envelope is serializable")
            );
            ExitCode::from(2)
        }
    }
}
