#![cfg(unix)]

use std::{collections::BTreeMap, path::PathBuf};

use ferrodoc_core::{
    BlobId, BlobRange, Capability, MediaType, RequestId, ScopedBlob, Sha256Digest,
};
use ferrodoc_engine_api::{
    BlobResolver, CancellationToken, Engine, EngineError, EngineErrorCategory, EngineRequest,
    ExecutionContext, TraceSink,
};
use ferrodoc_engine_command::{Argument, CommandConfig, CommandEngine};

#[test]
fn relative_and_non_allowlisted_executables_are_rejected() {
    let mut relative = config(PathBuf::from("cat"), vec![Argument::InputPath], 4096);
    relative.allowed_executables = vec![PathBuf::from("cat")];
    assert!(CommandEngine::new(relative).is_err());

    let mut denied = config(PathBuf::from("/bin/cat"), vec![Argument::InputPath], 4096);
    denied.allowed_executables = vec![PathBuf::from("/bin/echo")];
    assert!(CommandEngine::new(denied).is_err());
}

#[test]
fn shell_metacharacters_remain_one_literal_argument() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("must-not-exist");
    let literal = format!(";touch {}", marker.display());
    let mut engine = CommandEngine::new(config(
        PathBuf::from("/bin/echo"),
        vec![
            Argument::Literal {
                value: literal.clone(),
            },
            Argument::InputPath,
        ],
        4096,
    ))
    .unwrap();
    let input = b"fixture".to_vec();
    let resolver = Resolver(input.clone());
    let response = engine
        .execute(request(&input), &context(&resolver))
        .unwrap();
    assert!(!marker.exists());
    let text = match &response.evidence[0].content {
        ferrodoc_ir::EvidenceContent::Text { text } => text,
        _ => panic!("expected text"),
    };
    assert!(text.starts_with(&literal));
}

#[test]
fn output_beyond_bound_is_resource_exhaustion() {
    let mut engine = CommandEngine::new(config(
        PathBuf::from("/bin/cat"),
        vec![Argument::InputPath],
        4,
    ))
    .unwrap();
    let input = b"more than four bytes".to_vec();
    let resolver = Resolver(input.clone());
    let error = engine
        .execute(request(&input), &context(&resolver))
        .unwrap_err();
    assert_eq!(error.category, EngineErrorCategory::ResourceExhausted);
}

#[test]
fn configured_deadline_terminates_the_child() {
    let mut command = config(
        PathBuf::from("/usr/bin/tail"),
        vec![
            Argument::Literal { value: "-f".into() },
            Argument::InputPath,
        ],
        4096,
    );
    command.timeout_ms = 50;
    let mut engine = CommandEngine::new(command).unwrap();
    let input = b"fixture".to_vec();
    let resolver = Resolver(input.clone());
    let error = engine
        .execute(request(&input), &context(&resolver))
        .unwrap_err();
    assert_eq!(error.category, EngineErrorCategory::DeadlineExceeded);
}

fn config(executable: PathBuf, arguments: Vec<Argument>, max_output_bytes: u64) -> CommandConfig {
    CommandConfig {
        engine_id: "experimental.command.security".into(),
        allowed_executables: vec![executable.clone()],
        executable,
        arguments,
        max_output_bytes,
        timeout_ms: 5_000,
        max_ram_bytes: 256 * 1024 * 1024,
        deterministic: true,
    }
}

fn request(input: &[u8]) -> EngineRequest {
    EngineRequest {
        request_id: RequestId::derive(&[b"command-security"]),
        capability: Capability::OcrPage,
        input: ScopedBlob {
            id: BlobId::new("command-security").unwrap(),
            range: BlobRange::new(0, input.len() as u64).unwrap(),
            media_type: MediaType::new("text/plain").unwrap(),
            expected_digest: Some(Sha256Digest::of_bytes(input)),
        },
        page_index: Some(0),
        scope: None,
        parameters: BTreeMap::new(),
        deterministic_seed: None,
        deadline: None,
    }
}

fn context(resolver: &Resolver) -> ExecutionContext<'_> {
    ExecutionContext {
        cancellation: CancellationToken::default(),
        deadline: None,
        blobs: resolver,
        trace: &Trace,
    }
}

struct Resolver(Vec<u8>);

impl BlobResolver for Resolver {
    fn resolve(&self, _blob: &ScopedBlob) -> Result<Vec<u8>, EngineError> {
        Ok(self.0.clone())
    }
}

struct Trace;

impl TraceSink for Trace {
    fn event(&self, _code: &str, _fields: &BTreeMap<String, String>) {}
}
