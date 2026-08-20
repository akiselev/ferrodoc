use std::{error::Error, fs, path::Path};

use ferrodoc_bench::{BenchmarkReport, ComparisonReport, PredictionSet};

fn main() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schemas = root.join("schemas");
    fs::create_dir_all(&schemas)?;
    write(
        &schemas.join("benchmark-predictions-v1.json"),
        &schemars::schema_for!(PredictionSet),
    )?;
    write(
        &schemas.join("benchmark-report-v1.json"),
        &schemars::schema_for!(BenchmarkReport),
    )?;
    write(
        &schemas.join("benchmark-comparison-v1.json"),
        &schemars::schema_for!(ComparisonReport),
    )?;
    Ok(())
}

fn write(path: &Path, schema: &schemars::schema::RootSchema) -> Result<(), Box<dyn Error>> {
    let mut bytes = serde_json::to_vec_pretty(schema)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}
