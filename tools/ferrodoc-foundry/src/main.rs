use std::{path::PathBuf, process::ExitCode};

use ferrodoc_foundry::{CorpusManifest, FoundrySpec, generate, verify};

fn main() -> ExitCode {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let result = match arguments.as_slice() {
        [command, spec, output] if command == "generate" => {
            let spec_path = PathBuf::from(spec);
            let output = PathBuf::from(output);
            std::fs::read(&spec_path)
                .map_err(|error| format!("read foundry spec {spec_path:?}: {error}"))
                .and_then(|bytes| {
                    serde_json::from_slice::<FoundrySpec>(&bytes)
                        .map_err(|error| format!("parse foundry spec {spec_path:?}: {error}"))
                })
                .and_then(|spec| generate(&spec, &output).map_err(|error| error.to_string()))
                .map(|manifest| {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&manifest).expect("serializable")
                    );
                })
        }
        [command, root, manifest] if command == "verify" => {
            let manifest_path = PathBuf::from(manifest);
            std::fs::read(&manifest_path)
                .map_err(|error| format!("read corpus manifest {manifest_path:?}: {error}"))
                .and_then(|bytes| {
                    serde_json::from_slice::<CorpusManifest>(&bytes)
                        .map_err(|error| format!("parse corpus manifest {manifest_path:?}: {error}"))
                })
                .and_then(|manifest| {
                    verify(&PathBuf::from(root), &manifest).map_err(|error| error.to_string())
                })
        }
        _ => Err(
            "usage: ferrodoc-foundry generate <spec.json> <output-directory> | verify <corpus-root> <manifest.json>"
                .into(),
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
