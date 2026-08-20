//! Persistent schema versioning helpers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Schema version used by Phase 1 persistent formats.
pub const CURRENT_SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(1, 0);

/// A semantic schema version. Major changes require migration; minor additions are compatible.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct SchemaVersion {
    /// Compatibility-breaking version.
    pub major: u16,
    /// Backward-compatible additive version.
    pub minor: u16,
}

impl SchemaVersion {
    /// Creates a version.
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Checks whether a reader supporting `self` can consume `found`.
    pub const fn compatibility_with(self, found: Self) -> Compatibility {
        if self.major != found.major {
            Compatibility::IncompatibleMajor
        } else if found.minor > self.minor {
            Compatibility::NewerMinor
        } else {
            Compatibility::Compatible
        }
    }
}

/// Compatibility result for a persistent schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compatibility {
    /// The document can be consumed directly.
    Compatible,
    /// The major version matches, but the producer may have added unknown fields.
    NewerMinor,
    /// A migration or a different reader is required.
    IncompatibleMajor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_compatibility_is_explicit() {
        let reader = SchemaVersion::new(1, 2);
        assert_eq!(
            reader.compatibility_with(SchemaVersion::new(1, 1)),
            Compatibility::Compatible
        );
        assert_eq!(
            reader.compatibility_with(SchemaVersion::new(1, 3)),
            Compatibility::NewerMinor
        );
        assert_eq!(
            reader.compatibility_with(SchemaVersion::new(2, 0)),
            Compatibility::IncompatibleMajor
        );
    }
}
