//! Host-scoped immutable blob references.

use std::{fmt, str::FromStr};

use schemars::{JsonSchema, r#gen::SchemaGenerator, schema::Schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{CoreError, MediaType, Sha256Digest, error::invalid_text};

/// An opaque host-issued blob capability token. It is never a filesystem path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlobId(String);

impl BlobId {
    /// Creates a token from 1-128 safe ASCII identifier characters.
    pub fn new(input: impl Into<String>) -> Result<Self, CoreError> {
        let input = input.into();
        if (1..=128).contains(&input.len())
            && input
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            Ok(Self(input))
        } else {
            Err(invalid_text(
                "blob ID",
                input,
                "expected 1-128 ASCII letters, digits, '.', '_', or '-'",
            ))
        }
    }

    /// Returns the opaque token.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlobId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for BlobId {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::new(input)
    }
}

impl Serialize for BlobId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for BlobId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

impl JsonSchema for BlobId {
    fn schema_name() -> String {
        "BlobId".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
}

/// A nonempty checked byte range inside a registered blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "RawRange")]
pub struct BlobRange {
    offset: u64,
    len: u64,
}

#[derive(Deserialize)]
struct RawRange {
    offset: u64,
    len: u64,
}

impl TryFrom<RawRange> for BlobRange {
    type Error = CoreError;

    fn try_from(value: RawRange) -> Result<Self, Self::Error> {
        Self::new(value.offset, value.len)
    }
}

impl BlobRange {
    /// Creates a nonempty range and rejects end overflow.
    pub fn new(offset: u64, len: u64) -> Result<Self, CoreError> {
        if len == 0 {
            return Err(invalid_text("blob range", "0", "length must be nonzero"));
        }
        offset
            .checked_add(len)
            .ok_or(CoreError::ArithmeticOverflow { kind: "blob range" })?;
        Ok(Self { offset, len })
    }

    /// Starting byte offset.
    pub const fn offset(self) -> u64 {
        self.offset
    }

    /// Number of bytes.
    pub const fn len(self) -> u64 {
        self.len
    }

    /// Returns true only for the impossible invalid zero-length state.
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    /// Exclusive end offset, proven not to overflow by construction.
    pub fn end(self) -> u64 {
        self.offset + self.len
    }
}

/// An immutable blob capability passed to an engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScopedBlob {
    /// Host-issued opaque capability token.
    pub id: BlobId,
    /// Checked byte range within the registered blob.
    pub range: BlobRange,
    /// Content media type.
    pub media_type: MediaType,
    /// Optional digest the host must verify before resolution.
    pub expected_digest: Option<Sha256Digest>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_reject_empty_and_overflow() {
        assert!(BlobRange::new(0, 0).is_err());
        assert!(BlobRange::new(u64::MAX, 1).is_err());
        assert_eq!(BlobRange::new(5, 7).unwrap().end(), 12);
    }

    #[test]
    fn blob_ids_cannot_encode_paths() {
        for input in ["../secret", "/etc/passwd", "dir/file", "dir\\file"] {
            assert!(BlobId::new(input).is_err());
        }
    }
}
