use std::process::ExitCode;

use ferrodoc_layout_rulebased::RuleBasedLayoutEngine;

fn main() -> ExitCode {
    match ferrodoc_plugin_sdk::run_engine(RuleBasedLayoutEngine::new()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("layout engine protocol failure: {error}");
            ExitCode::FAILURE
        }
    }
}
