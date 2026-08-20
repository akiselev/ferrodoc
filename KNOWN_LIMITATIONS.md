# Known limitations

- The qualified portfolio includes native PDF, rule-based layout, pure-Rust OCRS, and optional Tesseract CPU OCR. Specialized table, formula, handwriting, and VLM engines are deferred.
- `models pull` intentionally installs from a local acquisition directory. It does not contact a registry or download model bytes.
- Linux exposes CPU topology and RAM through `/proc`; unsupported platforms report explicit unknowns. NVIDIA inventory is compile-time optional through the `nvml` feature and returns no devices when NVML is unavailable.
- Embedded execution does not currently attribute process-level peak RAM/VRAM, so measurements remain unknown even though conservative reservations are enforced. Process-level observation is platform-dependent future work.
- Cache and model-store roots are local filesystem stores; multi-host locking and distributed eviction are out of scope for v0.2.
- The foundry and integrity-first evaluator include fixed real-corpus engine reports. The corpus is intentionally tiny and qualifies plumbing/evidence, not broad engine quality.
- Device VRAM, model-load time, warm residency, and remote cost are supported evidence fields but remain explicit unknown unless the engine runner supplies observations. Statistical confidence intervals beyond visible repeated-sample variance are not yet modeled.
- The checked learned router is deliberately unqualified because it did not beat deterministic baselines on the one-document routing holdout. Runtime conversion therefore continues to use the deterministic policy; the model format and guard are plumbing, not a quality claim.
- Experiment integrity is enforced by digest verification and by an evaluator that never executes mutation commands. This is not an operating-system sandbox against a separately invoked malicious process.
- Tesseract discovery is runtime/platform dependent and embedded recognition cannot be interrupted inside a native call; process isolation is recommended. Successfully loaded native libraries remain pinned until process exit to avoid unsafe third-party runtime teardown.
- The command engine is an administrator-controlled experimental boundary. It cannot use a shell implicitly, but Ferrodoc cannot attest to the behavior, licensing, network access, or quality of an allowlisted third-party executable.
- The archived source import was truncated. Its complete original tree cannot be reconstructed from the committed fragments.
- Branch protection and required checks are remote repository settings. The checked policy is `.github/required-checks.json`, but the v0.2 tag must not be created until maintainers verify those settings are active and every required check is green.
