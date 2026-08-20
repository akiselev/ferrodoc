use std::{error::Error, fs, path::Path};

use ferrodoc_foundry::{CorpusManifest, FoundrySpec, TruthDocument};

fn main() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schemas = root.join("schemas");
    fs::create_dir_all(&schemas)?;
    write(
        &schemas.join("foundry-spec-v1.json"),
        &schemars::schema_for!(FoundrySpec),
    )?;
    write(
        &schemas.join("corpus-manifest-v1.json"),
        &schemars::schema_for!(CorpusManifest),
    )?;
    write(
        &schemas.join("corpus-truth-v1.json"),
        &schemars::schema_for!(TruthDocument),
    )?;
    Ok(())
}

fn write(path: &Path, schema: &schemars::schema::RootSchema) -> Result<(), Box<dyn Error>> {
    let mut bytes = serde_json::to_vec_pretty(schema)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}
