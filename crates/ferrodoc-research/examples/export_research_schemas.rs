use std::fs;

use ferrodoc_research::{ExperimentLedger, ExperimentSpec};
use schemars::schema_for;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all("schemas")?;
    fs::write(
        "schemas/experiment-spec-v1.json",
        serde_json::to_vec_pretty(&schema_for!(ExperimentSpec))?,
    )?;
    fs::write(
        "schemas/experiment-ledger-v1.json",
        serde_json::to_vec_pretty(&schema_for!(ExperimentLedger))?,
    )?;
    Ok(())
}
