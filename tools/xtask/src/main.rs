use std::{
    fs,
    path::Path,
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("doctor") if std::env::args().nth(2).is_none() => match doctor() {
            Ok(()) => {
                println!("ferrodoc doctor: ok");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("ferrodoc doctor: {error}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: cargo run -p xtask -- doctor");
            ExitCode::from(2)
        }
    }
}

fn doctor() -> Result<(), String> {
    for required in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "fixtures/pdf/born-digital.pdf",
        "fixtures/pdf/image-only.pdf",
        "fixtures/pdf/hybrid.pdf",
        "fixtures/golden/born-digital.md",
        "fixtures/protocol/v1/client-hello.bin",
        "schemas/protocol-request-v1.json",
        "schemas/protocol-response-v1.json",
        "models/ocrs-cpu.json",
        "models/README.md",
        "benchmarks/foundry-smoke.json",
        "benchmarks/real-regression/manifest.json",
        "schemas/foundry-spec-v1.json",
        "schemas/corpus-manifest-v1.json",
        "schemas/corpus-truth-v1.json",
        "schemas/benchmark-predictions-v1.json",
        "schemas/benchmark-report-v1.json",
        "schemas/benchmark-comparison-v1.json",
        "scripts/benchmark-smoke.sh",
        "docs/benchmarking.md",
    ] {
        if !Path::new(required).is_file() {
            return Err(format!("required repository file {required:?} is missing"));
        }
    }
    if Path::new(".materialize").exists() {
        return Err("forbidden opaque .materialize payload exists".into());
    }
    let toolchain = fs::read_to_string("rust-toolchain.toml")
        .map_err(|error| format!("read rust-toolchain.toml: {error}"))?;
    if !toolchain.contains("1.95.0") {
        return Err("rust-toolchain.toml does not pin Rust 1.95.0".into());
    }
    let metadata = Command::new("cargo")
        .args(["metadata", "--locked", "--no-deps", "--format-version", "1"])
        .output()
        .map_err(|error| format!("run cargo metadata: {error}"))?;
    if !metadata.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&metadata.stderr).trim()
        ));
    }
    if !String::from_utf8_lossy(&metadata.stdout).contains("\"name\":\"ferrodoc\"") {
        return Err("cargo metadata did not contain the Ferrodoc CLI".into());
    }
    Ok(())
}
