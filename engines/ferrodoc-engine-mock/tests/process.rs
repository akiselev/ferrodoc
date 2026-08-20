use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::Path,
    sync::{Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use ferrodoc_core::{
    BlobId, BlobRange, Capability, Estimate, MediaType, RequestId, ScopedBlob, Sha256Digest,
};
use ferrodoc_engine_api::{
    BlobResolver, CancellationToken, Engine, EngineError, EngineErrorCategory, EngineRequest,
    ExecutionContext, HardwareInventory, HealthRequest, HealthStatus, TraceSink,
};
use ferrodoc_engine_mock::MockEngine;
use ferrodoc_runtime::{PluginCommand, ProcessConfig, ProcessEngine};

fn command(fault: Option<&str>) -> PluginCommand {
    let command =
        PluginCommand::explicit(Path::new(env!("CARGO_BIN_EXE_ferrodoc-engine-mock"))).unwrap();
    fault.map_or(command.clone(), |fault| {
        command.environment("FERRODOC_MOCK_FAULT", fault)
    })
}

fn config() -> ProcessConfig {
    ProcessConfig {
        startup_timeout: Duration::from_millis(500),
        request_timeout: Duration::from_millis(300),
        shutdown_timeout: Duration::from_millis(200),
        stderr_bytes: 1024,
        ..ProcessConfig::default()
    }
}

fn request(bytes: &[u8]) -> EngineRequest {
    EngineRequest {
        request_id: RequestId::derive(&[b"process-test"]),
        capability: Capability::OcrPage,
        input: ScopedBlob {
            id: BlobId::new("input").unwrap(),
            range: BlobRange::new(0, bytes.len() as u64).unwrap(),
            media_type: MediaType::new("text/plain").unwrap(),
            expected_digest: Some(Sha256Digest::of_bytes(bytes)),
        },
        page_index: Some(0),
        parameters: BTreeMap::new(),
        deterministic_seed: None,
        deadline: None,
    }
}

fn inventory() -> HardwareInventory {
    HardwareInventory {
        logical_cpus: Estimate::Unknown,
        ram_total: Estimate::Unknown,
        ram_available: Estimate::Unknown,
        devices: Vec::new(),
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

#[derive(Default)]
struct RecordingTrace(Mutex<Vec<(String, BTreeMap<String, String>)>>);

impl TraceSink for RecordingTrace {
    fn event(&self, code: &str, fields: &BTreeMap<String, String>) {
        self.0.lock().unwrap().push((code.into(), fields.clone()));
    }
}

fn context<'a>(resolver: &'a Resolver, cancellation: CancellationToken) -> ExecutionContext<'a> {
    ExecutionContext {
        cancellation,
        deadline: None,
        blobs: resolver,
        trace: &Trace,
    }
}

#[test]
fn embedded_and_process_semantics_match() {
    let bytes = b"fixture".to_vec();
    let resolver = Resolver(bytes.clone());
    let request = request(&bytes);
    let trace = RecordingTrace::default();
    let context = ExecutionContext {
        cancellation: CancellationToken::default(),
        deadline: None,
        blobs: &resolver,
        trace: &trace,
    };
    let mut embedded = MockEngine::new();
    let mut process = ProcessEngine::spawn(&command(None), config()).unwrap();

    assert_eq!(embedded.descriptor(), process.descriptor());
    assert_eq!(
        embedded.health(HealthRequest::Dependencies).unwrap(),
        process.health(HealthRequest::Dependencies).unwrap()
    );
    assert_eq!(
        embedded.estimate(&request, &inventory()).unwrap(),
        process.estimate(&request, &inventory()).unwrap()
    );
    assert_eq!(
        embedded.execute(request.clone(), &context).unwrap(),
        process.execute(request, &context).unwrap()
    );
    assert!(trace.0.lock().unwrap().iter().any(|(code, fields)| {
        code == "engine.transport" && fields.get("mode").map(String::as_str) == Some("process")
    }));
    process.shutdown().unwrap();
}

#[test]
fn startup_faults_are_bounded_and_categorized() {
    for fault in ["crash", "garbage", "partial_frame", "oversized_frame"] {
        let started = Instant::now();
        let error = match ProcessEngine::spawn(&command(Some(fault)), config()) {
            Ok(_) => panic!("fault {fault} unexpectedly negotiated"),
            Err(error) => error,
        };
        assert_eq!(
            error.category,
            EngineErrorCategory::Protocol,
            "fault {fault}"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    let started = Instant::now();
    let error = match ProcessEngine::spawn(&command(Some("hang_start")), config()) {
        Ok(_) => panic!("startup hang unexpectedly negotiated"),
        Err(error) => error,
    };
    assert_eq!(error.category, EngineErrorCategory::DeadlineExceeded);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn execution_timeout_kills_hung_child() {
    let bytes = b"fixture".to_vec();
    let resolver = Resolver(bytes.clone());
    let context = context(&resolver, CancellationToken::default());
    let mut request = request(&bytes);
    request
        .parameters
        .insert("fault".into(), serde_json::json!("hang"));
    let mut process = ProcessEngine::spawn(&command(None), config()).unwrap();
    let started = Instant::now();
    let error = process.execute(request, &context).unwrap_err();
    assert_eq!(error.category, EngineErrorCategory::DeadlineExceeded);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        process.health(HealthRequest::Shallow).unwrap_err().category,
        EngineErrorCategory::Unavailable
    );
    process.restart().unwrap();
    assert_eq!(
        process.health(HealthRequest::Shallow).unwrap().status,
        HealthStatus::Healthy
    );
    process.shutdown().unwrap();
    assert_eq!(
        process.restart().unwrap_err().category,
        EngineErrorCategory::Unavailable
    );
}

#[test]
fn cancellation_kills_hung_child() {
    let bytes = b"fixture".to_vec();
    let resolver = Resolver(bytes.clone());
    let cancellation = CancellationToken::default();
    let context = context(&resolver, cancellation.clone());
    let mut request = request(&bytes);
    request
        .parameters
        .insert("fault".into(), serde_json::json!("hang"));
    let mut process = ProcessEngine::spawn(&command(None), config()).unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        cancellation.cancel();
        let _ = ready_tx.send(());
    });
    let error = process.execute(request, &context).unwrap_err();
    ready_rx.recv().unwrap();
    assert_eq!(error.category, EngineErrorCategory::Cancelled);
}

#[test]
fn stderr_flood_is_drained_and_retained_within_bound() {
    let mut process = ProcessEngine::spawn(&command(Some("stderr_flood")), config()).unwrap();
    assert_eq!(
        process.health(HealthRequest::Shallow).unwrap().status,
        HealthStatus::Healthy
    );
    thread::sleep(Duration::from_millis(20));
    assert_eq!(process.stderr_tail().len(), 1024);
}

#[test]
fn discovery_rejects_relative_and_symlink_escape_paths() {
    assert!(PluginCommand::explicit("ferrodoc-engine-mock").is_err());
    let trusted = tempfile::tempdir().unwrap();
    assert!(
        PluginCommand::discover_trusted(&[trusted.path().to_owned()], OsStr::new("../outside"))
            .is_err()
    );

    #[cfg(unix)]
    {
        use std::{
            fs,
            os::unix::fs::{PermissionsExt, symlink},
        };

        let outside = tempfile::tempdir().unwrap();
        let executable = outside.path().join("outside-engine");
        fs::write(&executable, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        symlink(&executable, trusted.path().join("linked-engine")).unwrap();
        assert!(
            PluginCommand::discover_trusted(
                &[trusted.path().to_owned()],
                OsStr::new("linked-engine")
            )
            .is_err()
        );
    }
}
