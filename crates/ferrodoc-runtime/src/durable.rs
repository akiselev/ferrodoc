//! Durable immutable artifacts and state-aware refinement reuse.

use std::{collections::BTreeMap, fmt, path::PathBuf, sync::Arc};

use ferrodoc_core::{ArtifactId, DocumentStateId, SchemaVersion, Sha256Digest};
use ferrodoc_ir::{Document, DocumentStateManifest, EvidenceDelta};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cache::{CacheError, CacheHit, CacheKeyParts, Cacheability, StageCache};

const STORE_VERSION: &str = "1";
const EVIDENCE_DELTA_MEDIA_TYPE: &str = "application/vnd.ferrodoc.evidence-delta+json;version=1";
/// Maximum canonical delta/manifest/checkpoint artifact accepted by the reference provider.
pub const MAX_DURABLE_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;

/// Physical artifact class. The class is part of physical identity, not logical state identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum DurableArtifactKind {
    /// Canonical immutable `EvidenceDelta` JSON.
    EvidenceDelta,
    /// Canonical retained `DocumentStateManifest` JSON.
    StateManifest,
    /// Canonical complete `Document` JSON checkpoint.
    DocumentCheckpoint,
}

impl DurableArtifactKind {
    fn label(self) -> &'static str {
        match self {
            Self::EvidenceDelta => "evidence-delta",
            Self::StateManifest => "state-manifest",
            Self::DocumentCheckpoint => "document-checkpoint",
        }
    }
}

/// A physical immutable realization and its separately retained logical identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DurableArtifactRef {
    /// Physical artifact class.
    pub kind: DurableArtifactKind,
    /// Physical identity derived from exact stored bytes and representation.
    pub artifact_id: ArtifactId,
    /// Digest of exact stored bytes.
    pub bytes_sha256: Sha256Digest,
    /// Exact uncompressed byte count.
    pub bytes: u64,
    /// Logical delta, state, or DocumentIR digest represented by the bytes.
    pub logical_id: String,
    /// Physical representation media type.
    pub representation: String,
}

/// Durable artifacts emitted by one progressive execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DurableExecutionArtifacts {
    /// One immutable physical artifact per returned delta.
    pub deltas: Vec<DurableArtifactRef>,
    /// Retained state-manifest realization.
    pub state_manifest: DurableArtifactRef,
    /// Canonical DocumentIR checkpoint realization.
    pub checkpoint: Option<DurableArtifactRef>,
}

/// Exact storage accounting; latency/resource measurements remain external observations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DurableStorageSummary {
    /// Original immutable PDF bytes read by this execution.
    pub source_pdf_bytes: u64,
    /// Pages in the materialized state.
    pub pages: u32,
    /// Sum of exact canonical delta artifact bytes.
    pub delta_bytes: u64,
    /// Exact retained manifest bytes.
    pub state_manifest_bytes: u64,
    /// Canonical checkpoint bytes, absent when policy skipped checkpointing.
    pub checkpoint_bytes: Option<u64>,
    /// `(delta + manifest) / PDF`; absent for an empty source.
    pub incremental_to_pdf_ratio: Option<f64>,
    /// `checkpoint / PDF`; absent without a checkpoint or with an empty source.
    pub checkpoint_to_pdf_ratio: Option<f64>,
}

impl DurableExecutionArtifacts {
    /// Derives deterministic byte accounting from retained physical references.
    pub fn summarize(&self, source_pdf_bytes: u64, pages: u32) -> DurableStorageSummary {
        let delta_bytes = self
            .deltas
            .iter()
            .fold(0_u64, |total, item| total.saturating_add(item.bytes));
        let checkpoint_bytes = self.checkpoint.as_ref().map(|item| item.bytes);
        let incremental = delta_bytes.saturating_add(self.state_manifest.bytes);
        DurableStorageSummary {
            source_pdf_bytes,
            pages,
            delta_bytes,
            state_manifest_bytes: self.state_manifest.bytes,
            checkpoint_bytes,
            incremental_to_pdf_ratio: (source_pdf_bytes != 0)
                .then_some(incremental as f64 / source_pdf_bytes as f64),
            checkpoint_to_pdf_ratio: (source_pdf_bytes != 0)
                .then(|| checkpoint_bytes.map(|bytes| bytes as f64 / source_pdf_bytes as f64))
                .flatten(),
        }
    }
}

/// Inputs to a caller-selected checkpoint-compaction policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointPolicyContext {
    /// Complete delta count in the resulting state.
    pub state_delta_count: usize,
    /// Tail delta count executed since the supplied checkpoint.
    pub tail_delta_count: usize,
    /// Exact canonical DocumentIR byte count if checkpointed.
    pub canonical_document_bytes: u64,
}

/// Policy hook for choosing physical checkpoint placement without renaming logical state.
pub trait CheckpointPolicy: Send + Sync {
    /// Returns whether the current complete canonical DocumentIR should be persisted.
    fn should_checkpoint(&self, context: CheckpointPolicyContext) -> bool;
}

/// Reference policies suitable for baseline and bounded replay compaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceCheckpointPolicy {
    /// Persist every returned state (the compatibility default).
    Always,
    /// Persist when the complete chain reaches this many deltas.
    DeltaCountAtLeast(usize),
    /// Retain only deltas and the state manifest.
    Never,
}

impl CheckpointPolicy for ReferenceCheckpointPolicy {
    fn should_checkpoint(&self, context: CheckpointPolicyContext) -> bool {
        match self {
            Self::Always => true,
            Self::DeltaCountAtLeast(threshold) => context.state_delta_count >= *threshold,
            Self::Never => false,
        }
    }
}

/// Durable persistence or verification failure.
#[derive(Debug, Error)]
pub enum DurableError {
    /// Underlying atomic cache/store operation failed.
    #[error(transparent)]
    Cache(#[from] CacheError),
    /// Artifact JSON was malformed or a future/incompatible shape was supplied.
    #[error("parse durable artifact: {0}")]
    Json(#[from] serde_json::Error),
    /// IR validation rejected a stored semantic artifact.
    #[error(transparent)]
    Ir(#[from] ferrodoc_ir::IrError),
    /// A required immutable artifact is absent.
    #[error("missing durable {kind} artifact {artifact_id}")]
    Missing {
        /// Expected artifact class.
        kind: &'static str,
        /// Expected physical identity.
        artifact_id: ArtifactId,
    },
    /// A physical reference or its logical binding is stale/corrupt.
    #[error("invalid durable artifact {artifact_id}: {reason}")]
    Invalid {
        /// Affected physical identity.
        artifact_id: ArtifactId,
        /// Stable explanation.
        reason: String,
    },
    /// Artifact exceeded the runtime's bounded canonical representation limit.
    #[error("durable artifact has {bytes} bytes, limit is {maximum}")]
    TooLarge {
        /// Observed or proposed byte count.
        bytes: u64,
        /// Configured hard limit.
        maximum: u64,
    },
}

/// Storage-provider boundary for immutable artifacts and semantic refinement-index entries.
///
/// An Artifactum adapter implements this runtime trait; semantic crates remain unaware of either
/// Artifactum or the filesystem reference layout.
pub trait DurableStorageProvider: Send + Sync {
    /// Reads an entry by its complete semantic key. Absence is a normal cold miss.
    fn get(
        &self,
        namespace: DurableNamespace,
        key: &CacheKeyParts,
    ) -> Result<Option<CacheHit>, CacheError>;

    /// Atomically publishes immutable bytes, converging or refusing on an existing mismatch.
    fn put(
        &self,
        namespace: DurableNamespace,
        key: &CacheKeyParts,
        cacheability: &Cacheability,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<(), CacheError>;
}

/// Low-cardinality durable namespace selected by the runtime contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableNamespace {
    /// Content-addressed retained artifacts.
    Artifacts,
    /// State-aware deterministic refinement results.
    Refinements,
}

/// Filesystem reference provider with atomic immutable publication.
#[derive(Debug, Clone)]
pub struct FilesystemDurableProvider {
    artifacts: StageCache,
    refinements: StageCache,
}

impl FilesystemDurableProvider {
    /// Opens the filesystem reference implementation under a caller-selected root.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, CacheError> {
        let root = root.into();
        Ok(Self {
            artifacts: StageCache::open(root.join("artifacts"))?,
            refinements: StageCache::open(root.join("refinements"))?,
        })
    }

    fn cache(&self, namespace: DurableNamespace) -> &StageCache {
        match namespace {
            DurableNamespace::Artifacts => &self.artifacts,
            DurableNamespace::Refinements => &self.refinements,
        }
    }
}

impl DurableStorageProvider for FilesystemDurableProvider {
    fn get(
        &self,
        namespace: DurableNamespace,
        key: &CacheKeyParts,
    ) -> Result<Option<CacheHit>, CacheError> {
        self.cache(namespace)
            .get_bounded(key, MAX_DURABLE_ARTIFACT_BYTES)
    }

    fn put(
        &self,
        namespace: DurableNamespace,
        key: &CacheKeyParts,
        cacheability: &Cacheability,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<(), CacheError> {
        self.cache(namespace)
            .put(key, cacheability, media_type, bytes)
    }
}

/// Typed durable state repository over a replaceable storage provider.
#[derive(Clone)]
pub struct DurableStateStore {
    provider: Arc<dyn DurableStorageProvider>,
}

impl fmt::Debug for DurableStateStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DurableStateStore")
            .finish_non_exhaustive()
    }
}

impl DurableStateStore {
    /// Opens a shared durable root. Independent workers may safely open the same root.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, DurableError> {
        Ok(Self::from_provider(Arc::new(
            FilesystemDurableProvider::open(root)?,
        )))
    }

    /// Uses a caller-provided durable backend such as Foundry's Artifactum adapter.
    pub fn from_provider(provider: Arc<dyn DurableStorageProvider>) -> Self {
        Self { provider }
    }

    /// Stores canonical delta JSON and returns physical and logical identities.
    pub fn persist_delta(&self, delta: &EvidenceDelta) -> Result<DurableArtifactRef, DurableError> {
        let bytes = delta.to_canonical_json()?;
        self.persist(
            DurableArtifactKind::EvidenceDelta,
            delta.id()?.to_string(),
            EVIDENCE_DELTA_MEDIA_TYPE,
            &bytes,
        )
    }

    /// Loads and verifies a required canonical delta artifact.
    pub fn load_delta(
        &self,
        reference: &DurableArtifactRef,
    ) -> Result<EvidenceDelta, DurableError> {
        self.require_kind(reference, DurableArtifactKind::EvidenceDelta)?;
        let bytes = self.load_bytes(reference)?;
        let delta: EvidenceDelta = serde_json::from_slice(&bytes)?;
        let logical = delta.id()?.to_string();
        self.require_logical(reference, &logical)?;
        if delta.to_canonical_json()? != bytes {
            return Err(self.invalid(reference, "delta bytes are not canonical JSON"));
        }
        Ok(delta)
    }

    /// Stores a retained state manifest without making its physical realization semantic.
    pub fn persist_manifest(
        &self,
        manifest: &DocumentStateManifest,
    ) -> Result<DurableArtifactRef, DurableError> {
        let bytes = manifest.to_canonical_json()?;
        self.persist(
            DurableArtifactKind::StateManifest,
            manifest.id()?.to_string(),
            "application/vnd.ferrodoc.document-state+json;version=1",
            &bytes,
        )
    }

    /// Loads and verifies a required retained state manifest.
    pub fn load_manifest(
        &self,
        reference: &DurableArtifactRef,
    ) -> Result<DocumentStateManifest, DurableError> {
        self.require_kind(reference, DurableArtifactKind::StateManifest)?;
        let bytes = self.load_bytes(reference)?;
        let manifest: DocumentStateManifest = serde_json::from_slice(&bytes)?;
        let logical = manifest.id()?.to_string();
        self.require_logical(reference, &logical)?;
        if manifest.to_canonical_json()? != bytes {
            return Err(self.invalid(reference, "manifest bytes are not canonical JSON"));
        }
        Ok(manifest)
    }

    /// Stores canonical complete DocumentIR checkpoint bytes.
    pub fn persist_checkpoint(
        &self,
        document: &Document,
    ) -> Result<DurableArtifactRef, DurableError> {
        let bytes = document.to_canonical_json()?;
        let logical = Sha256Digest::of_bytes(&bytes).to_string();
        self.persist(
            DurableArtifactKind::DocumentCheckpoint,
            logical,
            "application/vnd.ferrodoc.document-ir+json;version=1",
            &bytes,
        )
    }

    /// Loads and verifies a required canonical complete DocumentIR checkpoint.
    pub fn load_checkpoint(
        &self,
        reference: &DurableArtifactRef,
    ) -> Result<Document, DurableError> {
        self.require_kind(reference, DurableArtifactKind::DocumentCheckpoint)?;
        let bytes = self.load_bytes(reference)?;
        let document: Document = serde_json::from_slice(&bytes)?;
        let canonical = document.to_canonical_json()?;
        let logical = Sha256Digest::of_bytes(&canonical).to_string();
        self.require_logical(reference, &logical)?;
        if canonical != bytes {
            return Err(self.invalid(reference, "checkpoint bytes are not canonical DocumentIR"));
        }
        Ok(document)
    }

    /// Looks up a state-aware immutable delta result. Absence is a cold miss; invalid bytes fail.
    pub fn get_refinement(
        &self,
        key: &CacheKeyParts,
    ) -> Result<Option<EvidenceDelta>, DurableError> {
        let Some(hit) = self.provider.get(DurableNamespace::Refinements, key)? else {
            return Ok(None);
        };
        self.require_size(hit.bytes.len() as u64)?;
        if hit.media_type != EVIDENCE_DELTA_MEDIA_TYPE {
            return Err(DurableError::Invalid {
                artifact_id: ArtifactId::derive(&[b"ferrodoc-invalid-refinement"]),
                reason: "refinement cache has the wrong representation".into(),
            });
        }
        self.delta_from_hit(hit).map(Some)
    }

    /// Publishes one deterministic delta under its complete semantic execution key.
    pub fn put_refinement(
        &self,
        key: &CacheKeyParts,
        cacheability: &Cacheability,
        delta: &EvidenceDelta,
    ) -> Result<(), DurableError> {
        let bytes = delta.to_canonical_json()?;
        self.require_size(bytes.len() as u64)?;
        self.provider.put(
            DurableNamespace::Refinements,
            key,
            cacheability,
            EVIDENCE_DELTA_MEDIA_TYPE,
            &bytes,
        )?;
        Ok(())
    }

    fn persist(
        &self,
        kind: DurableArtifactKind,
        logical_id: String,
        representation: &str,
        bytes: &[u8],
    ) -> Result<DurableArtifactRef, DurableError> {
        self.require_size(bytes.len() as u64)?;
        let bytes_sha256 = Sha256Digest::of_bytes(bytes);
        let key = artifact_key(kind, bytes_sha256, representation);
        self.provider.put(
            DurableNamespace::Artifacts,
            &key,
            &Cacheability::Deterministic,
            representation,
            bytes,
        )?;
        Ok(DurableArtifactRef {
            kind,
            artifact_id: artifact_id(kind, bytes_sha256, representation),
            bytes_sha256,
            bytes: bytes.len() as u64,
            logical_id,
            representation: representation.into(),
        })
    }

    fn load_bytes(&self, reference: &DurableArtifactRef) -> Result<Vec<u8>, DurableError> {
        self.require_size(reference.bytes)?;
        let expected_id = artifact_id(
            reference.kind,
            reference.bytes_sha256,
            &reference.representation,
        );
        if expected_id != reference.artifact_id {
            return Err(self.invalid(reference, "physical identity does not match its metadata"));
        }
        let key = artifact_key(
            reference.kind,
            reference.bytes_sha256,
            &reference.representation,
        );
        let hit = self
            .provider
            .get(DurableNamespace::Artifacts, &key)?
            .ok_or_else(|| DurableError::Missing {
                kind: reference.kind.label(),
                artifact_id: reference.artifact_id.clone(),
            })?;
        if hit.media_type != reference.representation
            || hit.bytes.len() as u64 != reference.bytes
            || Sha256Digest::of_bytes(&hit.bytes) != reference.bytes_sha256
        {
            return Err(self.invalid(reference, "stored bytes or representation differ"));
        }
        Ok(hit.bytes)
    }

    fn delta_from_hit(&self, hit: CacheHit) -> Result<EvidenceDelta, DurableError> {
        let delta: EvidenceDelta = serde_json::from_slice(&hit.bytes)?;
        if delta.to_canonical_json()? != hit.bytes {
            return Err(DurableError::Invalid {
                artifact_id: ArtifactId::derive(&[b"ferrodoc-invalid-refinement"]),
                reason: "refinement cache contains noncanonical delta bytes".into(),
            });
        }
        Ok(delta)
    }

    fn require_kind(
        &self,
        reference: &DurableArtifactRef,
        expected: DurableArtifactKind,
    ) -> Result<(), DurableError> {
        if reference.kind != expected {
            return Err(self.invalid(reference, "artifact has the wrong physical class"));
        }
        Ok(())
    }

    fn require_logical(
        &self,
        reference: &DurableArtifactRef,
        actual: &str,
    ) -> Result<(), DurableError> {
        if reference.logical_id != actual {
            return Err(self.invalid(reference, "stored semantic identity differs from reference"));
        }
        Ok(())
    }

    fn invalid(&self, reference: &DurableArtifactRef, reason: &str) -> DurableError {
        DurableError::Invalid {
            artifact_id: reference.artifact_id.clone(),
            reason: reason.into(),
        }
    }

    fn require_size(&self, bytes: u64) -> Result<(), DurableError> {
        if bytes > MAX_DURABLE_ARTIFACT_BYTES {
            return Err(DurableError::TooLarge {
                bytes,
                maximum: MAX_DURABLE_ARTIFACT_BYTES,
            });
        }
        Ok(())
    }
}

/// Inputs to the complete state-aware refinement key.
pub struct RefinementKeyInput<'a> {
    /// Registered semantic stage.
    pub stage: &'a str,
    /// Immutable stage/model/configuration build identity.
    pub stage_build: Sha256Digest,
    /// Exact model identity, when the stage uses one.
    pub model_digest: Option<Sha256Digest>,
    /// Exact source PDF digest.
    pub source_pdf_sha256: Sha256Digest,
    /// Pinned logical state against which the stage was planned.
    pub input_state_id: &'a DocumentStateId,
    /// Selected engine ID.
    pub engine_id: &'a str,
    /// Selected engine implementation/version.
    pub engine_version: &'a str,
    /// Interpreted DocumentIR schema.
    pub schema_version: SchemaVersion,
    /// Normalized scope, configuration, page, and seed inputs.
    pub parameters: &'a BTreeMap<String, serde_json::Value>,
}

/// Builds the complete refinement key, including the pinned logical input state.
pub fn refinement_key(input: RefinementKeyInput<'_>) -> Result<CacheKeyParts, DurableError> {
    let mut semantic_parameters = input.parameters.clone();
    semantic_parameters.insert(
        "ferrodoc.input_state_id".into(),
        serde_json::to_value(input.input_state_id)?,
    );
    semantic_parameters.insert(
        "ferrodoc.stage_build".into(),
        serde_json::to_value(input.stage_build)?,
    );
    let model_digests = input
        .model_digest
        .map(|digest| BTreeMap::from([("primary".into(), digest)]))
        .unwrap_or_default();
    CacheKeyParts::with_parameters(
        input.stage,
        input.source_pdf_sha256,
        model_digests,
        input.engine_id,
        input.engine_version,
        input.schema_version,
        &semantic_parameters,
    )
    .map_err(Into::into)
}

fn artifact_key(
    kind: DurableArtifactKind,
    bytes_sha256: Sha256Digest,
    representation: &str,
) -> CacheKeyParts {
    CacheKeyParts {
        stage: format!("durable.{}", kind.label()),
        input_digest: bytes_sha256,
        model_digests: BTreeMap::new(),
        engine_id: "ferrodoc-durable-store".into(),
        engine_version: STORE_VERSION.into(),
        schema_version: ferrodoc_core::CURRENT_SCHEMA_VERSION,
        parameter_digest: Sha256Digest::of_bytes(representation.as_bytes()),
    }
}

fn artifact_id(
    kind: DurableArtifactKind,
    bytes_sha256: Sha256Digest,
    representation: &str,
) -> ArtifactId {
    ArtifactId::derive(&[
        b"ferrodoc-durable-artifact/1",
        kind.label().as_bytes(),
        representation.as_bytes(),
        bytes_sha256.as_bytes(),
    ])
}
