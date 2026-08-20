# Ferrodoc Recovery and v0.2 Implementation Plan

Status: proposed  
Plan baseline: `master` at `621c65ba54b22ca15478c55e2d203af8e7256a10`  
Target: a reproducible, evidence-preserving, hardware-aware document extraction system with one trustworthy offline CPU path and a stable engine boundary  
Plan date: 2026-08-19

## 1. Executive summary

Ferrodoc currently has a strong architectural premise but not a coherent, buildable implementation. The root workspace declares 31 members, while the committed tree contains five crate directories, one of which has no Rust target. Most crates, all plugin directories, `xtask`, model manifests, scripts, and architecture documents named by the README are absent. Four opaque files under `.materialize/` appear to contain an unfinished source-import payload. The checked-in CI mutates source with `cargo fmt --all`, invokes a missing smoke script, and cannot reach compilation while workspace members are absent.

The project must therefore be recovered before it is expanded. The critical path is:

1. recover or discard the incomplete source import and make `master` green;
2. reduce the workspace to defensible package boundaries;
3. harden the shared types and define a versioned evidence IR;
4. prove a deterministic born-digital and scanned-document vertical slice;
5. make the same engine implementation work embedded and over a hardened process protocol;
6. add content-addressed models, honest resource accounting, planning, scheduling, and caching;
7. make the foundry and benchmark system resistant to vacuous or misleading wins;
8. qualify a deliberately small engine portfolio;
9. add learned routing and experiment automation only after benchmark integrity exists;
10. release v0.2 only when every documented command is exercised in CI.

This plan is organized as one implementation PR per phase. A later phase must not start until the previous phase is merged, unless this document is amended with an explicit dependency change. The current planning PR does not count as Phase 0.

## 2. Product definition

Ferrodoc is a document extraction compiler. It accepts a document plus policy and hardware constraints, gathers deterministic and learned evidence, chooses an execution plan, preserves provenance for every hypothesis, reconciles competing evidence, and renders stable output formats.

The intended product properties are:

- **Evidence preserving:** native PDF text, OCR output, layout hypotheses, tables, formulas, and refinements remain distinguishable and attributable.
- **Deterministic by default:** the default path is offline, CPU-capable, reproducible, and cacheable.
- **Hardware aware:** engines declare compatible devices and conservative resource estimates; the planner enforces hard RAM, VRAM, privacy, network, and cost limits.
- **Isolation optional:** one engine implementation can run embedded or through a process transport without changing its semantic contract.
- **Benchmark driven:** routing and engine changes are accepted using versioned corpora, complete case accounting, measured resource data, and explicit Pareto comparisons.
- **Rust native at the orchestration boundary:** official engines use direct Rust APIs or narrow FFI. Shelling out to another OCR/model CLI is confined to an explicitly named escape-hatch engine.
- **Honest about maturity:** documentation describes only behavior verified in the repository and CI.

## 3. Non-goals for v0.2

The following are intentionally outside the v0.2 critical path:

- implementing every engine currently named in the root manifest or README;
- claiming state-of-the-art extraction quality before a defensible public evaluation exists;
- training a foundation OCR or vision-language model;
- distributed scheduling across a cluster;
- a stable Rust dynamic-library ABI;
- PDF editing, authoring, redaction, signing, or archival validation;
- automatic execution of downloaded third-party code;
- GPU determinism guarantees across vendors and driver versions;
- an unrestricted plugin API that can read arbitrary host paths;
- preserving unstable v0.1/v0.2 APIs solely for compatibility.

Ferrodoc is pre-release. Breaking package, schema, and CLI changes are acceptable when they reduce ambiguity or remove unsafe behavior.

## 4. Verified baseline

At the plan baseline:

- `Cargo.toml` declares 31 workspace members.
- The committed tree contains `ferrodoc-core`, `ferrodoc-bench`, `ferrodoc-cli`, `ferrodoc-batteries`, and `ferrodoc-foundry`.
- `ferrodoc-foundry` has a manifest but no `src/lib.rs`, `src/main.rs`, `[lib]`, or `[[bin]]` target.
- Twenty-six declared workspace paths are absent.
- `.materialize/` contains four opaque encoded fragments rather than normal source files.
- There is no committed `Cargo.lock` or `rust-toolchain.toml`.
- The workspace claims Rust 1.95, while CI installs Rust 1.97.1.
- CI runs formatting in write mode rather than check mode.
- CI invokes `scripts/smoke.sh`, but `scripts/` is absent.
- The README describes crates, plugins, models, scripts, documents, and commands that are not present.
- The default branch has no required status checks.
- `ferrodoc-core` contains useful initial types but also known correctness and modeling defects.
- `ferrodoc-bench` can award perfect scores to empty work and compare incomplete candidate case sets.
- `ferrodoc-cli` is wired to absent crates and reports unknown resource measurements as zero.

This section is a baseline, not a permanent status report. Each phase PR must update the progress ledger in Section 14 and may update this section when a statement becomes false.

## 5. Architectural decisions

These decisions govern all phases. A phase may change one only by adding an ADR under `docs/adr/` and updating this section.

### AD-1: Recover before expanding

Do not add engines, router models, model downloads, or research automation until the repository has a green integrity and core test baseline. Opaque source bundles are not implementation.

### AD-2: Package boundaries must correspond to real boundaries

Create a crate only when it supplies at least one of:

- an independently versioned or published API;
- isolation from a heavyweight or platform-specific dependency;
- a wire-protocol compatibility boundary;
- meaningful reuse outside the CLI;
- a substantial compile-time or platform boundary.

Planner, scheduler, cache, model coordination, and pipeline orchestration begin as modules of one runtime crate. They may be extracted later when dependency pressure justifies it.

### AD-3: The IR is an evidence graph, not a flattened document

OCR never overwrites native evidence. Engine outputs append provenance-bearing hypotheses. A selection or reconciliation layer records which evidence was chosen and why. Renderers consume a selected view while JSON can expose the full evidence graph.

### AD-4: Engine semantics are independent of transport

An engine implements a normal Rust trait. The runtime can call it directly or expose it through a versioned process protocol. There is no stable Rust dynamic-library ABI in v0.2. Process isolation is preferred for volatile native runtimes and untrusted parsers.

### AD-5: Blocking engine API, asynchronous orchestration

The engine trait is synchronous and runtime-agnostic. Inference and OCR libraries are typically blocking; the runtime executes them on dedicated worker threads or child processes. Tokio and transport dependencies do not enter `ferrodoc-core`, `ferrodoc-ir`, or `ferrodoc-engine-api`.

### AD-6: Unknown is not zero

Unknown RAM, VRAM, latency, quality, energy, or monetary cost is represented explicitly. Unknown values never silently satisfy hard limits and never appear as free or zero-cost measurements.

### AD-7: Content identity is typed and verified

Inputs, models, corpora, reports, and cache entries use typed digests. Model and corpus installation is atomic and content-addressed. Paths inside manifests are validated relative paths.

### AD-8: Default behavior is offline and CPU-capable

A clean default build performs no network access, downloads no model or native binary during build or tests, and does not require a GPU or system OCR package. Optional engines may have stronger requirements, but their features and CI jobs remain isolated.

### AD-9: Benchmarks reject incomplete evidence

Empty suites, empty cases, missing candidate cases, mismatched corpus digests, unmeasured hard constraints, and failed conversions are errors or explicit failures. They are never interpreted as perfect quality or zero resource usage.

### AD-10: Documentation is executable

Every command advertised as working must be exercised by CI or a documentation test. Future designs are labeled planned and kept separate from current behavior.

## 6. Target repository structure

The target v0.2 structure is intentionally smaller than the current manifest:

```text
.
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── PLAN.md
├── AGENTS.md
├── README.md
├── KNOWN_LIMITATIONS.md
├── docs/
│   ├── architecture.md
│   ├── protocol.md
│   ├── ir.md
│   ├── benchmarking.md
│   ├── models.md
│   ├── security.md
│   └── adr/
├── crates/
│   ├── ferrodoc-core/
│   ├── ferrodoc-ir/
│   ├── ferrodoc-engine-api/
│   ├── ferrodoc-protocol/
│   ├── ferrodoc-runtime/
│   ├── ferrodoc-pdf/
│   ├── ferrodoc-render/
│   └── ferrodoc-cli/
├── engines/
│   ├── ferrodoc-engine-mock/
│   ├── ferrodoc-layout-rulebased/
│   ├── ferrodoc-engine-ocrs/
│   └── ferrodoc-engine-tesseract/
├── tools/
│   ├── ferrodoc-foundry/
│   ├── ferrodoc-bench/
│   ├── ferrodoc-research/
│   └── xtask/
├── models/
├── fixtures/
├── benchmarks/
└── scripts/
```

The target is not an instruction to create empty directories or placeholder crates. A path enters the workspace only when it has a real target, tests, and a defined role.

### 6.1 Migration from the current intended layout

| Current intended package | v0.2 destination |
|---|---|
| `ferrodoc-core` | keep; restrict to validated primitives and identifiers |
| `ferrodoc-ir` | keep as a distinct semantic/schema boundary |
| `ferrodoc-plugin-sdk` | replace with `ferrodoc-engine-api` |
| `ferrodoc-protocol` | keep as the wire compatibility boundary |
| `ferrodoc-plugin-host` | module in `ferrodoc-runtime` |
| `ferrodoc-planner` | module in `ferrodoc-runtime` |
| `ferrodoc-scheduler` | module in `ferrodoc-runtime` |
| `ferrodoc-model-store` | module in `ferrodoc-runtime` for v0.2 |
| `ferrodoc-pipeline` | replace with `ferrodoc-runtime` |
| `ferrodoc-router` | module in `ferrodoc-runtime` until Phase 7 proves separation is useful |
| `ferrodoc-pdf` | keep; isolates PDF dependencies and parser hardening |
| `ferrodoc-render` | keep; reusable, deterministic output boundary |
| `ferrodoc-foundry` | move to `tools/` |
| `ferrodoc-bench` | move to `tools/` |
| `ferrodoc-research` | move to `tools/` |
| `ferrodoc-batteries` | defer; replace with explicit CLI/runtime features in v0.2 |
| engine/plugin crates | move under `engines/`; restore only qualified engines |

## 7. Core contracts

These contracts should be implemented before engine proliferation.

### 7.1 Validated geometry

Replace freely constructible `Rect` values with validated geometry:

- finite coordinates only;
- nonnegative dimensions;
- explicit coordinate space and units;
- page index and transform where applicable;
- clipping that calculates all original edges before mutating any coordinate;
- documented zero-area intersection semantics;
- checked expansion and translation operations.

Property tests must cover symmetry of IoU, clipping containment, translation invariance, invalid floats, negative dimensions, extreme margins, and round-tripping through serialization.

### 7.2 Canonical textual representations

Capabilities, devices, region kinds, media types, and profile names have one canonical wire representation. CLI aliases are accepted only at parsing boundaries. Serde, `Display`, schemas, manifests, and protocol messages emit the same canonical spelling.

### 7.3 Checked quantities

`Bytes` and other quantities use checked integer or fixed-point parsing:

- distinguish SI (`KB`, `MB`, `GB`) from IEC (`KiB`, `MiB`, `GiB`);
- reject NaN, infinity, negative values, overflow, and excessive precision;
- never cast through `f64` into `u64` without validation;
- expose checked addition, multiplication, and subtraction where planning requires them.

Introduce typed monetary and duration quantities rather than unlabelled integers where ambiguity would affect planning.

### 7.4 Explicit estimates

Use an explicit representation such as:

```rust
pub enum Estimate<T> {
    Known(T),
    Unknown,
}
```

A `ResourceEstimate` distinguishes peak from warm residency and includes confidence/source metadata. Planner policy decides whether an unknown is inadmissible, requires a probe, or may run under guarded observation.

### 7.5 Typed digests and model files

Use algorithm-specific digest types or a validated enum. A digest constructor validates byte length and encoding. Model manifests record logical relative path, digest, byte size, media type, source revision, license metadata, and optional acceptance requirements.

### 7.6 Scoped blobs

Do not send arbitrary host `PathBuf` values over the engine protocol. The host registers immutable blobs and passes a capability token plus checked byte range. Blob resolution enforces a read-only root, rejects path traversal and symlink escapes, validates ranges, and can verify an expected digest.

### 7.7 Deterministic and observational provenance

Separate cache-relevant provenance from run observations:

- deterministic: input digest, engine ID/version, model digest, normalized parameters, stage, schema version;
- observational: run ID, timestamp, host, device, duration, measured resources, logs.

Wall-clock timestamps and random run IDs do not enter deterministic artifact identity.

### 7.8 Device, backend, and placement separation

Do not mix physical devices, inference backends, placement policies, and remote service locations in one enum. Model them as independent axes so compatibility filtering remains explainable.

## 8. End-state execution model

### 8.1 Pipeline stages

A conversion is represented as explicit stages:

1. acquire and digest input;
2. inspect container and pages;
3. extract native PDF evidence;
4. render analysis images only when required;
5. classify page/region needs;
6. enumerate compatible engine candidates;
7. reject candidates that violate hard policy;
8. choose a plan;
9. execute with resource leases and cancellation;
10. append evidence to the IR;
11. reconcile and select a view;
12. render output;
13. record trace and measurements;
14. commit cache entries atomically.

A trace explains candidate rejection, selection, resource reservations, cache hits, engine calls, reconciliation choices, and failures.

### 8.2 Engine API sketch

The exact types may change, but the semantic split should resemble:

```rust
pub trait Engine: Send {
    fn descriptor(&self) -> &EngineDescriptor;
    fn health(&mut self, request: HealthRequest) -> Result<HealthReport, EngineError>;
    fn estimate(&self, request: &EngineRequest, inventory: &HardwareInventory)
        -> Result<Vec<EngineCandidate>, EngineError>;
    fn execute(&mut self, request: EngineRequest, context: &ExecutionContext)
        -> Result<EngineResponse, EngineError>;
}
```

`ExecutionContext` supplies cancellation, deadline, scoped blob resolution, deterministic seed where relevant, and structured tracing. It does not expose unrestricted host filesystem or network access.

### 8.3 Process protocol

The process transport uses framed CBOR or another explicitly versioned binary encoding. It must include:

- a fixed preamble and maximum frame length;
- protocol version range negotiation;
- unique request IDs;
- descriptor and capability discovery;
- health, estimate, execute, cancel, ping, and shutdown messages;
- structured error categories;
- deadlines and cancellation acknowledgement;
- bounded stdout reserved exclusively for protocol frames;
- stderr for diagnostics;
- malformed-frame, oversized-frame, crash, hang, and restart behavior;
- compatibility fixtures for every supported protocol version.

### 8.4 Planning and scheduling

Planning is a pure or mostly pure transformation from document features, policy, inventory, engine descriptors, model availability, and estimates into an explainable candidate graph and selected plan. Scheduling applies that plan with resource leases.

Hard constraints include:

- offline/private policy;
- allowed engines and services;
- RAM and VRAM budgets;
- device compatibility;
- model presence;
- maximum remote cost;
- deadlines;
- unsupported or unknown critical estimates.

Soft objectives include expected quality, latency, warm-start state, throughput, energy, and cost. The scheduler does not silently overcommit GPU memory.

## 9. Implementation phases

Each phase below is one PR. Do not split a phase across several PRs merely to show progress. Do not combine phases. A phase PR may remain draft while work is incomplete.

## Phase 0 — Repository recovery and truthful baseline

Branch: `phase-0/recover-repository`  
Depends on: planning PR merged  
Goal: make a clean checkout structurally coherent and green before feature work.

### Work

1. **Recover the unfinished source import safely.**
   - Copy `.materialize/` outside the repository working tree.
   - Determine its encoding and concatenate fragments only after recording their names, byte sizes, and digests.
   - Decode into a temporary directory with protections against absolute paths, `..`, device files, and escaping symlinks.
   - Produce `docs/recovery-inventory.md` listing every recovered path and classifying it as `keep`, `salvage-reference`, or `discard`.
   - Compare the recovered tree with PR #1, workspace membership, README claims, and current committed paths.
   - Never copy recovered files into the workspace wholesale. Admit code only after review of package purpose, dependencies, and compilation state.

2. **Remove opaque materialization artifacts.**
   - Delete `.materialize/` from the repository.
   - Add the path to `.gitignore` if any local recovery tooling still uses it.
   - Do not replace it with another archive or encoded source blob.

3. **Make workspace membership exact.**
   - Include only packages that exist, have a Rust target, and compile.
   - Remove missing or placeholder members from `Cargo.toml`.
   - Remove or repair `default-members` so the default command is valid.
   - Add an integrity script that compares `cargo metadata` with declared local package paths and rejects targetless members.

4. **Establish the toolchain and lockfile.**
   - Add `rust-toolchain.toml` with one pinned stable toolchain.
   - Decide whether Rust 1.95 is the real MSRV. Either test it or change the manifest claim.
   - Generate and commit `Cargo.lock` because Ferrodoc is an application workspace.
   - Remove network-at-build features from the default dependency graph.

5. **Repair CI.**
   - Run `cargo metadata --locked --format-version 1` first.
   - Run `cargo fmt --all -- --check`, never formatting in write mode.
   - Run `cargo check --workspace --all-targets --locked`.
   - Run `cargo test --workspace --locked`.
   - Run `cargo clippy --workspace --all-targets --locked -- -D warnings`.
   - Add a real deterministic smoke script before invoking it.
   - End with `git diff --exit-code` to detect generated or formatting changes.
   - Separate optional native/model engine jobs from the core job.

6. **Correct project claims.**
   - Rewrite README status and quick-start sections to describe only commands that work in this PR.
   - Rewrite `KNOWN_LIMITATIONS.md` based on verified behavior.
   - Update `AGENTS.md` paths and validation commands.
   - Mark planned packages and engines as planned, not implemented.
   - Replace the abbreviated Apache notice with the complete Apache-2.0 license text.

7. **Protect the baseline operationally.**
   - After merge, enable branch protection for `master` and require the core CI checks.
   - Record this as a manual repository-setting task in the Phase 0 PR body if it cannot be changed from code.

### Acceptance criteria

- `.materialize/` is absent.
- Every workspace member exists and has at least one valid Cargo target.
- `cargo metadata --locked --format-version 1` succeeds from a clean checkout.
- The pinned default toolchain passes format, check, tests, and Clippy.
- The claimed MSRV is tested or removed.
- CI never modifies the checkout.
- The smoke command exists and passes without network access.
- `git diff --exit-code` passes after all validation.
- No README command points to an absent package, script, model, or document.
- The recovery inventory explains the fate of every decoded source path.
- `master` can be protected by required green checks immediately after merge.

### Explicit non-goals

- restoring all 31 intended packages;
- making the current CLI feature-complete;
- adding model engines;
- preserving broken generated code for appearance of completeness.

## Phase 1 — Foundations, IR, and workspace consolidation

Branch: `phase-1/foundations-and-ir`  
Depends on: Phase 0  
Goal: establish stable semantic contracts and the reduced target workspace.

### Work

1. Implement the target package boundaries for:
   - `ferrodoc-core`;
   - `ferrodoc-ir`;
   - `ferrodoc-engine-api`;
   - `ferrodoc-protocol` schema types without process I/O yet;
   - `ferrodoc-runtime` skeleton;
   - `ferrodoc-pdf` skeleton;
   - `ferrodoc-render` skeleton;
   - `ferrodoc-cli` skeleton.

2. Refactor `ferrodoc-core`:
   - validated geometry and coordinate spaces;
   - canonical capability serialization;
   - checked quantities;
   - typed digests;
   - explicit unknown estimates;
   - separated device/backend/placement types;
   - deterministic provenance;
   - scoped blob identifiers rather than protocol paths;
   - schema version and compatibility helpers.

3. Define the evidence IR:
   - document identity and metadata;
   - pages and coordinate transforms;
   - source layers and render artifacts;
   - regions and reading-order graph;
   - evidence records with content, geometry, confidence, provenance, and engine metadata;
   - selected-view decisions with reason codes;
   - text, table, formula, image, and unknown payloads;
   - stable IDs independent of run timestamps;
   - forward-compatible unknown fields or version migration policy.

4. Define the engine semantic API and error taxonomy:
   - descriptors;
   - capabilities;
   - compatible backend/device declarations;
   - health reports;
   - candidate estimates;
   - execution request/response;
   - cancellation and deadlines;
   - retryability and failure categories.

5. Define JSON Schema snapshots for persistent manifests and IR. Add explicit schema-version fields and golden serialization tests.

6. Add property and fuzz-style unit tests for geometry, quantities, digest parsing, relative paths, ranges, IDs, and serialization round trips.

7. Write `docs/architecture.md`, `docs/ir.md`, and initial ADRs for package consolidation and transport-independent engines.

### Acceptance criteria

- All Phase 1 crates compile without OCR, GPU, HTTP, model, or native inference dependencies.
- `ferrodoc-core`, `ferrodoc-ir`, and `ferrodoc-engine-api` have no Tokio, HTTP, CUDA, ONNX, OCR, or PDF parser dependencies.
- `Rect::expand` and all geometry operations pass property tests.
- Serde, `Display`, `FromStr`, schemas, and manifests use one canonical capability spelling.
- Invalid and overflowing quantities are rejected.
- Unknown resources cannot be mistaken for zero.
- Arbitrary host paths do not appear in engine request schemas.
- Re-serializing a golden IR fixture is deterministic byte-for-byte under the canonical JSON writer.
- Public types and invariants are documented.

## Phase 2 — Minimal offline conversion vertical slice

Branch: `phase-2/minimal-conversion`  
Depends on: Phase 1  
Goal: prove Ferrodoc can convert real born-digital, scanned, and hybrid fixtures end to end on CPU without network access.

### Work

1. Implement PDF acquisition and inspection in `ferrodoc-pdf`:
   - input digest and size limits;
   - page count and dimensions;
   - native text extraction with glyph/span geometry where available;
   - deterministic page rasterization;
   - malformed/encrypted/unsupported PDF errors;
   - parser limits suitable for untrusted inputs.

2. Implement one rule-based layout engine and one CPU OCR engine:
   - `ferrodoc-layout-rulebased` for basic region segmentation;
   - `ferrodoc-engine-ocrs` using direct Rust APIs;
   - embedded execution only in this phase, but through `ferrodoc-engine-api`.

3. Implement minimal runtime stages:
   - native extraction;
   - page quality heuristic;
   - OCR only when native evidence is absent or below a documented threshold;
   - evidence append;
   - deterministic reconciliation;
   - conversion trace.

4. Implement renderers:
   - Markdown as the primary output;
   - full evidence JSON;
   - minimal semantic HTML if it can remain deterministic and tested.

5. Implement honest CLI commands:
   - `ferrodoc convert`;
   - `ferrodoc inspect`;
   - `ferrodoc plan` that produces an actual input-specific plan;
   - `ferrodoc explain` that reports the trace without claiming page-scoped execution unless it performs it;
   - `ferrodoc hardware` with unknown values represented as unknown.

6. Split the CLI into argument, configuration, output, and command modules. Fail on invalid environment overrides and on a model or engine override that cannot be honored.

7. Add deterministic fixtures:
   - born-digital PDF with headings, paragraphs, and reading order;
   - image-only scanned PDF;
   - hybrid PDF containing both native and scanned content;
   - malformed PDF;
   - page with rotation or non-default crop/media boxes.

8. Add golden output and IR tests. Output writes use temporary files plus atomic rename.

### Acceptance criteria

- A clean default build requires no network, GPU, system Tesseract, or model download.
- Born-digital text is retained as native evidence rather than round-tripped through OCR.
- Scanned content is extracted through the CPU OCR engine.
- Hybrid input preserves native and OCR evidence separately.
- Identical inputs and configuration produce identical selected IR and rendered output, excluding observational run metadata.
- The CLI returns nonzero exit codes and structured errors for malformed, encrypted, missing, and unsupported inputs.
- `plan` explains selected and rejected stages for the actual input.
- Fixture outputs pass on Linux CI from a clean checkout.
- README quick-start commands execute in CI.

## Phase 3 — Process engine protocol and host hardening

Branch: `phase-3/process-protocol`  
Depends on: Phase 2  
Goal: run the same engine semantics embedded and in an isolated child process with defined failure behavior.

### Work

1. Finalize and document protocol framing, version negotiation, limits, and messages.
2. Implement the process host in `ferrodoc-runtime`.
3. Implement `ferrodoc-engine-mock` with deterministic responses and fault-injection modes.
4. Add thin process wrappers for mock, rule-based layout, and OCRS engines.
5. Implement scoped blob registration and resolution.
6. Implement request deadlines, cancellation, child shutdown, and bounded restart policy.
7. Reserve stdout for protocol frames and direct diagnostics to stderr.
8. Add a plugin discovery policy based on explicit paths and trusted install locations. Do not execute arbitrary binaries found in the current directory.
9. Add protocol conformance fixtures and compatibility tests.
10. Add security tests for traversal, symlink escape, range overflow, oversized frames, malformed CBOR, unknown messages, duplicate IDs, output flooding, hangs, crashes, and partial frames.
11. Make embedded/process choice explicit in plans and traces.

### Acceptance criteria

- Mock, layout, and OCRS engines produce semantically equivalent responses embedded and over the process transport.
- A crashed or hung child yields a bounded, categorized error and does not hang the CLI.
- Cancellation terminates or abandons work within a documented bound.
- Oversized or malformed frames are rejected before large allocation.
- Child stdout cannot inject unframed diagnostics into protocol traffic.
- Engines cannot access a blob outside the host-approved scope through protocol data.
- Version mismatch errors identify supported ranges.
- All protocol fixtures are checked into the repository and schema-versioned.

## Phase 4 — Model store, hardware inventory, planner, scheduler, and cache

Branch: `phase-4/resource-runtime`  
Depends on: Phase 3  
Goal: make hardware-aware execution real rather than descriptive.

### Work

1. Implement content-addressed model storage:
   - immutable blobs by digest;
   - validated logical views for runtimes needing directory layouts;
   - atomic install and rollback;
   - offline verification;
   - source, revision, license, size, and acceptance metadata;
   - garbage collection based on live manifests and leases;
   - no model path visible before every file verifies.

2. Implement hardware inventory:
   - logical and physical CPU information where available;
   - total and available RAM with source/confidence;
   - optional NVML-based NVIDIA inventory behind a feature;
   - explicit unknowns on unsupported platforms;
   - fixture-driven inventory tests independent of host hardware.

3. Implement candidate planning:
   - engine capability and device compatibility filtering;
   - model availability;
   - hard RAM, VRAM, privacy, offline, cost, and deadline constraints;
   - profile policies (`fast`, `balanced`, `accurate`, `cpu`, `low-vram`, `offline`, `private`, `cheap`);
   - explainable rejection reason codes;
   - no fabricated fallback candidate.

4. Implement scheduling:
   - CPU worker limits;
   - per-device resource leases;
   - model warm-residency accounting;
   - cancellation propagation;
   - backpressure;
   - guarded execution when policy permits an unknown estimate;
   - observation of actual peak resources where feasible.

5. Implement deterministic caching:
   - key from input/model/engine/schema/normalized-parameter digests;
   - atomic entry commit;
   - corruption detection;
   - stage-level cacheability;
   - no observational timestamp in keys;
   - explicit invalidation on engine or schema changes.

6. Make CLI operations real:
   - `models list`, `models verify`, `models pull`, `models gc`;
   - `plugins doctor` executes health checks and reports dependencies;
   - `plan` reports candidate constraints and estimates;
   - `explain` shows leases, cache decisions, and measurements.

### Acceptance criteria

- Corrupt or partially downloaded models never become visible as installed.
- Model manifests cannot escape their logical root.
- Offline verification works without contacting a registry.
- Unknown RAM/VRAM/cost does not satisfy a hard budget by default.
- Low-VRAM execution stays within its declared reservation or rejects the plan before execution.
- The scheduler cannot grant overlapping leases beyond a configured device budget in deterministic tests.
- Cache hits are stable across runs and invalidated by any semantic input to the key.
- `plugins doctor` distinguishes discovery, dependency, model, health, and inference failures.
- All planner decisions have machine-readable reason codes and human-readable explanations.

## Phase 5 — Foundry and benchmark integrity

Branch: `phase-5/trustworthy-benchmarks`  
Depends on: Phase 4  
Goal: establish a benchmark loop that cannot reward missing work or fabricated measurements.

### Work

1. Rebuild `ferrodoc-foundry` as a deterministic tool:
   - versioned generator schema;
   - seeded document generation;
   - explicit fonts/assets with redistribution status;
   - deterministic degradations;
   - truth for text, regions, reading order, tables, formulas, and provenance;
   - corpus manifest with generator version and digest;
   - train/tuning/held-out partitions that cannot overlap by case identity.

2. Add a small versioned real-world regression corpus with redistribution-safe or purpose-built fixtures. Synthetic data is not the sole quality authority.

3. Redesign benchmark report schemas:
   - report and metric version;
   - corpus digest and exact case IDs;
   - engine/model/config/toolchain identifiers;
   - failed and skipped case accounting;
   - measured, estimated, and unknown values distinguished;
   - cold and warm timing;
   - per-category and aggregate quality.

4. Enforce benchmark integrity:
   - reject empty suites and cases;
   - require candidate and baseline to cover identical case sets;
   - reject mismatched corpus or metric versions;
   - count conversion failure as failure;
   - never treat absent resource metrics as zero;
   - make exclusions explicit and visible.

5. Implement appropriate metrics:
   - Unicode-normalized CER and WER;
   - reading-order edge accuracy or sequence metric;
   - one-to-one region assignment before classification/IoU scoring;
   - category-specific geometry thresholds;
   - table structure similarity rather than dimensions alone;
   - normalized LaTeX token or AST comparison;
   - deterministic exact checks for regression fixtures.

6. Implement measurement:
   - wall and CPU time;
   - peak resident memory;
   - optional per-device peak VRAM;
   - model load time and warm residency;
   - remote request cost when provided by the service;
   - repeated samples with summary statistics.

7. Implement multidimensional comparison:
   - quality, throughput, latency, RAM, VRAM, cost, and failure rate;
   - policy-specific Pareto dominance;
   - per-case regressions and improvements;
   - confidence/variance reporting where repeated samples exist.

8. Document benchmark governance and held-out-set rules in `docs/benchmarking.md`.

### Acceptance criteria

- Empty work cannot receive a passing or perfect score.
- A candidate missing any baseline case cannot dominate the baseline.
- One prediction cannot satisfy multiple truth regions unless the metric explicitly models a one-to-many relation.
- Tables and formulas require spatial association and semantic similarity.
- Reports with unknown RAM or VRAM preserve unknown rather than serializing zero.
- Comparison rejects incompatible corpus and metric versions.
- Synthetic and real regression suites both run in CI at an appropriate size.
- A fixed benchmark can be reproduced from its manifest, seed, assets, engine/model digests, and configuration.

## Phase 6 — Qualified engine portfolio and packaging

Branch: `phase-6/qualified-engines`  
Depends on: Phase 5  
Goal: ship a small, supportable set of engines instead of a large unvalidated catalog.

### v0.2 engine set

Required:

- native PDF extraction in `ferrodoc-pdf`;
- rule-based layout;
- OCRS CPU OCR;
- deterministic mock engine.

Optional but targeted:

- Tesseract through its C API, never through a nested CLI;
- explicit command escape-hatch engine, labeled experimental and disabled by default.

Deferred beyond v0.2 unless all phase gates remain green:

- OAR classic and VLM;
- generic ORT document pipelines;
- Burn model pipelines;
- llama.cpp/libmtmd;
- mistral.rs;
- remote Mistral OCR and other hosted providers;
- learned router engine as a separately installable plugin.

### Work

1. Create an engine conformance test harness covering descriptor validity, canonical capabilities, health, conservative estimates, deterministic fixture execution, cancellation, embedded/process parity, and error mapping.
2. Harden OCRS and rule-based engines against malformed inputs and resource limits.
3. Add Tesseract behind a non-default feature with platform-specific discovery and clear diagnostic output.
4. Add the command escape hatch only with explicit executable allowlisting, argument templates without shell interpolation, bounded I/O, deadlines, and a warning that it is not an official native integration.
5. Define feature groups:
   - default: pure Rust CPU vertical slice;
   - `process-engines`;
   - `tesseract`;
   - optional platform hardware probes.
6. Do not restore `ferrodoc-batteries` as a broad aggregator. Provide explicit features or a narrowly scoped `cpu-minimal` composition only if downstream embedding requires it.
7. Add per-engine documentation: dependencies, models, capabilities, device support, resource estimates, isolation recommendation, licenses, and benchmark status.
8. Add separate CI jobs for optional native dependencies so the default job remains portable.

### Acceptance criteria

- Every shipped engine passes the same conformance suite.
- Default features remain network-free and do not require a system native library.
- Optional Tesseract failure reports the missing library/version without breaking unrelated commands.
- No official v0.2 engine shells out to another OCR/model CLI.
- The command escape hatch cannot invoke a shell implicitly or interpolate untrusted arguments.
- Engine descriptors and documentation match observed capability and device support.
- Each engine has benchmark results on the fixed corpus and resource data is measured or explicit unknown.

## Phase 7 — Routing and experiment loop

Branch: `phase-7/routing-and-research`  
Depends on: Phase 6  
Goal: optimize engine selection using trustworthy observations without overfitting or hiding tradeoffs.

### Work

1. Establish non-learned baselines:
   - always-native where valid;
   - native quality threshold then OCR;
   - page/region type rules;
   - profile-specific deterministic policies.

2. Build routing examples from actual benchmark traces and outcomes, not random features labeled by the heuristic being imitated.
3. Define a versioned feature schema with missing-value handling and no leakage from held-out truth.
4. Split data by document identity and corpus partition so related synthetic variants do not cross train/evaluation boundaries.
5. Train and evaluate a small router only when it beats or extends deterministic baselines on held-out data under a declared objective.
6. Keep deterministic fallback for missing, incompatible, or low-confidence models.
7. Implement the experiment ledger:
   - immutable spec and code/model/corpus digests;
   - exact commands and environment facts;
   - per-trial status and raw report path;
   - mutations separated from evaluations;
   - resumable execution;
   - explicit budget limits;
   - retained Pareto frontier rather than one scalar winner.
8. Prevent the research loop from editing its own evaluator, held-out truth, or acceptance thresholds during a run.
9. Add CLI commands for router data inspection, training, evaluation, and plan comparison. Keep synthetic bootstrap commands clearly labeled as plumbing tests if retained.
10. Add `docs/research.md` documenting data lineage, leakage prevention, budgets, and reproducibility.

### Acceptance criteria

- Router training examples are traceable to real conversion and benchmark records.
- Held-out documents and related variants do not appear in training.
- The learned router is compared with deterministic baselines on identical case sets.
- A router model that violates a hard policy can never override the planner.
- Missing or low-confidence router output falls back deterministically.
- Every experiment records exact executable commands, inputs, digests, configuration, and result paths.
- The experiment runner cannot mutate benchmark truth or metric code in its evaluation workspace.
- Pareto results retain quality/resource tradeoffs rather than selecting solely on throughput or one aggregate score.

## Phase 8 — Release hardening and v0.2

Branch: `phase-8/release-v0.2`  
Depends on: Phase 7  
Goal: make the verified subset installable, documented, and maintainable as v0.2.

### Work

1. Stabilize the v0.2 CLI, configuration precedence, exit codes, JSON output schemas, and deprecation policy.
2. Audit public APIs and package publication metadata.
3. Add cross-platform CI for the pure-Rust core and CLI on Linux, macOS, and Windows; keep platform-specific engine jobs isolated.
4. Add MSRV CI, documentation tests, schema compatibility tests, and package/install tests.
5. Add dependency, license, and source policy using `cargo-deny` or an equivalent checked-in configuration.
6. Add scheduled advisory, fuzz, and sanitizer jobs where practical without making network-dependent scans the only PR gate.
7. Harden PDF and protocol input limits and document the threat model.
8. Ensure model licenses and redistribution constraints are visible before installation.
9. Validate release archives and installation from a clean environment.
10. Rewrite README around the verified v0.2 path. Move future engines and research ideas to a roadmap section.
11. Tag only after every release criterion below is satisfied.

### Acceptance criteria

- `cargo install --path crates/ferrodoc-cli --locked` succeeds from a clean checkout on supported platforms.
- Default conversion works without network access, GPU, or a system OCR library.
- All README commands are executed in CI.
- Persistent schemas have versioning and compatibility tests.
- The process protocol has a documented compatibility policy.
- Package metadata, dual licensing, third-party notices, and model-license handling are complete.
- Release artifacts contain no opaque source payload, local path, generated secret, benchmark held-out truth leak, or untracked model binary.
- Branch protection requires the release-critical checks.
- `KNOWN_LIMITATIONS.md` lists actual unsupported cases and optional dependency constraints.
- The v0.2 tag points to a green commit and the release notes identify exactly which engines are qualified.

## 10. CI design

The final CI topology should separate fast contract checks from expensive optional integrations.

### Required PR checks

1. `integrity`
   - workspace/path validation;
   - `cargo metadata --locked`;
   - forbidden opaque payload check;
   - broken internal documentation links;
   - generated-file cleanliness.

2. `format`
   - `cargo fmt --all -- --check`.

3. `core`
   - `cargo check --workspace --all-targets --locked` for the default feature graph;
   - `cargo test --workspace --locked`;
   - `cargo clippy --workspace --all-targets --locked -- -D warnings`.

4. `vertical-slice`
   - deterministic born-digital fixture;
   - scanned fixture through OCRS;
   - hybrid evidence-preservation fixture;
   - malformed-input cases;
   - golden Markdown and IR.

5. `process-protocol`
   - mock embedded/process parity;
   - fault injection;
   - protocol fixtures;
   - blob-scope security tests.

6. `docs`
   - doctests;
   - README command smoke tests;
   - schema snapshots;
   - internal link checks.

### Conditional or separate checks

- Tesseract on supported runners;
- NVML inventory tests with mocked fixtures and optional hardware smoke;
- cross-platform core/CLI matrix;
- MSRV;
- benchmark regression subset;
- release package/install tests.

### Scheduled checks

- full benchmark suite;
- fuzzing and sanitizers;
- dependency/advisory refresh;
- model manifest link and digest verification;
- extended platform/native-engine matrix.

CI must never download large model assets in the default PR job. Model-backed tests use tiny checked-in fixtures, deterministic mocks, or separately cached artifacts with verified digests.

## 11. Testing strategy

### 11.1 Unit and property tests

Use unit tests for parsing and decisions, and property tests for geometry, quantities, range arithmetic, manifest paths, cache keys, protocol frames, and serialization. Every fixed correctness bug receives a regression test.

### 11.2 Golden fixtures

Golden files are appropriate for stable IR and render output. They include schema versions and are updated only by an explicit command. CI fails if a test rewrites them.

### 11.3 Integration fixtures

Keep fixtures small and purpose-built. Each fixture documents the behavior it isolates. Avoid relying on large external PDFs for required CI.

### 11.4 Differential tests

Where two implementations exist, compare:

- embedded and process engine paths;
- native text and renderer coordinate interpretations;
- cache miss and cache hit results;
- baseline and optimized planner decisions;
- parser behavior across supported PDF backends only if a second backend is intentionally maintained.

### 11.5 Fault injection

The mock engine and process host must simulate slow responses, cancellation refusal, crashes, malformed frames, large outputs, inconsistent estimates, corrupt blobs, partial model installs, and resource exhaustion.

### 11.6 Fuzz targets

Prioritize:

- protocol frame parser;
- manifest and IR deserialization;
- PDF metadata/container inspection boundary;
- relative path and blob range validation;
- quantity and capability parsing;
- renderer handling of adversarial IR graphs.

## 12. Security and trust boundaries

Ferrodoc processes hostile documents and may launch native engines. The security model must be explicit.

- PDF parsing and rasterization are untrusted-input boundaries.
- Child engine processes are not trusted with arbitrary host filesystem access.
- Model manifests are data, not executable install scripts.
- Process discovery uses explicit trusted roots.
- The command escape hatch is disabled by default and never invokes a shell implicitly.
- Remote engines require explicit policy enabling network access and declare data handling/cost behavior.
- Secrets are passed through a credential interface or environment allowlist and never serialized into plans, traces, reports, or cache keys.
- Logs and errors redact credentials and sensitive document payloads by default.
- Cache permissions and cleanup behavior are documented.
- Resource and frame limits are enforced before allocation where possible.
- Symlinks, hard links, archive extraction, and relative paths receive dedicated validation.

Sandboxing beyond process isolation is platform-dependent and may remain a documented limitation in v0.2. The protocol and blob design must not preclude later Linux namespaces/seccomp, macOS sandbox profiles, or Windows job/AppContainer integration.

## 13. Performance and quality policy

Do not establish broad quality claims from the synthetic foundry alone. v0.2 performance work follows these rules:

- correctness and benchmark integrity precede routing optimization;
- native evidence is preferred when reliable because it is cheaper and often more faithful than OCR;
- page rasterization is demand-driven and cached;
- OCR is page- or region-scoped where supported;
- model loading and warm residency are measured separately from inference;
- low-VRAM is a hard-budget profile, not an aspirational label;
- throughput gains that increase failure rate or omit cases are regressions;
- any optimization must preserve a reproducible baseline report;
- nondeterministic engines report variance and seed/control information where available;
- performance numbers identify hardware, software versions, corpus digest, and cold/warm state.

## 14. Progress ledger

Every phase PR updates this table. Use only: `planned`, `in progress`, `blocked`, `complete`, or `superseded`.

| Phase | Status | PR | Baseline SHA | Completion evidence |
|---|---|---|---|---|
| Plan | complete | #2 | `621c65ba54b22ca15478c55e2d203af8e7256a10` | merged as `1be7412de6b0b42d72401377151097f718cd1d36` |
| 0 — repository recovery | complete | local merge | `1be7412de6b0b42d72401377151097f718cd1d36` | all local gates passed; see `docs/phase-0-pr.md`; remote branch protection remains a release operation |
| 1 — foundations and IR | planned | — | — | — |
| 2 — minimal conversion | planned | — | — | — |
| 3 — process protocol | planned | — | — | — |
| 4 — resource runtime | planned | — | — | — |
| 5 — trustworthy benchmarks | planned | — | — | — |
| 6 — qualified engines | planned | — | — | — |
| 7 — routing and research | planned | — | — | — |
| 8 — release v0.2 | planned | — | — | — |

A phase is `complete` only after its PR is merged and all acceptance criteria have evidence. Opening a PR does not make a phase complete.

## 15. Agent execution protocol

This section is the operational contract for autonomous agents implementing the plan.

### 15.1 Starting a phase

1. Read `PLAN.md`, `AGENTS.md`, relevant ADRs, and the immediately preceding phase PR.
2. Fetch the latest `master` and record its SHA in the progress ledger.
3. Verify the previous phase is marked complete and merged.
4. Create exactly the branch named in the phase unless it already exists for the same work.
5. Change the phase status to `in progress` in the first commit or PR update.
6. Reproduce the baseline validation before modifying code. Record pre-existing failures rather than silently inheriting them.

### 15.2 Scope discipline

- Implement the entire phase in one PR and do not begin the next phase in the same branch.
- Do not add placeholder crates or APIs for later phases.
- Do not restore an engine merely because its name existed in the old manifest.
- Do not commit generated archives, model binaries, build output, secrets, local absolute paths, or benchmark scratch data.
- Do not weaken tests, suppress warnings broadly, lower thresholds, or mark cases ignored to obtain green CI.
- Do not fabricate measurements. Use explicit unknown values.
- Do not rewrite truth data and metric code in the same optimization experiment without clearly separating and re-baselining the evaluator change.
- Do not make unrelated formatting or refactors outside phase scope.

### 15.3 Required PR body

Each phase PR body contains:

- phase goal and baseline SHA;
- summary of architecture changes;
- files/packages added, removed, or renamed;
- explicit deviations from `PLAN.md` with ADR links;
- migration notes;
- risk and rollback notes;
- an acceptance-criteria table with one row per criterion;
- every validation command executed and its result;
- unexecuted validation with the concrete reason;
- benchmark/corpus/model digests where relevant;
- follow-up work limited to later phases or genuine defects.

### 15.4 Validation commands

The exact commands may expand by phase, but the minimum after Phase 0 is:

```bash
cargo metadata --locked --format-version 1 >/dev/null
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
./scripts/smoke.sh
git diff --exit-code
```

An agent must show the failing command and relevant output when a command cannot pass. “Not run” is never equivalent to passing.

### 15.5 Commit discipline

- Keep commits reviewable and ordered by dependency: contracts, implementation, tests, documentation.
- Stage only files belonging to the phase.
- Do not mix recovered source with semantic refactoring in one commit when the distinction matters for review.
- Use commit messages that state the behavior or invariant introduced.
- Before push, inspect staged and unstaged diffs and confirm the worktree contains no unrelated changes.

### 15.6 Completing or blocking a phase

A phase can be marked `complete` only when all acceptance criteria pass or the plan is amended to remove/change a criterion with rationale. If blocked:

- mark the phase `blocked`;
- keep the PR draft;
- state the exact blocker, reproduction command, and smallest decision needed;
- do not start the next phase;
- do not conceal the blocker behind a stub or feature flag that claims completion.

## 16. Cross-phase definition of done

Every implementation phase must satisfy all applicable items:

- workspace metadata resolves from a clean checkout with the lockfile;
- formatting, check, tests, and Clippy pass without modifying files;
- public behavior has tests;
- fixed bugs have regression tests;
- persistent formats are versioned;
- errors preserve useful context without leaking secrets or document contents;
- default tests require no network and no large model download;
- new optional dependencies are isolated behind features and CI jobs;
- documentation matches actual commands and behavior;
- no resource value is fabricated;
- deterministic outputs exclude observational timestamps and random IDs;
- atomic writes are used for persistent artifacts;
- new trust boundaries have limits and adversarial tests;
- the phase updates the progress ledger and relevant architecture documents;
- the PR contains reproducible validation evidence;
- the worktree is clean after validation.

## 17. Release criteria for v0.2

Ferrodoc v0.2 is ready only when all of the following are true:

1. A clean checkout resolves and builds with the pinned toolchain and committed lockfile.
2. The default build is offline, CPU-capable, and free of model/native-binary download side effects.
3. Born-digital, scanned, and hybrid fixtures convert through the documented CLI.
4. Native and OCR evidence remain independently inspectable with provenance.
5. The same qualified engine semantics work embedded and over the process protocol.
6. Protocol crash, timeout, cancellation, malformed-frame, and blob-scope behavior is tested.
7. Planner hard limits reject incompatible or unknown candidates instead of fabricating fallbacks.
8. Model installation is content-addressed, verified, atomic, and license-aware.
9. Benchmarking rejects empty, incomplete, incompatible, or unmeasured comparisons.
10. Required engines pass a common conformance suite.
11. Learned routing, when enabled, beats or extends deterministic baselines on held-out data and cannot override hard policy.
12. Every README command is run in CI.
13. Core and CLI checks pass on the supported operating systems.
14. The claimed MSRV is tested.
15. Licenses, third-party notices, model metadata, and security limitations are complete.
16. `master` is protected by required checks.
17. No opaque source fragments, placeholder members, broken documentation links, or fabricated resource metrics remain.

## 18. Risks and mitigations

| Risk | Consequence | Mitigation |
|---|---|---|
| `.materialize` payload is incomplete or corrupt | presumed implementation cannot be recovered | inventory it once, salvage reviewed code only, prune workspace, rebuild from contracts |
| recovered code is broad but untested | large compile/debug surface obscures product path | admit only code needed by the current phase; keep target workspace small |
| upstream OCR/inference crates churn | repeated API breakage and build failures | isolate each runtime in an engine crate and separate CI job; pin exact versions where required |
| build-time binary/model downloads | unreproducible and unsafe default builds | prohibit them in default features; install verified artifacts at runtime |
| PDF parser vulnerabilities | hostile input can crash or compromise host | limits, fuzzing, process isolation roadmap, minimal parser surface, regular dependency review |
| benchmark overfitting | apparent quality gains fail on real documents | fixed held-out partition, real regression corpus, evaluator immutability during experiments |
| resource estimates are optimistic | OOM, driver reset, scheduler instability | conservative estimates, explicit unknowns, guarded probes, measured feedback, hard leases |
| native libraries differ by platform | optional engines break installation | pure-Rust default, isolated features, platform diagnostics, separate CI |
| model licensing is unclear | redistribution or compliance risk | mandatory license/source metadata and acceptance gates before installation |
| protocol becomes accidental public ABI too early | compatibility burden blocks iteration | explicit versioning, pre-release policy, golden fixtures, documented support window |
| router learns synthetic heuristic | complexity without quality gain | train only from real benchmark outcomes and compare with deterministic baselines |
| agents optimize for green CI rather than truth | hidden stubs, ignored tests, fake metrics | acceptance evidence, no fabricated values, complete case accounting, phase review contract |

## 19. Post-v0.2 roadmap

These items are candidates only after v0.2 release criteria are met:

- OAR classic and document-VLM engines;
- generic ONNX Runtime engine packs;
- Burn-native models;
- llama.cpp/libmtmd document VLMs;
- mistral.rs document VLMs;
- hosted OCR/VLM providers with privacy and cost policies;
- table-specialist and formula-specialist engines;
- handwriting and chart extraction;
- richer reconciliation and confidence calibration;
- region-level adaptive routing;
- reproducible tiny-model training;
- sandbox backends for Linux, macOS, and Windows;
- remote/distributed scheduling;
- public benchmark publication and regression dashboard;
- package publication and a narrowly scoped embedding facade.

Each post-v0.2 engine should enter through the conformance, model, planner, protocol, benchmark, and documentation gates established by this plan. Engine count is not itself a success metric.

## 20. Immediate next action

After this plan is merged, create `phase-0/recover-repository` from the latest `master` and execute Phase 0 only. The first deliverable is not a new OCR engine. It is a truthful, reproducible repository whose workspace and CI can be trusted as the foundation for every later phase.
