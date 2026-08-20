//! Bounded child-process implementation of the semantic engine API.

use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use ferrodoc_core::{BlobRange, RequestId, Sha256Digest};
use ferrodoc_engine_api::{
    Engine, EngineCandidate, EngineDescriptor, EngineError, EngineErrorCategory, EngineRequest,
    EngineResponse, ExecutionContext, HardwareInventory, HealthReport, HealthRequest,
};
use ferrodoc_protocol::{
    CURRENT_PROTOCOL_VERSION, ClientHello, EngineMessage, HostMessage, MAX_FRAME_LENGTH,
    RequestEnvelope, ResponseEnvelope, SUPPORTED_VERSIONS, ServerHello, read_frame, read_preamble,
    write_frame, write_preamble,
};

/// Explicit executable and fixed launch environment for one plugin.
#[derive(Debug, Clone)]
pub struct PluginCommand {
    program: PathBuf,
    arguments: Vec<OsString>,
    environment: BTreeMap<OsString, OsString>,
}

impl PluginCommand {
    /// Resolves a caller-supplied absolute executable path.
    pub fn explicit(program: impl AsRef<Path>) -> Result<Self, EngineError> {
        let program = program.as_ref();
        if !program.is_absolute() {
            return Err(protocol_error(
                "plugin executable must be an explicit absolute path",
            ));
        }
        let canonical = program
            .canonicalize()
            .map_err(|error| protocol_error(format!("resolve plugin executable: {error}")))?;
        if !canonical.is_file() {
            return Err(protocol_error("plugin executable is not a regular file"));
        }
        Ok(Self {
            program: canonical,
            arguments: Vec::new(),
            environment: BTreeMap::new(),
        })
    }

    /// Discovers one exact executable name under explicit trusted roots only.
    pub fn discover_trusted(
        trusted_roots: &[PathBuf],
        executable_name: &OsStr,
    ) -> Result<Self, EngineError> {
        for root in trusted_roots {
            let canonical_root = match root.canonicalize() {
                Ok(root) => root,
                Err(_) => continue,
            };
            let candidate = canonical_root.join(executable_name);
            let Ok(canonical_candidate) = candidate.canonicalize() else {
                continue;
            };
            if canonical_candidate.starts_with(&canonical_root) && canonical_candidate.is_file() {
                return Self::explicit(canonical_candidate);
            }
        }
        Err(protocol_error("plugin not found in trusted install roots"))
    }

    /// Adds one fixed argument from trusted host configuration.
    pub fn argument(mut self, argument: impl Into<OsString>) -> Self {
        self.arguments.push(argument.into());
        self
    }

    /// Adds one explicit environment item. The child otherwise receives an empty environment.
    pub fn environment(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.environment.insert(key.into(), value.into());
        self
    }
}

/// Process lifecycle bounds.
#[derive(Debug, Clone)]
pub struct ProcessConfig {
    /// Maximum negotiation duration.
    pub startup_timeout: Duration,
    /// Default duration for requests without a shorter semantic deadline.
    pub request_timeout: Duration,
    /// Maximum graceful-shutdown wait.
    pub shutdown_timeout: Duration,
    /// Maximum retained stderr bytes. The pipe is always drained beyond this limit.
    pub stderr_bytes: usize,
    /// Negotiated maximum frame payload.
    pub maximum_frame_length: u32,
    /// Maximum explicit restarts after the original child terminates.
    pub maximum_restarts: u32,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            startup_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_millis(500),
            stderr_bytes: 64 * 1024,
            maximum_frame_length: MAX_FRAME_LENGTH,
            maximum_restarts: 1,
        }
    }
}

enum ReaderEvent {
    Response(ResponseEnvelope),
    Failure(String),
}

/// One isolated child implementing the same blocking [`Engine`] contract.
pub struct ProcessEngine {
    descriptor: EngineDescriptor,
    child: Child,
    stdin: ChildStdin,
    responses: mpsc::Receiver<ReaderEvent>,
    version: ferrodoc_protocol::ProtocolVersion,
    maximum_frame_length: u32,
    request_counter: u64,
    config: ProcessConfig,
    stderr: Arc<Mutex<Vec<u8>>>,
    alive: bool,
    command: PluginCommand,
    restarts: u32,
}

impl ProcessEngine {
    /// Spawns and negotiates one explicitly selected plugin.
    pub fn spawn(command: &PluginCommand, config: ProcessConfig) -> Result<Self, EngineError> {
        if config.maximum_frame_length == 0 || config.maximum_frame_length > MAX_FRAME_LENGTH {
            return Err(protocol_error("invalid configured maximum frame length"));
        }
        let mut child = Command::new(&command.program)
            .args(&command.arguments)
            .env_clear()
            .envs(&command.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| protocol_error(format!("spawn plugin: {error}")))?;
        let mut stdin = child.stdin.take().expect("piped child stdin");
        let stdout = child.stdout.take().expect("piped child stdout");
        let stderr_pipe = child.stderr.take().expect("piped child stderr");
        let stderr = Arc::new(Mutex::new(Vec::new()));
        drain_stderr(stderr_pipe, Arc::clone(&stderr), config.stderr_bytes);

        let client_hello = ClientHello {
            versions: SUPPORTED_VERSIONS,
            maximum_frame_length: config.maximum_frame_length,
        };
        if let Err(error) = write_preamble(&mut stdin)
            .and_then(|()| write_frame(&mut stdin, &client_hello, config.maximum_frame_length))
        {
            terminate_child(&mut child);
            return Err(protocol_error(error.to_string()));
        }

        let (hello_tx, hello_rx) = mpsc::sync_channel(1);
        let configured_maximum = config.maximum_frame_length;
        thread::spawn(move || {
            let mut stdout = stdout;
            let result = read_preamble(&mut stdout)
                .and_then(|()| read_frame::<ServerHello>(&mut stdout, configured_maximum))
                .map(|hello| (hello, stdout))
                .map_err(|error| error.to_string());
            let _ = hello_tx.send(result);
        });
        let (hello, stdout) = match hello_rx.recv_timeout(config.startup_timeout) {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                terminate_child(&mut child);
                return Err(protocol_error(format!(
                    "plugin negotiation failed: {error}"
                )));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                terminate_child(&mut child);
                return Err(EngineError::new(
                    EngineErrorCategory::DeadlineExceeded,
                    true,
                    "plugin startup timed out",
                ));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                terminate_child(&mut child);
                return Err(protocol_error("plugin negotiation channel disconnected"));
            }
        };
        if hello.version != CURRENT_PROTOCOL_VERSION
            || hello.maximum_frame_length == 0
            || hello.maximum_frame_length > config.maximum_frame_length
        {
            terminate_child(&mut child);
            return Err(protocol_error(format!(
                "invalid server negotiation: version {}, maximum {}",
                hello.version.0, hello.maximum_frame_length
            )));
        }
        if let Err(error) = hello.descriptor.validate() {
            terminate_child(&mut child);
            return Err(error);
        }
        let (response_tx, responses) = mpsc::channel();
        start_response_reader(stdout, response_tx, hello.maximum_frame_length);
        Ok(Self {
            descriptor: hello.descriptor,
            child,
            stdin,
            responses,
            version: hello.version,
            maximum_frame_length: hello.maximum_frame_length,
            request_counter: 0,
            config,
            stderr,
            alive: true,
            command: command.clone(),
            restarts: 0,
        })
    }

    /// Returns the bounded tail of child diagnostics, never protocol stdout.
    pub fn stderr_tail(&self) -> Vec<u8> {
        self.stderr.lock().expect("stderr mutex poisoned").clone()
    }

    /// Restarts a terminated child only while the configured bound permits it.
    /// In-flight semantic requests are never retried automatically.
    pub fn restart(&mut self) -> Result<(), EngineError> {
        if self.alive {
            return Err(protocol_error("cannot restart a running plugin"));
        }
        if self.restarts >= self.config.maximum_restarts {
            return Err(EngineError::new(
                EngineErrorCategory::Unavailable,
                false,
                "plugin restart limit exhausted",
            ));
        }
        let mut replacement = Self::spawn(&self.command, self.config.clone())?;
        replacement.restarts = self.restarts + 1;
        *self = replacement;
        Ok(())
    }

    /// Requests graceful shutdown and kills the process if it does not exit in time.
    pub fn shutdown(&mut self) -> Result<(), EngineError> {
        if !self.alive {
            return Ok(());
        }
        let timeout = self.config.shutdown_timeout;
        let response = self.roundtrip(HostMessage::Shutdown, timeout, None);
        if !matches!(&response, Ok(EngineMessage::Shutdown)) {
            self.terminate();
            return response.and_then(|message| {
                Err(protocol_error(format!(
                    "unexpected shutdown response {message:?}"
                )))
            });
        }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    self.alive = false;
                    return Ok(());
                }
                Ok(None) => thread::sleep(Duration::from_millis(5)),
                Err(error) => {
                    self.terminate();
                    return Err(protocol_error(format!("wait for plugin shutdown: {error}")));
                }
            }
        }
        self.terminate();
        Err(EngineError::new(
            EngineErrorCategory::DeadlineExceeded,
            true,
            "plugin shutdown timed out",
        ))
    }

    fn next_request_id(&mut self) -> RequestId {
        self.request_counter = self.request_counter.wrapping_add(1);
        RequestId::derive(&[
            self.descriptor.id.as_bytes(),
            &self.request_counter.to_be_bytes(),
        ])
    }

    fn roundtrip(
        &mut self,
        message: HostMessage,
        timeout: Duration,
        context: Option<&ExecutionContext<'_>>,
    ) -> Result<EngineMessage, EngineError> {
        if !self.alive {
            return Err(EngineError::new(
                EngineErrorCategory::Unavailable,
                false,
                "plugin process is not running",
            ));
        }
        let request_id = self.next_request_id();
        let envelope = RequestEnvelope {
            version: self.version,
            request_id: request_id.clone(),
            message,
        };
        if let Err(error) = write_frame(&mut self.stdin, &envelope, self.maximum_frame_length) {
            self.terminate();
            return Err(protocol_error(format!("write plugin request: {error}")));
        }
        let configured_deadline = Instant::now() + timeout;
        let deadline = context
            .and_then(|context| context.deadline)
            .map_or(configured_deadline, |semantic| {
                semantic.min(configured_deadline)
            });
        loop {
            if context.is_some_and(|context| context.cancellation.is_cancelled()) {
                self.terminate();
                return Err(EngineError::new(
                    EngineErrorCategory::Cancelled,
                    false,
                    "plugin request cancelled; child terminated",
                ));
            }
            let now = Instant::now();
            if now >= deadline {
                self.terminate();
                return Err(EngineError::new(
                    EngineErrorCategory::DeadlineExceeded,
                    true,
                    "plugin request timed out; child terminated",
                ));
            }
            let wait = (deadline - now).min(Duration::from_millis(10));
            match self.responses.recv_timeout(wait) {
                Ok(ReaderEvent::Response(response)) => {
                    if response.version != self.version || response.request_id != request_id {
                        self.terminate();
                        return Err(protocol_error("mismatched plugin response envelope"));
                    }
                    return match response.message {
                        EngineMessage::Error(error) => Err(error),
                        message => Ok(message),
                    };
                }
                Ok(ReaderEvent::Failure(error)) => {
                    self.terminate();
                    return Err(protocol_error(format!("read plugin response: {error}")));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.terminate();
                    return Err(protocol_error("plugin response stream disconnected"));
                }
            }
        }
    }

    fn terminate(&mut self) {
        if self.alive {
            terminate_child(&mut self.child);
            self.alive = false;
        }
    }
}

impl Engine for ProcessEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }

    fn health(&mut self, request: HealthRequest) -> Result<HealthReport, EngineError> {
        let timeout = self.config.request_timeout;
        match self.roundtrip(HostMessage::Health(request), timeout, None)? {
            EngineMessage::Health(report) => Ok(report),
            message => Err(protocol_error(format!(
                "unexpected health response {message:?}"
            ))),
        }
    }

    fn estimate(
        &mut self,
        request: &EngineRequest,
        inventory: &HardwareInventory,
    ) -> Result<Vec<EngineCandidate>, EngineError> {
        let timeout = self.config.request_timeout;
        match self.roundtrip(
            HostMessage::Estimate {
                request: request.clone(),
                inventory: inventory.clone(),
            },
            timeout,
            None,
        )? {
            EngineMessage::Estimate(candidates) => Ok(candidates),
            message => Err(protocol_error(format!(
                "unexpected estimate response {message:?}"
            ))),
        }
    }

    fn execute(
        &mut self,
        mut request: EngineRequest,
        context: &ExecutionContext<'_>,
    ) -> Result<EngineResponse, EngineError> {
        context.checkpoint()?;
        let bytes = context.blobs.resolve(&request.input)?;
        if bytes.is_empty() {
            return Err(EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                "cannot register an empty process blob",
            ));
        }
        request.input.range = BlobRange::new(0, bytes.len() as u64).map_err(|error| {
            EngineError::new(
                EngineErrorCategory::InvalidRequest,
                false,
                error.to_string(),
            )
        })?;
        request.input.expected_digest = Some(Sha256Digest::of_bytes(&bytes));
        let blob_id = request.input.id.clone();
        let timeout = self.config.request_timeout;
        match self.roundtrip(
            HostMessage::RegisterBlob {
                blob: request.input.clone(),
                bytes,
            },
            timeout,
            Some(context),
        )? {
            EngineMessage::BlobRegistered(id) if id == blob_id => {}
            message => {
                return Err(protocol_error(format!(
                    "unexpected blob registration response {message:?}"
                )));
            }
        }
        let result = match self.roundtrip(HostMessage::Execute(request), timeout, Some(context)) {
            Ok(EngineMessage::Execute(response)) => Ok(response),
            Ok(message) => Err(protocol_error(format!(
                "unexpected execution response {message:?}"
            ))),
            Err(error) => Err(error),
        };
        if self.alive {
            let release = self.roundtrip(HostMessage::ReleaseBlob(blob_id.clone()), timeout, None);
            if !matches!(release, Ok(EngineMessage::BlobReleased(id)) if id == blob_id)
                && result.is_ok()
            {
                return Err(protocol_error("blob release failed after execution"));
            }
        }
        result
    }
}

impl Drop for ProcessEngine {
    fn drop(&mut self) {
        let _ = self.shutdown();
        self.terminate();
    }
}

fn start_response_reader(
    mut stdout: std::process::ChildStdout,
    sender: mpsc::Sender<ReaderEvent>,
    maximum: u32,
) {
    thread::spawn(move || {
        loop {
            match read_frame(&mut stdout, maximum) {
                Ok(response) => {
                    if sender.send(ReaderEvent::Response(response)).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(ReaderEvent::Failure(error.to_string()));
                    return;
                }
            }
        }
    });
}

fn drain_stderr(
    mut stderr_pipe: std::process::ChildStderr,
    retained: Arc<Mutex<Vec<u8>>>,
    limit: usize,
) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        while let Ok(count) = stderr_pipe.read(&mut buffer) {
            if count == 0 {
                return;
            }
            let mut retained = retained.lock().expect("stderr mutex poisoned");
            retained.extend_from_slice(&buffer[..count]);
            if retained.len() > limit {
                let excess = retained.len() - limit;
                retained.drain(..excess);
            }
        }
    });
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn protocol_error(message: impl Into<String>) -> EngineError {
    EngineError::new(EngineErrorCategory::Protocol, false, message)
}
