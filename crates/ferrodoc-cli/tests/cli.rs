use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/pdf")
        .join(name)
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn reports_truthful_phase_status() {
    let output = run(&["status"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Ferrodoc Phase 2 offline PDF vertical slice\n"
    );
}

#[test]
fn born_digital_markdown_matches_golden() {
    let output = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .args(["convert", "--format", "markdown"])
        .arg(fixture("born-digital.pdf"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        include_bytes!("../../../fixtures/golden/born-digital.md")
    );
}

#[test]
fn plan_is_specific_to_native_and_scanned_inputs() {
    let native = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .arg("plan")
        .arg(fixture("born-digital.pdf"))
        .output()
        .unwrap();
    assert!(native.status.success());
    let native_json: serde_json::Value = serde_json::from_slice(&native.stdout).unwrap();
    let native_ocr = native_json["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stage| stage["stage"] == "ocr.ocrs")
        .unwrap();
    assert_eq!(native_ocr["decision"], "rejected");
    assert_eq!(native_ocr["execution"], "embedded");

    let scan = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .arg("plan")
        .arg(fixture("image-only.pdf"))
        .output()
        .unwrap();
    assert!(scan.status.success());
    let scan_json: serde_json::Value = serde_json::from_slice(&scan.stdout).unwrap();
    let scan_ocr = scan_json["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stage| stage["stage"] == "ocr.ocrs")
        .unwrap();
    assert_eq!(scan_ocr["decision"], "unavailable");
}

#[test]
fn output_file_is_replaced_with_complete_content() {
    let directory = tempfile::tempdir().unwrap();
    let output_path = directory.path().join("document.html");
    fs::write(&output_path, b"stale").unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .args(["convert", "--format", "html", "--output"])
        .arg(&output_path)
        .arg(fixture("born-digital.pdf"))
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(
        fs::read(output_path).unwrap(),
        include_bytes!("../../../fixtures/golden/born-digital.html")
    );
}

#[test]
fn malformed_and_missing_inputs_have_structured_errors() {
    let malformed = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .arg("inspect")
        .arg(fixture("malformed.pdf"))
        .output()
        .unwrap();
    assert_eq!(malformed.status.code(), Some(2));
    let error: serde_json::Value = serde_json::from_slice(&malformed.stderr).unwrap();
    assert_eq!(error["error"]["category"], "malformed_pdf");

    let missing = run(&["inspect", "/definitely/missing/ferrodoc.pdf"]);
    assert_eq!(missing.status.code(), Some(2));
    let error: serde_json::Value = serde_json::from_slice(&missing.stderr).unwrap();
    assert_eq!(error["error"]["category"], "missing_input");

    let encrypted = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .arg("inspect")
        .arg(fixture("encrypted.pdf"))
        .output()
        .unwrap();
    assert_eq!(encrypted.status.code(), Some(2));
    let error: serde_json::Value = serde_json::from_slice(&encrypted.stderr).unwrap();
    assert_eq!(error["error"]["category"], "encrypted_pdf");

    let unsupported = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .arg("inspect")
        .arg(fixture("unsupported-rotation.pdf"))
        .output()
        .unwrap();
    assert_eq!(unsupported.status.code(), Some(2));
    let error: serde_json::Value = serde_json::from_slice(&unsupported.stderr).unwrap();
    assert_eq!(error["error"]["category"], "unsupported_pdf");
}

#[test]
fn invalid_environment_and_engine_overrides_fail() {
    let invalid_environment = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .arg("plan")
        .arg(fixture("born-digital.pdf"))
        .env("FERRODOC_OCR_DPI", "not-a-number")
        .output()
        .unwrap();
    assert_eq!(invalid_environment.status.code(), Some(2));
    let error: serde_json::Value = serde_json::from_slice(&invalid_environment.stderr).unwrap();
    assert_eq!(error["error"]["category"], "configuration");

    let unsupported_engine = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .args(["plan", "--ocr-engine", "imaginary"])
        .arg(fixture("born-digital.pdf"))
        .output()
        .unwrap();
    assert_eq!(unsupported_engine.status.code(), Some(2));
}

#[test]
fn hardware_reports_measurements_or_explicit_unknowns() {
    let output = run(&["hardware"]);
    assert!(output.status.success());
    let inventory: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for field in [
        "logical_cpus",
        "physical_cpus",
        "ram_total",
        "ram_available",
    ] {
        assert!(matches!(
            inventory[field]["status"].as_str(),
            Some("known" | "unknown")
        ));
    }
    if inventory["ram_total"]["status"] == "known" {
        assert_eq!(inventory["ram_source"]["status"], "known");
    }
}

#[test]
fn model_backed_scan_and_hybrid_when_models_are_provided() {
    let Some(model_dir) = std::env::var_os("FERRODOC_TEST_OCRS_MODEL_DIR") else {
        return;
    };
    let scan = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .args(["convert", "--format", "markdown", "--ocrs-model-dir"])
        .arg(&model_dir)
        .arg(fixture("image-only.pdf"))
        .output()
        .unwrap();
    assert!(
        scan.status.success(),
        "{}",
        String::from_utf8_lossy(&scan.stderr)
    );
    assert_eq!(
        String::from_utf8(scan.stdout).unwrap(),
        "SCANNED FERRODOC PAGE\nOptical text survives the CPU path.\n"
    );

    let hybrid = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .args(["convert", "--format", "json", "--ocrs-model-dir"])
        .arg(&model_dir)
        .arg(fixture("hybrid.pdf"))
        .output()
        .unwrap();
    assert!(
        hybrid.status.success(),
        "{}",
        String::from_utf8_lossy(&hybrid.stderr)
    );
    let ir: serde_json::Value = serde_json::from_slice(&hybrid.stdout).unwrap();
    let kinds: Vec<_> = ir["pages"][0]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|layer| layer["kind"]["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"native_pdf"));
    assert!(kinds.contains(&"ocr"));
}
