//! Runtime-agnostic contracts shared across Ferrodoc.
//!
//! Constructors and deserializers enforce the same invariants. Callers cannot
//! create invalid geometry, quantities, digests, relative paths, or blob ranges
//! through the public API.

mod blob;
mod canonical;
mod digest;
mod error;
mod geometry;
mod id;
mod manifest;
mod provenance;
mod quantity;
mod resource;
mod schema;

pub use blob::{BlobId, BlobRange, ScopedBlob};
pub use canonical::{
    BackendId, Capability, DeviceId, DeviceKind, MediaType, PlacementPolicy, Profile,
};
pub use digest::Sha256Digest;
pub use error::CoreError;
pub use geometry::{CoordinateSpace, CoordinateTransform, PageRect, Rect, Unit};
pub use id::{
    ArtifactId, DocumentId, DocumentStateId, EvidenceDeltaId, EvidenceId, LayerId, ModelId, PageId,
    RegionId, RequestId,
};
pub use manifest::{
    AcceptanceRequirement, LicenseMetadata, ModelFile, ModelManifest, RelativePath,
};
pub use provenance::{DeterministicProvenance, Observation, Stage};
pub use quantity::{Bytes, MicroUsd, Millis, Probability};
pub use resource::{Estimate, EstimateConfidence, EstimateSource, ResourceEstimate};
pub use schema::{CURRENT_SCHEMA_VERSION, Compatibility, SchemaVersion};
