use std::process::ExitCode;

use ferrodoc_table_rulebased::RuleBasedTableEngine;

fn main() -> ExitCode {
    match ferrodoc_plugin_sdk::run_engine(RuleBasedTableEngine::new()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("table engine protocol failure: {error}");
            ExitCode::FAILURE
        }
    }
}
