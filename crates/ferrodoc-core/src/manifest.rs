//! Persistent model manifest contracts.

use std::{collections::BTreeSet, fmt, str::FromStr};

use schemars::{JsonSchema, r#gen::SchemaGenerator, schema::Schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{
    Bytes, CURRENT_SCHEMA_VERSION, CoreError, MediaType, ModelId, SchemaVersion, Sha256Digest,
    error::invalid_text,
};

/// A normalized logical path inside an immutable model view.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativePath(String);

impl RelativePath {
    /// Creates a slash-separated relative path with no traversal or platform prefix.
    pub fn new(input: impl Into<String>) -> Result<Self, CoreError> {
        let input = input.into();
        let segments: Vec<_> = input.split('/').collect();
        let invalid = input.is_empty()
            || input.starts_with('/')
            || input.ends_with('/')
            || input.contains(['\\', '\0', ':'])
            || segments
                .iter()
                .any(|segment| segment.is_empty() || matches!(*segment, "." | ".."));
        if invalid {
            Err(invalid_text(
                "relative path",
                input,
                "expected normalized slash-separated segments without traversal or prefix",
            ))
        } else {
            Ok(Self(input))
        }
    }

    /// Returns the normalized logical path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for RelativePath {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::new(input)
    }
}

impl Serialize for RelativePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

impl JsonSchema for RelativePath {
    fn schema_name() -> String {
        "RelativePath".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
}

/// License and source metadata displayed before model installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LicenseMetadata {
    /// SPDX license expression or `LicenseRef-*` identifier.
    pub expression: String,
    /// Human-readable source location.
    pub source: String,
    /// Optional notice text that must accompany the logical view.
    pub notice: Option<String>,
}

/// An explicit acceptance gate for model installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AcceptanceRequirement {
    /// The caller must affirm the stated license terms.
    License {
        /// Stable acceptance prompt.
        prompt: String,
    },
    /// The caller must affirm a use restriction supplied by the distributor.
    UsageTerms {
        /// Stable acceptance prompt.
        prompt: String,
    },
}

/// One immutable file in a model manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "RawModelFile")]
pub struct ModelFile {
    path: RelativePath,
    digest: Sha256Digest,
    bytes: Bytes,
    media_type: MediaType,
}

#[derive(Deserialize)]
struct RawModelFile {
    path: RelativePath,
    digest: Sha256Digest,
    bytes: Bytes,
    media_type: MediaType,
}

impl TryFrom<RawModelFile> for ModelFile {
    type Error = CoreError;

    fn try_from(value: RawModelFile) -> Result<Self, Self::Error> {
        Self::new(value.path, value.digest, value.bytes, value.media_type)
    }
}

impl ModelFile {
    /// Creates a nonempty model file record.
    pub fn new(
        path: RelativePath,
        digest: Sha256Digest,
        bytes: Bytes,
        media_type: MediaType,
    ) -> Result<Self, CoreError> {
        if bytes.get() == 0 {
            return Err(invalid_text(
                "model file size",
                "0",
                "model files must be nonempty",
            ));
        }
        Ok(Self {
            path,
            digest,
            bytes,
            media_type,
        })
    }

    /// Logical relative path.
    pub fn path(&self) -> &RelativePath {
        &self.path
    }

    /// Expected content digest.
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Expected byte count.
    pub const fn bytes(&self) -> Bytes {
        self.bytes
    }

    /// Declared media type.
    pub fn media_type(&self) -> &MediaType {
        &self.media_type
    }
}

/// A versioned, content-addressed model manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "RawModelManifest")]
pub struct ModelManifest {
    schema_version: SchemaVersion,
    id: ModelId,
    revision: String,
    files: Vec<ModelFile>,
    license: LicenseMetadata,
    acceptance: Option<AcceptanceRequirement>,
}

#[derive(Deserialize)]
struct RawModelManifest {
    schema_version: SchemaVersion,
    id: ModelId,
    revision: String,
    files: Vec<ModelFile>,
    license: LicenseMetadata,
    acceptance: Option<AcceptanceRequirement>,
}

impl TryFrom<RawModelManifest> for ModelManifest {
    type Error = CoreError;

    fn try_from(value: RawModelManifest) -> Result<Self, Self::Error> {
        Self::new(
            value.schema_version,
            value.id,
            value.revision,
            value.files,
            value.license,
            value.acceptance,
        )
    }
}

impl ModelManifest {
    /// Creates a manifest and validates version, metadata, nonempty files, and unique paths.
    pub fn new(
        schema_version: SchemaVersion,
        id: ModelId,
        revision: impl Into<String>,
        files: Vec<ModelFile>,
        license: LicenseMetadata,
        acceptance: Option<AcceptanceRequirement>,
    ) -> Result<Self, CoreError> {
        let revision = revision.into();
        if schema_version.major != CURRENT_SCHEMA_VERSION.major {
            return Err(invalid_text(
                "model manifest schema",
                format!("{}.{}", schema_version.major, schema_version.minor),
                "unsupported major version",
            ));
        }
        if revision.trim().is_empty() {
            return Err(invalid_text(
                "model revision",
                revision,
                "revision must be nonempty",
            ));
        }
        if files.is_empty() {
            return Err(invalid_text(
                "model files",
                "[]",
                "at least one file is required",
            ));
        }
        if license.expression.trim().is_empty() || license.source.trim().is_empty() {
            return Err(invalid_text(
                "model license",
                &license.expression,
                "license expression and source must be nonempty",
            ));
        }
        let unique: BTreeSet<_> = files.iter().map(ModelFile::path).collect();
        if unique.len() != files.len() {
            return Err(invalid_text(
                "model files",
                "duplicate path",
                "logical paths must be unique",
            ));
        }
        Ok(Self {
            schema_version,
            id,
            revision,
            files,
            license,
            acceptance,
        })
    }

    /// Schema version.
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    /// Stable model identity.
    pub fn id(&self) -> &ModelId {
        &self.id
    }

    /// Immutable source revision.
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Manifest files.
    pub fn files(&self) -> &[ModelFile] {
        &self.files
    }

    /// License metadata.
    pub const fn license(&self) -> &LicenseMetadata {
        &self.license
    }

    /// Optional acceptance requirement.
    pub const fn acceptance(&self) -> Option<&AcceptanceRequirement> {
        self.acceptance.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_reject_escape_and_platform_prefixes() {
        for input in [
            "", "/root", "../model", "a/../b", "a//b", "C:/model", "a\\b",
        ] {
            assert!(RelativePath::new(input).is_err(), "accepted {input:?}");
        }
        assert_eq!(
            RelativePath::new("weights/model.bin").unwrap().as_str(),
            "weights/model.bin"
        );
    }

    #[test]
    fn relative_path_deserialization_validates() {
        assert!(serde_json::from_str::<RelativePath>("\"../secret\"").is_err());
    }
}
