# Threat model and input limits

Ferrodoc assumes PDFs, process-engine messages, model acquisition directories, benchmark artifacts, and administrator-provided command configurations may be malformed. A model/native engine selected by an administrator is trusted to run code but is not trusted to produce correct evidence or honest resource observations.

## Enforced boundaries

- PDF acquisition rejects a file larger than 256 MiB from metadata before reading and uses a bounded reader to defend against size changes. Parsing caps pages at 10,000, indirect objects at 2,000,000, inherited page-tree depth at 128, and one raster at 200,000,000 pixels. Arithmetic is checked before raster allocation.
- Protocol frames are capped at 16 MiB before inbound allocation and during outbound serialization. The fixed preamble, version negotiation, complete CBOR consumption, deadlines, cancellation, child termination, stderr drainage, and blob scope are tested.
- Engine inputs use opaque scoped blobs. Process discovery requires explicit trusted roots or absolute executables. The experimental command engine additionally requires canonical allowlisting, typed arguments, a cleared environment, bounded stdout/stderr, and no shell.
- Model installation accepts only regular files beneath the acquisition root, verifies exact length and SHA-256, requires explicit license acceptance where declared, and atomically publishes a complete content-addressed view. Symlink escape and partial/corrupt installs are rejected.
- Cache and research artifacts use atomic publication and digest verification. Experiment evaluation executes no mutation command and re-hashes protected truth/metric code before and after scoring.
- Secrets are not accepted by persistent schemas. Process environments are cleared or explicitly constructed at trust boundaries; reports and cache identities contain digests and redacted categories rather than document bytes or credentials.

## Residual risks

The pure-Rust PDF dependencies and optional native libraries still process complex hostile data and may contain vulnerabilities. Process isolation is not a platform sandbox: v0.2 does not provide Linux namespaces/seccomp, macOS sandbox profiles, or Windows AppContainer/job restrictions. Embedded Tesseract cannot be interrupted inside a native call. Cache/model-store filesystem permissions follow the invoking user. Hard links created outside Ferrodoc and denial-of-service below configured limits remain operating-environment concerns.

Use process transport for volatile native engines, a dedicated low-privilege account/container for untrusted documents, read-only input mounts, a private cache/model root, explicit CPU/RAM/time limits, and no network unless a policy-enabled remote engine is intentionally used.
