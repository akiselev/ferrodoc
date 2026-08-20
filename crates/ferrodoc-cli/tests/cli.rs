use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use ferrodoc_core::{
    Bytes, CURRENT_SCHEMA_VERSION, LicenseMetadata, MediaType, ModelFile, ModelId, ModelManifest,
    RelativePath, Sha256Digest,
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
        "Ferrodoc Phase 4 resource-aware runtime\n"
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
fn model_commands_install_list_verify_and_collect_offline() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("store");
    let source = directory.path().join("source");
    fs::create_dir_all(source.join("weights")).unwrap();
    let model_bytes = b"fixture model bytes";
    fs::write(source.join("weights/model.bin"), model_bytes).unwrap();
    let manifest = ModelManifest::new(
        CURRENT_SCHEMA_VERSION,
        ModelId::derive(&[b"cli-model"]),
        "fixture-revision",
        vec![
            ModelFile::new(
                RelativePath::new("weights/model.bin").unwrap(),
                Sha256Digest::of_bytes(model_bytes),
                Bytes::new(model_bytes.len() as u64),
                MediaType::new("application/octet-stream").unwrap(),
            )
            .unwrap(),
        ],
        LicenseMetadata {
            expression: "MIT".into(),
            source: "fixture".into(),
            notice: None,
        },
        None,
    )
    .unwrap();
    let manifest_path = directory.path().join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let pull = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .args(["models", "pull", "--store"])
        .arg(&store)
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("--source")
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        pull.status.success(),
        "{}",
        String::from_utf8_lossy(&pull.stderr)
    );
    for action in ["list", "verify", "gc"] {
        let output = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
            .args(["models", action, "--store"])
            .arg(&store)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(serde_json::from_slice::<serde_json::Value>(&output.stdout).is_ok());
    }
}

#[test]
fn plugin_doctor_and_plan_expose_categorized_resource_decisions() {
    let doctor = run(&[
        "plugins",
        "doctor",
        "--plugin",
        "/definitely/missing/engine",
    ]);
    assert!(doctor.status.success());
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let stages: BTreeSet<_> = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|check| {
            (
                check["stage"].as_str().unwrap(),
                check["status"].as_str().unwrap(),
            )
        })
        .collect();
    assert!(stages.contains(&("discovery", "failed")));
    assert!(stages.contains(&("model", "failed")));
    assert!(stages.contains(&("health", "failed")));
    assert!(stages.contains(&("inference", "skipped")));

    let plan = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .args(["plan", "--profile", "low-vram", "--max-ram", "1 MiB"])
        .arg(fixture("born-digital.pdf"))
        .output()
        .unwrap();
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let codes: BTreeSet<_> = plan["resource_plans"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|report| report["decisions"].as_array().unwrap())
        .flat_map(|decision| decision["reasons"].as_array().unwrap())
        .filter_map(|reason| reason["code"].as_str())
        .collect();
    assert!(codes.contains("ram_budget_exceeded"));
    assert!(codes.contains("model_unavailable"));
}

#[test]
fn explain_reports_leases_cache_decisions_and_measurement_state() {
    let output = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .arg("explain")
        .arg(fixture("born-digital.pdf"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let explanation: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(explanation["leases"].is_array());
    assert_eq!(explanation["cache_decisions"]["status"], "not_configured");
    assert_eq!(explanation["measurements"]["status"], "unknown");
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
