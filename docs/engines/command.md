# Experimental command escape hatch

- Package: `ferrodoc-engine-command`; IDs must use `experimental.command.*`.
- Status: experimental, disabled unless an explicit `FERRODOC_COMMAND_CONFIG` is supplied to its standalone process wrapper, and never selected as an official v0.2 OCR integration.
- Security boundary: executable and allowlist entries must be absolute and canonicalize to the same file. Arguments are typed as exact literals or one host-generated input path. `std::process::Command` is used directly with no shell, no placeholder interpolation, cleared environment, private temporary directory, null stdin, bounded stdout/stderr, cancellation, and hard deadlines.
- Capabilities: `ocr.page`; arbitrary UTF-8 stdout becomes text evidence. Executable bytes and trusted configuration both enter provenance identity.
- Devices/network: declared CPU placement and optional network use because an administrator-chosen executable is outside Ferrodoc's semantic control.
- Resources: the trusted configuration supplies a conservative RAM/deadline envelope; VRAM and remote cost stay unknown.
- Isolation: process-only use is strongly recommended. This is not a substitute for a direct Rust or narrow FFI official engine.
- Licenses: Ferrodoc wrapper MIT OR Apache-2.0; executable/model licensing is the administrator's responsibility.
- Benchmark status: common conformance, canonical allowlist denial, literal shell-metacharacter, bounded-output, and child-deadline tests pass. It is excluded from document-quality portfolio claims because behavior is entirely configuration-defined.
