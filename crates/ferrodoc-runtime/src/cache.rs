//! Deterministic stage cache with atomic entry publication.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use ferrodoc_core::{SchemaVersion, Sha256Digest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

const MAX_METADATA_BYTES: u64 = 1024 * 1024;

/// Every semantic input to a stage cache key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CacheKeyParts {
    /// Stage or capability namespace.
    pub stage: String,
    /// Immutable input artifact digest.
    pub input_digest: Sha256Digest,
    /// Model digests keyed by semantic role.
    #[serde(default)]
    pub model_digests: BTreeMap<String, Sha256Digest>,
    /// Engine ID.
    pub engine_id: String,
    /// Engine implementation/version identity.
    pub engine_version: String,
    /// Output schema version.
    pub schema_version: SchemaVersion,
    /// Digest of normalized parameters.
    pub parameter_digest: Sha256Digest,
}

impl CacheKeyParts {
    /// Hashes normalized JSON parameters.
    pub fn with_parameters(
        stage: impl Into<String>,
        input_digest: Sha256Digest,
        model_digests: BTreeMap<String, Sha256Digest>,
        engine_id: impl Into<String>,
        engine_version: impl Into<String>,
        schema_version: SchemaVersion,
        parameters: &BTreeMap<String, serde_json::Value>,
    ) -> Result<Self, CacheError> {
        let normalized = serde_json::to_vec(parameters)?;
        Ok(Self {
            stage: stage.into(),
            input_digest,
            model_digests,
            engine_id: engine_id.into(),
            engine_version: engine_version.into(),
            schema_version,
            parameter_digest: Sha256Digest::of_bytes(&normalized),
        })
    }

    /// Derives the content-addressed cache entry ID.
    pub fn digest(&self) -> Result<Sha256Digest, CacheError> {
        Ok(Sha256Digest::of_bytes(&serde_json::to_vec(self)?))
    }
}

/// Whether a stage result may enter the deterministic cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Cacheability {
    /// Engine is deterministic for these inputs.
    Deterministic,
    /// Nondeterminism is controlled by this explicit seed.
    Seeded {
        /// Cache-relevant deterministic seed.
        seed: u64,
    },
    /// Stage cannot promise repeatable semantics.
    Uncacheable {
        /// Stable explanation.
        reason: String,
    },
}

/// Verified cache hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheHit {
    /// Entry bytes.
    pub bytes: Vec<u8>,
    /// Declared media type.
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EntryMetadata {
    key: CacheKeyParts,
    value_digest: Sha256Digest,
    bytes: u64,
    media_type: String,
}

/// Cache failure.
#[derive(Debug, Error)]
pub enum CacheError {
    /// File-system operation failed.
    #[error("{operation} {path:?}: {source}")]
    Io {
        /// Stable operation name.
        operation: &'static str,
        /// Affected path.
        path: PathBuf,
        /// Underlying error.
        source: io::Error,
    },
    /// Metadata or key serialization failed.
    #[error("serialize or parse cache metadata: {0}")]
    Metadata(#[from] serde_json::Error),
    /// Existing cache state failed digest, size, or key validation.
    #[error("cache corruption at {path:?}: {reason}")]
    Corrupt {
        /// Corrupt entry path.
        path: PathBuf,
        /// Stable explanation.
        reason: String,
    },
    /// Caller attempted to cache a nondeterministic result.
    #[error("stage is not cacheable: {0}")]
    Uncacheable(String),
}

/// Filesystem cache rooted at a caller-selected directory.
#[derive(Debug, Clone)]
pub struct StageCache {
    root: PathBuf,
}

impl StageCache {
    /// Opens or creates a cache.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, CacheError> {
        let cache = Self { root: root.into() };
        create_dir_all(&cache.entries_dir())?;
        create_dir_all(&cache.staging_dir())?;
        Ok(cache)
    }

    /// Reads and validates an entry. Absence is a normal cache miss.
    pub fn get(&self, key: &CacheKeyParts) -> Result<Option<CacheHit>, CacheError> {
        self.get_bounded(key, u64::MAX)
    }

    /// Reads and validates an entry without allocating beyond a caller-selected value bound.
    pub fn get_bounded(
        &self,
        key: &CacheKeyParts,
        maximum_bytes: u64,
    ) -> Result<Option<CacheHit>, CacheError> {
        let digest = key.digest()?;
        let directory = self.entries_dir().join(digest.to_string());
        if !directory.exists() {
            return Ok(None);
        }
        let metadata_path = directory.join("metadata.json");
        let value_path = directory.join("value.bin");
        let metadata: EntryMetadata =
            serde_json::from_slice(&read_bounded(&metadata_path, MAX_METADATA_BYTES)?)?;
        if metadata.key != *key {
            return Err(CacheError::Corrupt {
                path: directory,
                reason: "stored semantic key differs from requested key".into(),
            });
        }
        let file_type = fs::symlink_metadata(&value_path)
            .map_err(|source| io_error("inspect cache value", &value_path, source))?
            .file_type();
        if !file_type.is_file() {
            return Err(CacheError::Corrupt {
                path: value_path,
                reason: "cache value is not a regular file".into(),
            });
        }
        let value_bytes = fs::metadata(&value_path)
            .map_err(|source| io_error("inspect cache value", &value_path, source))?
            .len();
        if value_bytes != metadata.bytes || value_bytes > maximum_bytes {
            return Err(CacheError::Corrupt {
                path: value_path,
                reason: "cache value size differs from metadata or exceeds its read bound".into(),
            });
        }
        let bytes = fs::read(&value_path)
            .map_err(|source| io_error("read cache value", &value_path, source))?;
        let actual = Sha256Digest::of_bytes(&bytes);
        if bytes.len() as u64 != metadata.bytes || actual != metadata.value_digest {
            return Err(CacheError::Corrupt {
                path: directory,
                reason: "cache value size or digest mismatch".into(),
            });
        }
        Ok(Some(CacheHit {
            bytes,
            media_type: metadata.media_type,
        }))
    }

    /// Atomically publishes a verified cache entry.
    pub fn put(
        &self,
        key: &CacheKeyParts,
        cacheability: &Cacheability,
        media_type: impl Into<String>,
        bytes: &[u8],
    ) -> Result<(), CacheError> {
        if let Cacheability::Uncacheable { reason } = cacheability {
            return Err(CacheError::Uncacheable(reason.clone()));
        }
        let digest = key.digest()?;
        let destination = self.entries_dir().join(digest.to_string());
        if destination.exists() {
            let hit = self.get(key)?.ok_or_else(|| CacheError::Corrupt {
                path: destination.clone(),
                reason: "entry disappeared during verification".into(),
            })?;
            if hit.bytes != bytes {
                return Err(CacheError::Corrupt {
                    path: destination,
                    reason: "same semantic key produced different bytes".into(),
                });
            }
            return Ok(());
        }
        let staging = self.staging_dir().join(Uuid::new_v4().simple().to_string());
        create_dir_all(&staging)?;
        let result = (|| {
            atomic_write(&staging.join("value.bin"), bytes)?;
            let metadata = EntryMetadata {
                key: key.clone(),
                value_digest: Sha256Digest::of_bytes(bytes),
                bytes: bytes.len() as u64,
                media_type: media_type.into(),
            };
            atomic_write(
                &staging.join("metadata.json"),
                &serde_json::to_vec_pretty(&metadata)?,
            )?;
            match fs::rename(&staging, &destination) {
                Ok(()) => {}
                Err(error) if destination.exists() => {
                    fs::remove_dir_all(&staging).map_err(|source| {
                        io_error("remove duplicate cache staging", &staging, source)
                    })?;
                    let _ = error;
                }
                Err(source) => {
                    return Err(io_error("publish cache entry", &destination, source));
                }
            }
            let published = self.get(key)?.ok_or_else(|| CacheError::Corrupt {
                path: destination,
                reason: "published entry is not visible".into(),
            })?;
            if published.bytes != bytes {
                return Err(CacheError::Corrupt {
                    path: self.entries_dir().join(digest.to_string()),
                    reason: "concurrent producer published different bytes for one semantic key"
                        .into(),
                });
            }
            Ok(())
        })();
        if result.is_err() && staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }

    fn entries_dir(&self) -> PathBuf {
        self.root.join("entries")
    }

    fn staging_dir(&self) -> PathBuf {
        self.root.join("staging")
    }

    pub(crate) fn root_path_for_error(&self) -> PathBuf {
        self.root.clone()
    }
}

fn read_bounded(path: &Path, maximum_bytes: u64) -> Result<Vec<u8>, CacheError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| io_error("inspect cache file", path, source))?;
    if !metadata.file_type().is_file() || metadata.len() > maximum_bytes {
        return Err(CacheError::Corrupt {
            path: path.to_owned(),
            reason: "cache file is not regular or exceeds its read bound".into(),
        });
    }
    fs::read(path).map_err(|source| io_error("read cache file", path, source))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), CacheError> {
    let parent = path.parent().expect("cache file has parent");
    create_dir_all(parent)?;
    let temporary = parent.join(format!(".tmp-{}", Uuid::new_v4().simple()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|source| io_error("create temporary cache file", &temporary, source))?;
    let result = (|| {
        file.write_all(bytes)
            .map_err(|source| io_error("write temporary cache file", &temporary, source))?;
        file.sync_all()
            .map_err(|source| io_error("sync temporary cache file", &temporary, source))?;
        fs::rename(&temporary, path).map_err(|source| io_error("publish cache file", path, source))
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn create_dir_all(path: &Path) -> Result<(), CacheError> {
    fs::create_dir_all(path).map_err(|source| io_error("create cache directory", path, source))
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> CacheError {
    CacheError::Io {
        operation,
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use ferrodoc_core::CURRENT_SCHEMA_VERSION;

    use super::*;

    fn key(parameters: BTreeMap<String, serde_json::Value>) -> CacheKeyParts {
        CacheKeyParts::with_parameters(
            "ocr.page",
            Sha256Digest::of_bytes(b"input"),
            BTreeMap::from([("recognition".into(), Sha256Digest::of_bytes(b"model"))]),
            "ocrs",
            "1.0.0",
            CURRENT_SCHEMA_VERSION,
            &parameters,
        )
        .unwrap()
    }

    #[test]
    fn hits_are_stable_across_reopen_and_parameter_order() {
        let directory = tempfile::tempdir().unwrap();
        let first = key(BTreeMap::from([
            ("dpi".into(), serde_json::json!(144)),
            ("language".into(), serde_json::json!("en")),
        ]));
        let second = key(BTreeMap::from([
            ("language".into(), serde_json::json!("en")),
            ("dpi".into(), serde_json::json!(144)),
        ]));
        assert_eq!(first.digest().unwrap(), second.digest().unwrap());
        let cache = StageCache::open(directory.path()).unwrap();
        cache
            .put(
                &first,
                &Cacheability::Deterministic,
                "application/cbor",
                b"evidence",
            )
            .unwrap();
        drop(cache);
        assert_eq!(
            StageCache::open(directory.path())
                .unwrap()
                .get(&second)
                .unwrap()
                .unwrap()
                .bytes,
            b"evidence"
        );
    }

    #[test]
    fn every_semantic_identity_change_invalidates_the_key() {
        let base = key(BTreeMap::new());
        let mut variants = Vec::new();
        let mut value = base.clone();
        value.input_digest = Sha256Digest::of_bytes(b"other input");
        variants.push(value);
        let mut value = base.clone();
        value
            .model_digests
            .insert("recognition".into(), Sha256Digest::of_bytes(b"other model"));
        variants.push(value);
        let mut value = base.clone();
        value.engine_version = "2.0.0".into();
        variants.push(value);
        let mut value = base.clone();
        value.engine_id = "other-engine".into();
        variants.push(value);
        let mut value = base.clone();
        value.stage = "layout.detect".into();
        variants.push(value);
        let mut value = base.clone();
        value.schema_version.minor += 1;
        variants.push(value);
        let mut value = base.clone();
        value.parameter_digest = Sha256Digest::of_bytes(b"other parameters");
        variants.push(value);
        for variant in variants {
            assert_ne!(base.digest().unwrap(), variant.digest().unwrap());
        }
    }

    #[test]
    fn corruption_is_detected_and_uncacheable_stages_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let cache = StageCache::open(directory.path()).unwrap();
        let key = key(BTreeMap::new());
        cache
            .put(&key, &Cacheability::Deterministic, "text/plain", b"value")
            .unwrap();
        let value = directory
            .path()
            .join("entries")
            .join(key.digest().unwrap().to_string())
            .join("value.bin");
        fs::write(value, b"tampered").unwrap();
        assert!(matches!(cache.get(&key), Err(CacheError::Corrupt { .. })));
        assert!(matches!(
            cache.put(
                &key,
                &Cacheability::Uncacheable {
                    reason: "random".into()
                },
                "text/plain",
                b"value"
            ),
            Err(CacheError::Uncacheable(_))
        ));
    }

    #[test]
    fn key_metadata_contains_no_observational_time() {
        let encoded = serde_json::to_string(&key(BTreeMap::new())).unwrap();
        assert!(!encoded.contains("timestamp"));
        assert!(!encoded.contains("created_at"));
        assert!(!encoded.contains("hostname"));
    }

    #[test]
    fn one_semantic_key_refuses_different_immutable_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let cache = StageCache::open(directory.path()).unwrap();
        let key = key(BTreeMap::new());
        cache
            .put(
                &key,
                &Cacheability::Deterministic,
                "application/json",
                b"first",
            )
            .unwrap();
        assert!(matches!(
            cache.put(
                &key,
                &Cacheability::Deterministic,
                "application/json",
                b"second"
            ),
            Err(CacheError::Corrupt { .. })
        ));
        assert!(matches!(
            cache.get_bounded(&key, 4),
            Err(CacheError::Corrupt { .. })
        ));
    }
}
