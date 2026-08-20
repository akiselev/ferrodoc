use std::process::ExitCode;

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    match arguments.next().as_deref().and_then(|value| value.to_str()) {
        Some("--version" | "-V") if arguments.next().is_none() => {
            println!("ferrodoc {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("status") if arguments.next().is_none() => {
            println!("Ferrodoc Phase 1 foundation; PDF conversion is not implemented yet");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: ferrodoc --version | ferrodoc status");
            ExitCode::from(2)
        }
    }
}
