# Phase 6 validation evidence

Baseline: `8b9fc923`
Branch: `phase-6/qualified-engines`

## Goal and architecture changes

Phase 6 narrows v0.2 to a qualified, supportable engine portfolio with one shared behavioral contract.

- `ferrodoc-engine-api::conformance` checks descriptor validity, health, sourced conservative estimates, deterministic fixture execution, cancellation, and unsupported-operation mapping for every shipped engine.
- Native PDF extraction, rule-based layout, OCRS, and the deterministic mock remain the required pure-Rust portfolio. Layout and OCRS reject oversized inputs before expensive processing.
- `ferrodoc-engine-tesseract` is an optional direct C-API integration. It discovers platform libraries and language data, reports actionable health diagnostics, hashes the exact trained-data file, and does not introduce a default native link dependency.
- `ferrodoc-engine-command` is an explicitly experimental escape hatch. It requires an absolute allowlisted executable, uses typed arguments without a shell or interpolation, clears the environment, bounds output, applies deadlines/cancellation, and records executable/configuration digests.
- The CLI/runtime select OCRS or optional Tesseract through `Box<dyn Engine>` while keeping model identity in provenance and cache keys. Default feature groups are `cpu-minimal` and `process-engines`; `tesseract` and `nvml` remain opt-in.
- Fixed-corpus qualification invokes the real CLI and retains failed cases, measured cold wall time, exact candidate/configuration/model identity, and explicit unknown resource fields.

Added packages are `engines/ferrodoc-engine-tesseract` and `engines/ferrodoc-engine-command`. Added artifacts include the common conformance harness, fixed-corpus qualification runner, per-engine documentation, optional CI jobs, and feature-specific parity/security tests. No package or persistent schema was removed or renamed.

## Fixed-corpus observations

All runs used real regression corpus `9ca75658e0db622584320467b7cc312cff9f4409c2909d62c0a78df7fcd4f6d5` and identical two-case membership.

| Candidate | Model digest | Complete cases | Aggregate quality | Measured cold wall total | Resource evidence |
|---|---|---:|---:|---:|---|
| OCRS without models | none | 1/2 | 0.1667 | measured per case | RAM/VRAM explicitly unknown |
| OCRS with verified models | `d577b1fc226a101888d16db3471f9cf49a06a65400cc04fa82a9ebb54cb7b860` | 2/2 | 0.3333 | 44,993.53 ms | RAM/VRAM explicitly unknown |
| Tesseract 5.5.3, English | `daa0c97d651c19fba3b25e81317cd697e9908c8208090c94c3905381c23fc047` | 2/2 | 0.3333 | 9,313.88 ms | RAM/VRAM explicitly unknown |

These tiny-corpus observations qualify execution and evidence accounting; they are not broad quality or performance claims. Mock and experimental command engines have conformance/parity evidence but are not document OCR candidates, so their benchmark status is explicitly not applicable.

## Acceptance criteria

| Criterion | Evidence |
|---|---|
| Every shipped engine passes the same conformance suite | Rule-based layout, OCRS with real models, Tesseract with the installed native dependency, deterministic mock, and allowlisted command engines pass `ferrodoc_engine_api::conformance`. Process parity tests cover layout, OCRS, Tesseract, mock, and command transports. |
| Default features are network-free and need no system native library | Default build/smoke pass offline. `cargo tree -p ferrodoc` contains neither Tesseract nor `libloading`; `ldd target/debug/ferrodoc` contains neither Tesseract nor Leptonica. |
| Missing optional Tesseract is isolated and diagnostic | Unit and doctor tests distinguish missing library, unsupported version, missing language data, health, and inference without affecting unrelated commands. |
| No official v0.2 engine shells out to an OCR/model CLI | OCRS uses Rust APIs and Tesseract uses a narrow dynamically loaded C API. The only `std::process::Command` integration is labeled `experimental.command.*`. |
| Command escape hatch cannot invoke an implicit shell or interpolate untrusted arguments | Security tests reject relative/nonallowlisted executables, preserve metacharacters as one literal argument, bound output, and terminate on deadline. |
| Descriptors and docs match observed support | Each engine document records dependencies, models, capabilities, devices, estimates, isolation, licenses, and benchmark status; doctor/conformance exercises the corresponding descriptors. |
| Fixed-corpus results and resource evidence exist | The table above records identical-corpus results. Unknown RAM/VRAM/cost remain tagged unknown rather than zero; cold wall time is measured by the parent process. |

## Validation

The final locked gate passed:

```text
cargo metadata --locked --format-version 1
./scripts/check-workspace.sh
./scripts/check-boundaries.sh
cargo run --locked -p xtask -- doctor
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo check --locked -p ferrodoc-runtime --features nvml
cargo check --locked -p ferrodoc --no-default-features
cargo check --locked -p ferrodoc --features tesseract
cargo test --workspace --locked                         # 138 tests passed
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --locked -p ferrodoc --all-targets --features tesseract -- -D warnings
./scripts/smoke.sh                                      # offline workspace smoke: ok
./scripts/benchmark-smoke.sh                            # offline benchmark smoke: ok
./scripts/engine-qualification.sh "$PWD/target/debug/ferrodoc" /tmp/ferrodoc-phase6-qualification-final
cargo tree -p ferrodoc                                  # optional native graph absent
ldd target/debug/ferrodoc                               # optional native libraries absent
FERRODOC_TEST_OCRS_MODEL_DIR=/tmp/ferrodoc-ocrs.rVvvzW cargo test -p ferrodoc-engine-ocrs --test conformance --locked
FERRODOC_REQUIRE_TESSERACT=1 cargo test -p ferrodoc-engine-tesseract --test conformance --locked
FERRODOC_REQUIRE_TESSERACT=1 cargo test -p ferrodoc-runtime --test process_engines --features tesseract --locked tesseract
FERRODOC_TEST_TESSERACT=1 cargo test -p ferrodoc --features tesseract --locked tesseract
git diff --exit-code
```

No required Phase 6 validation was left unexecuted.

## Real-PDF evidence

The requested Lightbulb corpus was exercised with the six-page, image-only `1977-laser-fusion-concept-using-d-d-t-pellets-with-the-dnp-laser-feedback-osti-7096907.pdf` (SHA-256 `77d10748ff5dbc096c70ccb9a7c65c2bc7c77a00cae1c18be39c5b9913b00166`).

- Tesseract low-VRAM conversion completed in 45 seconds and produced 8,037 bytes across 170 Markdown lines (output SHA-256 `d652c47609e6677c1b04c2b8f5264d5cfc0afe45d6da613948169f5a3583ea90`).
- OCRS processed for 31 minutes without an error and with observed RSS peaking near 629 MiB, below its 768 MiB reservation, but was terminated because latency was no longer useful for this smoke. Atomic output handling correctly left no partial file.

This records both the successful path and the incomplete latency outcome; it is not ground-truthed quality evidence.

## Migration, risk, rollback, and deviations

The default feature graph remains source-compatible and does not gain a native dependency. Tesseract users must opt into `--features tesseract` and provide a supported system library plus language data. Command-engine configurations must use absolute canonical executable paths and an explicit allowlist.

Primary risks are platform variation in dynamic Tesseract libraries, native-runtime isolation, administrator mistakes in command allowlists, and overinterpreting the tiny fixed corpus. Feature isolation, useful health checks, process wrappers, strict command construction, common conformance, and explicit evidence classes constrain those risks. The Tesseract library is pinned for the process lifetime after successful initialization because unloading the installed OpenMP build caused a native teardown crash; API instances are still deleted. Rollback is removal of the two isolated optional engine crates and their feature/CI wiring; the required pure-Rust path remains intact.

There are no deviations from the Phase 6 plan. Remote PR creation and repository settings were not performed. The phase is merged locally after this evidence commit.
