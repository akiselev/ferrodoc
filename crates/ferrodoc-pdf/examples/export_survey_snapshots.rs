use std::{fs, path::PathBuf};

use ferrodoc_pdf::{PdfDocument, PdfLimits, PdfSurvey};
use schemars::schema_for;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let document = PdfDocument::from_bytes(
        include_bytes!("../../../fixtures/pdf/born-digital.pdf").to_vec(),
        PdfLimits::default(),
    )?;
    write_json(
        root.join("schemas/pdf-survey-v1.json"),
        &schema_for!(PdfSurvey),
    )?;
    write_json(
        root.join("fixtures/pdf-survey-born-digital-v1.json"),
        &document.survey()?,
    )?;
    Ok(())
}

fn write_json(
    path: PathBuf,
    value: &impl serde::Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}
