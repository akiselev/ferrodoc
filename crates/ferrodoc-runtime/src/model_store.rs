//! Atomic, content-addressed model storage.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use ferrodoc_core::{ModelId, ModelManifest, RelativePath, Sha256Digest};
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

const MANIFEST_FILE: &str = "manifest.json";
const PAYLOAD_DIR: &str = "payload";

/// A visible, fully verified logical model view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledModel {
    /// Stable model ID.
    pub id: ModelId,
    /// Source revision from the manifest.
    pub revision: String,
    /// Digest of the canonical manifest bytes.
    pub manifest_digest: Sha256Digest,
    /// Verified view directory.
    pub path: PathBuf,
}

/// Garbage-collection outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GcReport {
    /// Number of removed unreferenced blobs.
    pub removed_blobs: u64,
    /// Bytes reclaimed from removed blobs.
    pub reclaimed_bytes: u64,
}

/// Model-store failure.
#[derive(Debug, Error)]
pub enum ModelStoreError {
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
    /// Manifest encoding is invalid.
    #[error("invalid model manifest: {0}")]
    Manifest(#[from] serde_json::Error),
    /// Installation requires explicit terms acceptance.
    #[error("model {0} requires explicit license or usage-term acceptance")]
    AcceptanceRequired(ModelId),
    /// A required source file is missing, escaped its source root, or is not regular.
    #[error("invalid model source for {logical_path}: {reason}")]
    InvalidSource {
        /// Manifest logical path.
        logical_path: RelativePath,
        /// Stable explanation.
        reason: String,
    },
    /// Source or stored content did not match its manifest.
    #[error("model file {logical_path} failed verification: {reason}")]
    Verification {
        /// Manifest logical path.
        logical_path: RelativePath,
        /// Stable explanation.
        reason: String,
    },
    /// Existing immutable store state is corrupt.
    #[error("model store corruption at {path:?}: {reason}")]
    Corrupt {
        /// Corrupt path.
        path: PathBuf,
        /// Stable explanation.
        reason: String,
    },
}

/// Content-addressed model store rooted at a caller-selected directory.
#[derive(Debug, Clone)]
pub struct ModelStore {
    root: PathBuf,
}

impl ModelStore {
    /// Opens a store and creates its private structural directories.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, ModelStoreError> {
        let store = Self { root: root.into() };
        for path in [
            store.blobs_dir(),
            store.views_dir(),
            store.staging_dir(),
            store.leases_dir(),
        ] {
            create_dir_all(&path)?;
        }
        Ok(store)
    }

    /// Returns the store root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Installs a manifest from already acquired bytes.
    ///
    /// No logical view is returned or discoverable until every file has been
    /// checked and the staging directory has been atomically renamed.
    pub fn install_bytes(
        &self,
        manifest: &ModelManifest,
        sources: &BTreeMap<RelativePath, Vec<u8>>,
        accepted: bool,
    ) -> Result<InstalledModel, ModelStoreError> {
        if manifest.acceptance().is_some() && !accepted {
            return Err(ModelStoreError::AcceptanceRequired(manifest.id().clone()));
        }
        for file in manifest.files() {
            let bytes = sources
                .get(file.path())
                .ok_or_else(|| ModelStoreError::InvalidSource {
                    logical_path: file.path().clone(),
                    reason: "file is absent".into(),
                })?;
            verify_file(file.path(), file.bytes().get(), file.digest(), bytes)?;
        }
        let manifest_bytes = serde_json::to_vec_pretty(manifest)?;
        let manifest_digest = Sha256Digest::of_bytes(&manifest_bytes);
        let final_dir = self
            .views_dir()
            .join(manifest.id().as_str())
            .join(manifest_digest.to_string());
        if final_dir.is_dir() {
            let installed = self.verify_view(&final_dir)?;
            if installed.revision != manifest.revision() {
                return Err(ModelStoreError::Corrupt {
                    path: final_dir,
                    reason: "visible view manifest differs from requested revision".into(),
                });
            }
            return Ok(installed);
        }

        for file in manifest.files() {
            self.install_blob(
                file.digest(),
                sources.get(file.path()).expect("checked above"),
            )?;
        }

        let staging = self.staging_dir().join(Uuid::new_v4().simple().to_string());
        create_dir_all(&staging.join(PAYLOAD_DIR))?;
        let result = (|| {
            for file in manifest.files() {
                let destination = staging
                    .join(PAYLOAD_DIR)
                    .join(path_from_relative(file.path()));
                if let Some(parent) = destination.parent() {
                    create_dir_all(parent)?;
                }
                let blob = self.blob_path(file.digest());
                fs::hard_link(&blob, &destination)
                    .or_else(|_| fs::copy(&blob, &destination).map(|_| ()))
                    .map_err(|source| io_error("materialize model file", &destination, source))?;
            }
            atomic_write(&staging.join(MANIFEST_FILE), &manifest_bytes)?;
            self.verify_view(&staging)?;
            let parent = final_dir.parent().expect("view has model parent");
            create_dir_all(parent)?;
            match fs::rename(&staging, &final_dir) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    fs::remove_dir_all(&staging).map_err(|source| {
                        io_error("remove duplicate staging view", &staging, source)
                    })?;
                }
                Err(source) => return Err(io_error("publish model view", &final_dir, source)),
            }
            self.verify_view(&final_dir)
        })();
        if result.is_err() && staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }

    /// Acquires files from a local directory after rejecting symlink escapes.
    pub fn install_from_directory(
        &self,
        manifest: &ModelManifest,
        source_root: &Path,
        accepted: bool,
    ) -> Result<InstalledModel, ModelStoreError> {
        let canonical_root = fs::canonicalize(source_root)
            .map_err(|source| io_error("resolve model source root", source_root, source))?;
        let mut sources = BTreeMap::new();
        for file in manifest.files() {
            let source = source_root.join(path_from_relative(file.path()));
            let canonical =
                fs::canonicalize(&source).map_err(|error| ModelStoreError::InvalidSource {
                    logical_path: file.path().clone(),
                    reason: format!("cannot resolve file: {error}"),
                })?;
            let metadata =
                fs::metadata(&canonical).map_err(|error| ModelStoreError::InvalidSource {
                    logical_path: file.path().clone(),
                    reason: format!("cannot inspect file: {error}"),
                })?;
            if !canonical.starts_with(&canonical_root) || !metadata.is_file() {
                return Err(ModelStoreError::InvalidSource {
                    logical_path: file.path().clone(),
                    reason: "resolved path escapes source root or is not a regular file".into(),
                });
            }
            let bytes = fs::read(&canonical)
                .map_err(|source| io_error("read model source", &canonical, source))?;
            sources.insert(file.path().clone(), bytes);
        }
        self.install_bytes(manifest, &sources, accepted)
    }

    /// Lists only fully published, verified model views.
    pub fn list(&self) -> Result<Vec<InstalledModel>, ModelStoreError> {
        let mut installed = Vec::new();
        for model_dir in read_dirs(&self.views_dir())? {
            if !model_dir
                .file_type()
                .map_err(|source| io_error("inspect model directory", &model_dir.path(), source))?
                .is_dir()
            {
                continue;
            }
            for view in read_dirs(&model_dir.path())? {
                if view
                    .file_type()
                    .map_err(|source| io_error("inspect model view", &view.path(), source))?
                    .is_dir()
                {
                    installed.push(self.verify_view(&view.path())?);
                }
            }
        }
        installed.sort_by(|left, right| {
            (
                left.id.as_str(),
                left.revision.as_str(),
                left.manifest_digest,
            )
                .cmp(&(
                    right.id.as_str(),
                    right.revision.as_str(),
                    right.manifest_digest,
                ))
        });
        Ok(installed)
    }

    /// Verifies every visible view and referenced blob without network access.
    pub fn verify_all(&self) -> Result<Vec<InstalledModel>, ModelStoreError> {
        self.list()
    }

    /// Returns a verified logical view matching an ID and revision.
    pub fn logical_view(
        &self,
        id: &ModelId,
        revision: &str,
    ) -> Result<Option<InstalledModel>, ModelStoreError> {
        Ok(self
            .list()?
            .into_iter()
            .find(|model| &model.id == id && model.revision == revision))
    }

    /// Creates a filesystem lease which keeps one blob live during garbage collection.
    pub fn lease_blob(&self, digest: Sha256Digest) -> Result<ModelLease, ModelStoreError> {
        let directory = self.leases_dir().join(digest.to_string());
        create_dir_all(&directory)?;
        let path = directory.join(Uuid::new_v4().simple().to_string());
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| io_error("create model lease", &path, source))?;
        Ok(ModelLease { path })
    }

    /// Removes unreferenced immutable blobs, retaining all visible manifests and active leases.
    pub fn garbage_collect(&self) -> Result<GcReport, ModelStoreError> {
        let mut live = BTreeSet::new();
        for installed in self.list()? {
            let manifest = read_manifest(&installed.path.join(MANIFEST_FILE))?;
            live.extend(manifest.files().iter().map(|file| file.digest()));
        }
        for directory in read_dirs(&self.leases_dir())? {
            if directory
                .file_type()
                .map_err(|source| io_error("inspect lease directory", &directory.path(), source))?
                .is_dir()
                && !read_dirs(&directory.path())?.is_empty()
                && let Ok(digest) = directory.file_name().to_string_lossy().parse()
            {
                live.insert(digest);
            }
        }
        let mut report = GcReport {
            removed_blobs: 0,
            reclaimed_bytes: 0,
        };
        for entry in read_dirs(&self.blobs_dir())? {
            let path = entry.path();
            if !entry
                .file_type()
                .map_err(|source| io_error("inspect model blob", &path, source))?
                .is_file()
            {
                continue;
            }
            let Ok(digest) = entry.file_name().to_string_lossy().parse::<Sha256Digest>() else {
                return Err(ModelStoreError::Corrupt {
                    path,
                    reason: "blob name is not a digest".into(),
                });
            };
            if !live.contains(&digest) {
                let bytes = entry
                    .metadata()
                    .map_err(|source| io_error("inspect model blob", &path, source))?
                    .len();
                fs::remove_file(&path)
                    .map_err(|source| io_error("remove model blob", &path, source))?;
                report.removed_blobs += 1;
                report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(bytes);
            }
        }
        Ok(report)
    }

    fn install_blob(&self, digest: Sha256Digest, bytes: &[u8]) -> Result<(), ModelStoreError> {
        let path = self.blob_path(digest);
        if path.exists() {
            return verify_blob(&path, digest);
        }
        atomic_write(&path, bytes)?;
        verify_blob(&path, digest)
    }

    fn verify_view(&self, path: &Path) -> Result<InstalledModel, ModelStoreError> {
        let manifest = read_manifest(&path.join(MANIFEST_FILE))?;
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        let manifest_digest = Sha256Digest::of_bytes(&manifest_bytes);
        for file in manifest.files() {
            let logical = path.join(PAYLOAD_DIR).join(path_from_relative(file.path()));
            let metadata = fs::symlink_metadata(&logical)
                .map_err(|source| io_error("inspect logical model file", &logical, source))?;
            if !metadata.file_type().is_file() {
                return Err(ModelStoreError::Corrupt {
                    path: logical,
                    reason: "logical model entry is not a regular file".into(),
                });
            }
            let bytes = fs::read(&logical)
                .map_err(|source| io_error("read logical model file", &logical, source))?;
            verify_file(file.path(), file.bytes().get(), file.digest(), &bytes)?;
            verify_blob(&self.blob_path(file.digest()), file.digest())?;
        }
        Ok(InstalledModel {
            id: manifest.id().clone(),
            revision: manifest.revision().into(),
            manifest_digest,
            path: path.to_owned(),
        })
    }

    fn blobs_dir(&self) -> PathBuf {
        self.root.join("blobs")
    }
    fn views_dir(&self) -> PathBuf {
        self.root.join("views")
    }
    fn staging_dir(&self) -> PathBuf {
        self.root.join("staging")
    }
    fn leases_dir(&self) -> PathBuf {
        self.root.join("leases")
    }
    fn blob_path(&self, digest: Sha256Digest) -> PathBuf {
        self.blobs_dir().join(digest.to_string())
    }
}

/// Active blob lease removed on drop.
#[derive(Debug)]
pub struct ModelLease {
    path: PathBuf,
}

impl Drop for ModelLease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        if let Some(parent) = self.path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
}

fn verify_file(
    path: &RelativePath,
    expected_bytes: u64,
    expected_digest: Sha256Digest,
    bytes: &[u8],
) -> Result<(), ModelStoreError> {
    if bytes.len() as u64 != expected_bytes {
        return Err(ModelStoreError::Verification {
            logical_path: path.clone(),
            reason: format!("expected {expected_bytes} bytes, found {}", bytes.len()),
        });
    }
    let actual = Sha256Digest::of_bytes(bytes);
    if actual != expected_digest {
        return Err(ModelStoreError::Verification {
            logical_path: path.clone(),
            reason: format!("expected digest {expected_digest}, found {actual}"),
        });
    }
    Ok(())
}

fn verify_blob(path: &Path, digest: Sha256Digest) -> Result<(), ModelStoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| io_error("inspect model blob", path, source))?;
    if !metadata.file_type().is_file() {
        return Err(ModelStoreError::Corrupt {
            path: path.to_owned(),
            reason: "blob is not a regular file".into(),
        });
    }
    let bytes = fs::read(path).map_err(|source| io_error("read model blob", path, source))?;
    let actual = Sha256Digest::of_bytes(&bytes);
    if actual != digest {
        return Err(ModelStoreError::Corrupt {
            path: path.to_owned(),
            reason: format!("filename digest {digest} differs from content digest {actual}"),
        });
    }
    Ok(())
}

fn read_manifest(path: &Path) -> Result<ModelManifest, ModelStoreError> {
    let bytes = fs::read(path).map_err(|source| io_error("read model manifest", path, source))?;
    serde_json::from_slice(&bytes).map_err(Into::into)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ModelStoreError> {
    let parent = path.parent().expect("store files have parents");
    create_dir_all(parent)?;
    let temporary = parent.join(format!(".tmp-{}", Uuid::new_v4().simple()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|source| io_error("create temporary model file", &temporary, source))?;
    let result = (|| {
        file.write_all(bytes)
            .map_err(|source| io_error("write temporary model file", &temporary, source))?;
        file.sync_all()
            .map_err(|source| io_error("sync temporary model file", &temporary, source))?;
        match fs::rename(&temporary, path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                fs::remove_file(&temporary).map_err(|source| {
                    io_error("remove duplicate temporary file", &temporary, source)
                })
            }
            Err(source) => Err(io_error("publish model file", path, source)),
        }
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn path_from_relative(path: &RelativePath) -> PathBuf {
    path.as_str().split('/').collect()
}

fn create_dir_all(path: &Path) -> Result<(), ModelStoreError> {
    fs::create_dir_all(path)
        .map_err(|source| io_error("create model-store directory", path, source))
}

fn read_dirs(path: &Path) -> Result<Vec<fs::DirEntry>, ModelStoreError> {
    fs::read_dir(path)
        .map_err(|source| io_error("read model-store directory", path, source))?
        .map(|entry| {
            entry.map_err(|source| io_error("read model-store directory entry", path, source))
        })
        .collect()
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> ModelStoreError {
    ModelStoreError::Io {
        operation,
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use ferrodoc_core::{
        AcceptanceRequirement, Bytes, CURRENT_SCHEMA_VERSION, LicenseMetadata, MediaType, ModelFile,
    };

    use super::*;

    fn fixture_manifest(bytes: &[u8], acceptance: bool) -> ModelManifest {
        ModelManifest::new(
            CURRENT_SCHEMA_VERSION,
            ModelId::derive(&[b"fixture"]),
            "source/revision:1",
            vec![
                ModelFile::new(
                    RelativePath::new("weights/model.bin").unwrap(),
                    Sha256Digest::of_bytes(bytes),
                    Bytes::new(bytes.len() as u64),
                    MediaType::new("application/octet-stream").unwrap(),
                )
                .unwrap(),
            ],
            LicenseMetadata {
                expression: "MIT".into(),
                source: "fixture".into(),
                notice: None,
            },
            acceptance.then(|| AcceptanceRequirement::License {
                prompt: "accept".into(),
            }),
        )
        .unwrap()
    }

    fn sources(bytes: &[u8]) -> BTreeMap<RelativePath, Vec<u8>> {
        BTreeMap::from([(
            RelativePath::new("weights/model.bin").unwrap(),
            bytes.to_vec(),
        )])
    }

    #[test]
    fn installation_is_invisible_until_complete_and_verifies_offline() {
        let directory = tempfile::tempdir().unwrap();
        let store = ModelStore::open(directory.path()).unwrap();
        let manifest = fixture_manifest(b"verified model", false);
        assert!(store.list().unwrap().is_empty());
        let installed = store
            .install_bytes(&manifest, &sources(b"verified model"), false)
            .unwrap();
        assert!(installed.path.join("payload/weights/model.bin").is_file());
        assert_eq!(store.verify_all().unwrap(), vec![installed]);
    }

    #[test]
    fn corrupt_and_unaccepted_models_never_become_visible() {
        let directory = tempfile::tempdir().unwrap();
        let store = ModelStore::open(directory.path()).unwrap();
        let manifest = fixture_manifest(b"correct", true);
        assert!(matches!(
            store.install_bytes(&manifest, &sources(b"correct"), false),
            Err(ModelStoreError::AcceptanceRequired(_))
        ));
        assert!(
            store
                .install_bytes(&manifest, &sources(b"wrong"), true)
                .is_err()
        );
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn one_bad_file_prevents_a_multi_file_view_from_becoming_visible() {
        let directory = tempfile::tempdir().unwrap();
        let store = ModelStore::open(directory.path()).unwrap();
        let first = b"first";
        let second = b"second";
        let manifest = ModelManifest::new(
            CURRENT_SCHEMA_VERSION,
            ModelId::derive(&[b"pair"]),
            "pair-revision",
            vec![
                ModelFile::new(
                    RelativePath::new("first.bin").unwrap(),
                    Sha256Digest::of_bytes(first),
                    Bytes::new(first.len() as u64),
                    MediaType::new("application/octet-stream").unwrap(),
                )
                .unwrap(),
                ModelFile::new(
                    RelativePath::new("second.bin").unwrap(),
                    Sha256Digest::of_bytes(second),
                    Bytes::new(second.len() as u64),
                    MediaType::new("application/octet-stream").unwrap(),
                )
                .unwrap(),
            ],
            LicenseMetadata {
                expression: "MIT".into(),
                source: "fixture".into(),
                notice: None,
            },
            None,
        )
        .unwrap();
        let sources = BTreeMap::from([
            (RelativePath::new("first.bin").unwrap(), first.to_vec()),
            (
                RelativePath::new("second.bin").unwrap(),
                b"partial".to_vec(),
            ),
        ]);
        assert!(store.install_bytes(&manifest, &sources, false).is_err());
        assert!(store.list().unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn source_symlink_cannot_escape_acquisition_root() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        fs::create_dir_all(source.join("weights")).unwrap();
        fs::write(directory.path().join("outside"), b"verified model").unwrap();
        symlink(
            directory.path().join("outside"),
            source.join("weights/model.bin"),
        )
        .unwrap();
        let store = ModelStore::open(directory.path().join("store")).unwrap();
        let error = store
            .install_from_directory(&fixture_manifest(b"verified model", false), &source, false)
            .unwrap_err();
        assert!(matches!(error, ModelStoreError::InvalidSource { .. }));
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn garbage_collection_honors_views_and_leases() {
        let directory = tempfile::tempdir().unwrap();
        let store = ModelStore::open(directory.path()).unwrap();
        let manifest = fixture_manifest(b"live", false);
        store
            .install_bytes(&manifest, &sources(b"live"), false)
            .unwrap();
        let orphan = Sha256Digest::of_bytes(b"orphan");
        store.install_blob(orphan, b"orphan").unwrap();
        let lease = store.lease_blob(orphan).unwrap();
        assert_eq!(store.garbage_collect().unwrap().removed_blobs, 0);
        drop(lease);
        assert_eq!(store.garbage_collect().unwrap().removed_blobs, 1);
        assert_eq!(store.verify_all().unwrap().len(), 1);
    }
}
