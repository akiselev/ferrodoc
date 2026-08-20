use std::process::Command;

#[test]
fn reports_truthful_phase_status() {
    let output = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .arg("status")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("PDF conversion is not implemented yet"));
}

#[test]
fn rejects_unimplemented_commands() {
    let status = Command::new(env!("CARGO_BIN_EXE_ferrodoc"))
        .arg("convert")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(2));
}
