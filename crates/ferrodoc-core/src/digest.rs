//! Algorithm-specific content digests.

use std::{fmt, fs::File, io::Read, path::Path, str::FromStr};

use schemars::{JsonSchema, r#gen::SchemaGenerator, schema::Schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use sha2::{Digest as _, Sha256};

use crate::{CoreError, error::invalid_text};

/// A SHA-256 digest with exactly 32 bytes and canonical lowercase hexadecimal text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// Hashes bytes.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(hasher.finalize().into())
    }

    /// Hashes a file without loading it all into memory.
    pub fn of_file(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        Ok(Self(hasher.finalize().into()))
    }

    /// Creates a digest from its exact bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns canonical lowercase hexadecimal text.
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&hex::encode(self.0))
    }
}

impl FromStr for Sha256Digest {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.len() != 64 || input.bytes().any(|byte| !byte.is_ascii_hexdigit()) {
            return Err(invalid_text(
                "SHA-256 digest",
                input,
                "expected exactly 64 hexadecimal digits",
            ));
        }
        if input.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(invalid_text(
                "SHA-256 digest",
                input,
                "uppercase hexadecimal is not canonical",
            ));
        }
        let decoded = hex::decode(input)
            .map_err(|_| invalid_text("SHA-256 digest", input, "expected lowercase hexadecimal"))?;
        let bytes: [u8; 32] = decoded.try_into().map_err(|_| {
            invalid_text("SHA-256 digest", input, "expected exactly 32 digest bytes")
        })?;
        Ok(Self(bytes))
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

impl JsonSchema for Sha256Digest {
    fn schema_name() -> String {
        "Sha256Digest".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_parsing_is_typed_and_canonical() {
        let digest = Sha256Digest::of_bytes(b"ferrodoc");
        assert_eq!(digest.to_string().parse::<Sha256Digest>().unwrap(), digest);
        assert!("00".parse::<Sha256Digest>().is_err());
        assert!(
            digest
                .to_string()
                .to_ascii_uppercase()
                .parse::<Sha256Digest>()
                .is_err()
        );
    }

    #[test]
    fn file_and_bytes_hashes_match() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input");
        std::fs::write(&path, b"ferrodoc").unwrap();
        assert_eq!(
            Sha256Digest::of_file(path).unwrap(),
            Sha256Digest::of_bytes(b"ferrodoc")
        );
    }
}
