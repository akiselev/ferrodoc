use std::process::ExitCode;

use ferrodoc_engine_tesseract::TesseractEngine;

fn main() -> ExitCode {
    let language = std::env::var("FERRODOC_TESSERACT_LANGUAGE").unwrap_or_else(|_| "eng".into());
    match ferrodoc_plugin_sdk::run_engine(TesseractEngine::discover(language)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Tesseract engine protocol failure: {error}");
            ExitCode::FAILURE
        }
    }
}
