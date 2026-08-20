use std::{
    env,
    io::{self, Write},
    process::ExitCode,
    thread,
    time::Duration,
};

use ferrodoc_engine_mock::MockEngine;
use ferrodoc_protocol::{MAX_FRAME_LENGTH, PREAMBLE};

fn main() -> ExitCode {
    match env::var("FERRODOC_MOCK_FAULT").ok().as_deref() {
        Some("crash") => ExitCode::from(42),
        Some("garbage") => {
            print_raw(b"unframed diagnostic output");
            ExitCode::SUCCESS
        }
        Some("partial_frame") => {
            let mut bytes = PREAMBLE.to_vec();
            bytes.extend(8_u32.to_be_bytes());
            bytes.push(0xa1);
            print_raw(&bytes);
            ExitCode::SUCCESS
        }
        Some("oversized_frame") => {
            let mut bytes = PREAMBLE.to_vec();
            bytes.extend((MAX_FRAME_LENGTH + 1).to_be_bytes());
            print_raw(&bytes);
            ExitCode::SUCCESS
        }
        Some("hang_start") => {
            thread::sleep(Duration::from_secs(60));
            ExitCode::SUCCESS
        }
        Some("stderr_flood") => {
            let mut stderr = io::stderr().lock();
            let _ = stderr.write_all(&vec![b'x'; 1024 * 1024]);
            let _ = stderr.flush();
            drop(stderr);
            run_server()
        }
        Some(other) => {
            eprintln!("unknown FERRODOC_MOCK_FAULT value {other:?}");
            ExitCode::from(2)
        }
        None => run_server(),
    }
}

fn run_server() -> ExitCode {
    match ferrodoc_plugin_sdk::run_engine(MockEngine::new()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mock engine protocol failure: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_raw(bytes: &[u8]) {
    let mut stdout = io::stdout().lock();
    let _ = stdout.write_all(bytes);
    let _ = stdout.flush();
}
