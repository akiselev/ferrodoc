//! Experimental command escape hatch with no shell or request-driven interpolation.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use ferrodoc_core::{
    BackendId, Bytes, CURRENT_SCHEMA_VERSION, Capability, DeterministicProvenance, DeviceId,
    DeviceKind, Estimate, EstimateConfidence, EstimateSource, EvidenceId, LayerId, Millis,
    ResourceEstimate, Sha256Digest, Stage,
};
use ferrodoc_engine_api::{
    DependencyHealth, Engine, EngineCandidate, EngineCompatibility, EngineDescriptor, EngineError,
    EngineErrorCategory, EngineRequest, EngineResponse, ExecutionContext, HardwareInventory,
    HealthReport, HealthRequest, HealthStatus, NetworkUse,
};
use ferrodoc_ir::{Evidence, EvidenceContent};
use serde::{Deserialize, Serialize};

/// Stable implementation version.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_CONFIGURED_OUTPUT: u64 = 16 * Bytes::MIB;
const MAX_CONFIGURED_TIMEOUT_MS: u64 = 300_000;

/// One exact process argument. Only `input_path` inserts host-generated data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Argument {
    /// Exact argument bytes from trusted configuration, with no placeholder processing.
    Literal { value: String },
    /// Temporary immutable input file path inserted as one argument.
    InputPath,
}

/// Trusted host configuration for the experimental engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandConfig {
    /// Stable lowercase engine ID, conventionally under `experimental.command.*`.
    pub engine_id: String,
    /// Explicit absolute executable path.
    pub executable: PathBuf,
    /// Explicit absolute paths permitted by the host.
    pub allowed_executables: Vec<PathBuf>,
    /// Exact argument vector; never parsed by a shell.
    pub arguments: Vec<Argument>,
    /// Maximum captured stdout and stderr bytes each.
    pub max_output_bytes: u64,
    /// Hard child deadline.
    pub timeout_ms: u64,
    /// User-declared conservative host RAM envelope.
    pub max_ram_bytes: u64,
    /// Whether identical input/config is expected to be deterministic.
    pub deterministic: bool,
}

/// Validated experimental command engine.
pub struct CommandEngine {
    descriptor: EngineDescriptor,
    executable: PathBuf,
    arguments: Vec<Argument>,
    max_output_bytes: u64,
    timeout: Duration,
    max_ram: Bytes,
    executable_digest: Sha256Digest,
    configuration_digest: Sha256Digest,
}

impl CommandEngine {
    /// Validates absolute allowlisting and bounded execution settings.
    pub fn new(config: CommandConfig) -> Result<Self, EngineError> {
        if !config.engine_id.starts_with("experimental.command.") {
            return Err(invalid(
                "command engine ID must start with experimental.command.",
            ));
        }
        if !config.executable.is_absolute()
            || config
                .allowed_executables
                .iter()
                .any(|path| !path.is_absolute())
        {
            return Err(invalid("executable and allowlist paths must be absolute"));
        }
        if config.allowed_executables.is_empty()
            || config.max_output_bytes == 0
            || config.max_output_bytes > MAX_CONFIGURED_OUTPUT
            || config.timeout_ms == 0
            || config.timeout_ms > MAX_CONFIGURED_TIMEOUT_MS
            || config.max_ram_bytes == 0
            || config
                .arguments
                .iter()
                .filter(|argument| matches!(argument, Argument::InputPath))
                .count()
                != 1
            || config.arguments.iter().any(
                |argument| matches!(argument, Argument::Literal { value } if value.contains('\0')),
            )
        {
            return Err(invalid(
                "command bounds, one input_path argument, and NUL-free literals are required",
            ));
        }
        let executable = canonical_file(&config.executable)?;
        let allowlist: BTreeSet<_> = config
            .allowed_executables
            .iter()
            .map(|path| canonical_file(path))
            .collect::<Result<_, _>>()?;
        if !allowlist.contains(&executable) {
            return Err(invalid("executable is not in the canonical allowlist"));
        }
        let executable_digest = Sha256Digest::of_file(&executable).map_err(|error| {
            EngineError::new(EngineErrorCategory::Dependency, false, error.to_string())
        })?;
        let configuration_digest = Sha256Digest::of_bytes(
            &serde_json::to_vec(&config)
                .map_err(|error| invalid(format!("serialize command configuration: {error}")))?,
        );
        let descriptor = EngineDescriptor {
            id: config.engine_id,
            version: ENGINE_VERSION.into(),
            capabilities: BTreeSet::from([Capability::OcrPage]),
            compatibility: vec![EngineCompatibility {
                backend: BackendId::new("external-command").expect("static backend"),
                devices: BTreeSet::from([DeviceKind::Cpu]),
            }],
            deterministic: config.deterministic,
            network_use: NetworkUse::Optional,
            max_concurrency: 1,
        };
        descriptor.validate()?;
        Ok(Self {
            descriptor,
            executable,
            arguments: config.arguments,
            max_output_bytes: config.max_output_bytes,
            timeout: Duration::from_millis(config.timeout_ms),
            max_ram: Bytes::new(config.max_ram_bytes),
            executable_digest,
            configuration_digest,
        })
    }

    /// Digest of the exact allowlisted executable bytes.
    pub const fn executable_digest(&self) -> Sha256Digest {
        self.executable_digest
    }
}

impl Engine for CommandEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }

    fn health(&mut self, _request: HealthRequest) -> Result<HealthReport, EngineError> {
        let metadata = fs::metadata(&self.executable).map_err(|error| {
            EngineError::new(
                EngineErrorCategory::Dependency,
                false,
                format!("allowlisted executable disappeared: {error}"),
            )
        })?;
        let ready = metadata.is_file();
        let status = if ready {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unavailable
        };
        let message = format!(
            "experimental no-shell executable {} with digest {}",
            self.executable.display(),
            self.executable_digest
        );
        Ok(HealthReport {
            status,
            dependencies: vec![DependencyHealth {
                id: "allowlisted-executable".into(),
                status,
                message: message.clone(),
            }],
            message,
        })
    }

    fn estimate(
        &mut self,
        request: &EngineRequest,
        _inventory: &HardwareInventory,
    ) -> Result<Vec<EngineCandidate>, EngineError> {
        require_capability(request)?;
        Ok(vec![EngineCandidate {
            engine_id: self.descriptor.id.clone(),
            backend: BackendId::new("external-command").expect("static backend"),
            device: DeviceId::new(DeviceKind::Cpu, None).expect("static device"),
            resources: ResourceEstimate {
                peak_ram: Estimate::Known(self.max_ram),
                warm_ram: Estimate::Known(Bytes::new(0)),
                peak_vram: Estimate::Unknown,
                warm_vram: Estimate::Known(Bytes::new(0)),
                latency: Estimate::Known(Millis::new(self.timeout.as_millis() as u64)),
                remote_cost: Estimate::Unknown,
                quality: Estimate::Unknown,
                source: Estimate::Known(EstimateSource {
                    confidence: EstimateConfidence::Conservative,
                    method: "trusted command configuration conservative envelope".into(),
                }),
            },
        }])
    }

    fn execute(
        &mut self,
        request: EngineRequest,
        context: &ExecutionContext<'_>,
    ) -> Result<EngineResponse, EngineError> {
        require_capability(&request)?;
        context.checkpoint()?;
        let bytes = context.blobs.resolve(&request.input)?;
        let temporary = tempfile::tempdir()
            .map_err(|error| internal(format!("create command sandbox: {error}")))?;
        let input_path = temporary.path().join("input.bin");
        let mut input_file = fs::File::create(&input_path)
            .map_err(|error| internal(format!("create command input: {error}")))?;
        input_file
            .write_all(&bytes)
            .and_then(|_| input_file.sync_all())
            .map_err(|error| internal(format!("write command input: {error}")))?;
        drop(input_file);

        let mut command = Command::new(&self.executable);
        for argument in &self.arguments {
            match argument {
                Argument::Literal { value } => command.arg(value),
                Argument::InputPath => command.arg(&input_path),
            };
        }
        command
            .current_dir(temporary.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C");
        let mut child = command.spawn().map_err(|error| {
            EngineError::new(
                EngineErrorCategory::Dependency,
                true,
                format!("spawn allowlisted executable: {error}"),
            )
        })?;
        let stdout = bounded_reader(
            child.stdout.take().expect("piped stdout"),
            self.max_output_bytes,
        );
        let stderr = bounded_reader(
            child.stderr.take().expect("piped stderr"),
            self.max_output_bytes,
        );
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            if context.cancellation.is_cancelled() {
                terminate(&mut child);
                join_reader(stdout)?;
                join_reader(stderr)?;
                return Err(EngineError::new(
                    EngineErrorCategory::Cancelled,
                    false,
                    "command execution cancelled",
                ));
            }
            if Instant::now() >= deadline
                || context
                    .deadline
                    .is_some_and(|deadline| Instant::now() >= deadline)
            {
                terminate(&mut child);
                join_reader(stdout)?;
                join_reader(stderr)?;
                return Err(EngineError::new(
                    EngineErrorCategory::DeadlineExceeded,
                    true,
                    "command execution exceeded its deadline",
                ));
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|error| internal(format!("wait for command: {error}")))?
            {
                break status;
            }
            thread::sleep(Duration::from_millis(5));
        };
        let stdout = join_reader(stdout)?;
        let stderr = join_reader(stderr)?;
        if stdout.len() > self.max_output_bytes as usize
            || stderr.len() > self.max_output_bytes as usize
        {
            return Err(EngineError::new(
                EngineErrorCategory::ResourceExhausted,
                false,
                "command output exceeded configured bound",
            ));
        }
        if !status.success() {
            return Err(internal(format!(
                "allowlisted command exited with {}; stderr: {}",
                status,
                String::from_utf8_lossy(&stderr).trim()
            )));
        }
        let text = String::from_utf8(stdout)
            .map_err(|_| invalid("command stdout is not valid UTF-8"))?
            .trim()
            .to_string();
        context.checkpoint()?;
        let input_digest = request
            .input
            .expected_digest
            .unwrap_or_else(|| Sha256Digest::of_bytes(&bytes));
        let mut parameters = request.parameters.clone();
        parameters.insert(
            "command_configuration_digest".into(),
            serde_json::json!(self.configuration_digest),
        );
        let provenance = DeterministicProvenance {
            schema_version: CURRENT_SCHEMA_VERSION,
            input_digest,
            engine_id: self.descriptor.id.clone(),
            engine_version: ENGINE_VERSION.into(),
            model_digest: Some(self.executable_digest),
            parameters,
            stage: Stage::Ocr,
        };
        let identity = provenance
            .identity_digest()
            .map_err(|error| internal(error.to_string()))?;
        let layer_id = LayerId::derive(&[identity.as_bytes()]);
        let evidence = if text.is_empty() {
            Vec::new()
        } else {
            vec![Evidence {
                id: EvidenceId::derive(&[identity.as_bytes(), text.as_bytes()]),
                layer_id: layer_id.clone(),
                content: EvidenceContent::Text { text },
                geometry: None,
                confidence: None,
                provenance,
                engine_metadata: BTreeMap::from([(
                    "integration".into(),
                    serde_json::json!("experimental-command-no-shell"),
                )]),
            }]
        };
        Ok(EngineResponse {
            request_id: request.request_id,
            evidence,
            metadata: BTreeMap::from([("layer_id".into(), serde_json::json!(layer_id))]),
        })
    }
}

fn require_capability(request: &EngineRequest) -> Result<(), EngineError> {
    if request.capability == Capability::OcrPage {
        Ok(())
    } else {
        Err(EngineError::new(
            EngineErrorCategory::Unsupported,
            false,
            "experimental command engine supports only ocr.page",
        ))
    }
}

fn canonical_file(path: &Path) -> Result<PathBuf, EngineError> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        EngineError::new(
            EngineErrorCategory::Dependency,
            false,
            format!("canonicalize executable {path:?}: {error}"),
        )
    })?;
    if !canonical.is_file() {
        return Err(invalid(format!("executable {canonical:?} is not a file")));
    }
    Ok(canonical)
}

fn bounded_reader(
    reader: impl Read + Send + 'static,
    maximum: u64,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.take(maximum + 1).read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

fn join_reader(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
) -> Result<Vec<u8>, EngineError> {
    reader
        .join()
        .map_err(|_| internal("command output reader panicked"))?
        .map_err(|error| internal(format!("read command output: {error}")))
}

fn terminate(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::InvalidRequest, false, message)
}

fn internal(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::Internal, false, message)
}
