use std::{collections::BTreeSet, fs, path::Path};

use ferrodoc_core::{DocumentStateId, EvidenceId, Sha256Digest};
use ferrodoc_runtime::{EXTERNAL_EVIDENCE_PIN_SCHEMA, ExternalEvidencePin};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut schema = serde_json::to_vec_pretty(&schemars::schema_for!(ExternalEvidencePin))?;
    schema.push(b'\n');
    fs::write(root.join("schemas/external-evidence-pin-v1.json"), schema)?;

    let fixture = ExternalEvidencePin {
        schema: EXTERNAL_EVIDENCE_PIN_SCHEMA.into(),
        source_pdf_sha256: Sha256Digest::of_bytes(b"purpose-built-fp6-source-pdf"),
        document_state_id: DocumentStateId::derive(&[b"purpose-built-fp6-state"]),
        evidence_ids: BTreeSet::from([
            EvidenceId::derive(&[b"retained-baseline-anchor"]),
            EvidenceId::derive(&[b"targeted-table-evidence"]),
        ]),
    };
    let mut fixture_json = serde_json::to_vec(&fixture)?;
    fixture_json.push(b'\n');
    fs::write(
        root.join("fixtures/external-evidence-pin-v1.json"),
        fixture_json,
    )?;
    Ok(())
}
