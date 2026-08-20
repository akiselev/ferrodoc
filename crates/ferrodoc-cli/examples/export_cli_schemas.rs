use std::fs;

use ferrodoc::{CliErrorEnvelope, PlanOutput};
use schemars::schema_for;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all("schemas")?;
    fs::write(
        "schemas/cli-error-v1.json",
        serde_json::to_vec_pretty(&schema_for!(CliErrorEnvelope))?,
    )?;
    fs::write(
        "schemas/cli-plan-v1.json",
        serde_json::to_vec_pretty(&schema_for!(PlanOutput))?,
    )?;
    Ok(())
}
