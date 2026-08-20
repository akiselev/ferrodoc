//! Stable, domain-separated content identities.

use std::{fmt, str::FromStr};

use schemars::{JsonSchema, r#gen::SchemaGenerator, schema::Schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{CoreError, Sha256Digest, error::invalid_text};

fn derive_id(prefix: &'static str, parts: &[&[u8]]) -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(prefix.as_bytes());
    bytes.push(0);
    for part in parts {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    format!("{prefix}_{}", Sha256Digest::of_bytes(&bytes))
}

fn validate_id(prefix: &'static str, input: &str) -> Result<(), CoreError> {
    let digest = input
        .strip_prefix(prefix)
        .and_then(|value| value.strip_prefix('_'))
        .ok_or_else(|| invalid_text("stable ID", input, "invalid domain prefix"))?;
    digest.parse::<Sha256Digest>().map(|_| ())
}

macro_rules! stable_id {
    ($name:ident, $prefix:literal, $docs:literal) => {
        #[doc = $docs]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Derives an ID from domain-separated, length-prefixed deterministic bytes.
            pub fn derive(parts: &[&[u8]]) -> Self {
                Self(derive_id($prefix, parts))
            }

            /// Returns the canonical textual ID.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = CoreError;

            fn from_str(input: &str) -> Result<Self, Self::Err> {
                validate_id($prefix, input)?;
                Ok(Self(input.into()))
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                String::deserialize(deserializer)?
                    .parse()
                    .map_err(D::Error::custom)
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> String {
                stringify!($name).into()
            }

            fn json_schema(generator: &mut SchemaGenerator) -> Schema {
                String::json_schema(generator)
            }
        }
    };
}

stable_id!(DocumentId, "doc", "Stable document identity.");
stable_id!(PageId, "page", "Stable page identity.");
stable_id!(RegionId, "region", "Stable region identity.");
stable_id!(EvidenceId, "evidence", "Stable evidence identity.");
stable_id!(ModelId, "model", "Stable model identity.");
stable_id!(RequestId, "request", "Stable request identity.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_domain_separated_and_stable() {
        let first = DocumentId::derive(&[b"a", b"bc"]);
        let second = DocumentId::derive(&[b"ab", b"c"]);
        let page = PageId::derive(&[b"a", b"bc"]);
        assert_ne!(first, second);
        assert_ne!(first.as_str(), page.as_str());
        assert_eq!(first.to_string().parse::<DocumentId>().unwrap(), first);
        assert!(first.to_string().parse::<PageId>().is_err());
    }
}
