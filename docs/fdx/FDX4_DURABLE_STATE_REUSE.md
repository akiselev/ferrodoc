# FDX4 — Durable state and refinement reuse

Status: implemented reference contract

FP4 makes the FP0–FP3 immutable delta/state model durable without introducing an
Artifactum dependency into `ferrodoc-ir`, `ferrodoc-core`, or an engine crate. The runtime exposes
`DurableStorageProvider`; the checked-in `FilesystemDurableProvider` is an atomic reference
implementation, while Foundry can provide an Artifactum-backed implementation at the same seam.

## Physical artifacts and logical identities

`DurableStateStore` imports and exports canonical JSON realizations of `EvidenceDelta`,
`DocumentStateManifest`, and complete `Document` checkpoints. Each `DurableArtifactRef` retains:

- a physical artifact ID derived from artifact class, representation, and exact byte digest;
- exact byte digest and length;
- the separately validated logical delta/state/DocumentIR identity;
- the representation media type.

Loading validates the complete physical reference, byte digest/length, canonical serialization,
schema, and logical binding. Missing artifacts are explicit errors. Malformed, corrupt, future
schema, wrong-kind, and stale logical references fail closed. Changing a checkpoint, encoding, or
manifest realization can change the physical artifact ID but cannot rename the logical
`DocumentStateId`. The filesystem reference provider checks metadata before allocation and applies
a 512 MiB hard bound to any canonical delta, manifest, checkpoint, or cached refinement artifact.

FP4 persists canonical JSON. It does not add a custom binary archive. The provider seam permits a
Foundry deployment to compress a physical realization, but compression metadata is never admitted
to logical state identity.

## State-aware deterministic reuse

The durable refinement key covers the exact source PDF digest, pinned input `DocumentStateId`,
stage identity and immutable build digest, engine ID/version, IR schema, normalized scope/config,
model/build identities, and deterministic seed. A warm lookup returns canonical immutable
`EvidenceDelta` bytes rather than an opaque final conversion result. The runtime revalidates the
delta source, input state, stage, scope, producer build/version, and configuration before skipping
engine admission/execution. Unseeded nondeterministic stages do not enter this cache.

Independent workers sharing a provider converge on the same semantic key and exact delta bytes.
Atomic publication rejects an existing or concurrent different byte sequence for that key. Cache
decisions and complete key digests are observations in `durable_reuse`; they do not affect delta or
state identity.

## Materialization and compaction

Every durable execution retains its deltas and state manifest. `CheckpointPolicy` selects whether
to persist the complete canonical DocumentIR, independently from logical state identity. The
reference policies support always, never, and a complete-delta-count threshold; Foundry may provide
a benchmark-driven policy. Omitting a checkpoint remains replayable from the full delta set.

The FP4 oracle proves that replaying all retained deltas and replaying an older canonical
checkpoint plus only its tail produce byte-identical canonical DocumentIR. The older state manifest,
checkpoint, and evidence IDs remain loadable and resolvable after a newer refinement is stored.

`DurableExecutionArtifacts::summarize` reports exact PDF, delta, manifest, and checkpoint bytes plus
incremental/PDF and checkpoint/PDF ratios. Wall/CPU time, peak RAM/VRAM, compressed size, page render
latency, quality, coverage gain, and document-family failure rates are deliberately external
observations, not semantic artifacts.

## Acceptance evidence and limits

The runtime regressions cover:

- cold miss followed by a separate worker's verified warm hit with no engine execution;
- identical delta, logical state, canonical document, and physical artifacts across those workers;
- immutable delta/manifest/checkpoint round trips;
- full-delta versus older-checkpoint-plus-tail canonical equivalence;
- old evidence-anchor resolution after later state generation;
- checkpoint policy that stores deltas/manifests without a checkpoint;
- state identity stability across different physical manifest realizations;
- missing, stale, and corrupt artifact refusal;
- exact storage-amplification byte accounting.

The purpose-built FP1/FP3 engine and minimized PDF/table fixtures are protocol and span oracles, not
representative corpus-quality evidence. FP4 does not claim Artifactum integration, Zstd benchmark
results, multi-host object-store performance, a real-datasheet corpus benchmark, or measured
CPU/GPU/RAM/VRAM/latency/quality coverage by document family. Those remain Foundry FLS5 deployment
and corpus gates.
