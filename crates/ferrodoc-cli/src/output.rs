use std::{
    fs,
    io::{self, Write},
    path::Path,
};

use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("write output: {0}")]
    Io(#[from] io::Error),
    #[error("atomically persist output to {path:?}: {source}")]
    Persist {
        path: std::path::PathBuf,
        source: tempfile::PersistError,
    },
}

pub fn write(bytes: &[u8], path: Option<&Path>) -> Result<(), OutputError> {
    match path {
        None => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(bytes)?;
            stdout.flush()?;
        }
        Some(path) => atomic_write(path, bytes)?,
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), OutputError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file_mut().sync_all()?;
    temporary
        .persist(path)
        .map_err(|source| OutputError::Persist {
            path: path.to_owned(),
            source,
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_output_atomically() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("output.md");
        fs::write(&path, b"old").unwrap();
        atomic_write(&path, b"new").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"new");
    }
}
