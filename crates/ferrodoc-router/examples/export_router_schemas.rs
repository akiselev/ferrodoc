use std::fs;

use ferrodoc_router::{RouterModel, RoutingDataset};
use schemars::schema_for;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all("schemas")?;
    fs::write(
        "schemas/routing-dataset-v1.json",
        serde_json::to_vec_pretty(&schema_for!(RoutingDataset))?,
    )?;
    fs::write(
        "schemas/router-model-v1.json",
        serde_json::to_vec_pretty(&schema_for!(RouterModel))?,
    )?;
    Ok(())
}
