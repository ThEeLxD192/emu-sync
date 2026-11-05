use crate::{SyncHistoryStore, file_sha256, storage::absolute};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Stable upload copy; the temporary directory removes itself when it is dropped.
pub struct UploadSnapshot {
    directory: tempfile::TempDir,
    path: PathBuf,
    digest: String,
}
impl UploadSnapshot {
    pub fn prepare(source: &Path) -> io::Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("emusync-save-upload-")
            .tempdir()?;
        let path = directory.path().join(
            source
                .file_name()
                .ok_or_else(|| io::Error::other("save has no filename"))?,
        );
        fs::copy(source, &path)?;
        let digest = file_sha256(&path)?;
        if file_sha256(source)? != digest {
            return Err(io::Error::other("save changed while preparing upload"));
        }
        Ok(Self {
            directory,
            path,
            digest,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn verify_uploaded(&self, checksum: Option<&str>, source: &Path) -> io::Result<()> {
        if checksum.is_some_and(|value| !value.eq_ignore_ascii_case(&self.digest)) {
            return Err(io::Error::other("uploaded content could not be verified"));
        }
        if file_sha256(source)? != self.digest {
            return Err(io::Error::other("save changed during upload"));
        }
        Ok(())
    }
    pub fn directory(&self) -> &Path {
        self.directory.path()
    }
}

/// Restore staging in the same directory as its destination, so replacement is atomic.
pub struct StagedRestore {
    temp: tempfile::NamedTempFile,
    destination: PathBuf,
    original_digest: Option<String>,
}
impl StagedRestore {
    pub fn prepare(destination: &Path) -> io::Result<Self> {
        let destination = absolute(destination)?;
        let parent = destination
            .parent()
            .ok_or_else(|| io::Error::other("save has no parent"))?;
        fs::create_dir_all(parent)?;
        let original_digest = digest_if_file(&destination)?;
        let temp = tempfile::Builder::new()
            .prefix(".emusync.")
            .suffix(".restore")
            .tempfile_in(parent)?;
        Ok(Self {
            temp,
            destination,
            original_digest,
        })
    }
    pub fn path(&self) -> &Path {
        self.temp.path()
    }
    pub fn commit(
        self,
        expected_remote_hash: Option<&str>,
        history: &SyncHistoryStore,
        key: &str,
    ) -> io::Result<String> {
        let digest = file_sha256(self.temp.path())?;
        if expected_remote_hash.is_some_and(|expected| !expected.eq_ignore_ascii_case(&digest)) {
            return Err(io::Error::other("downloaded content could not be verified"));
        }
        if digest_if_file(&self.destination)? != self.original_digest {
            return Err(io::Error::other("save changed during download"));
        }
        if let Some(old) = &self.original_digest
            && old != &digest
        {
            history.backup(key, &self.destination, old)?;
        }
        self.temp.persist(&self.destination).map_err(|e| e.error)?;
        Ok(digest)
    }
}
fn digest_if_file(path: &Path) -> io::Result<Option<String>> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(file_sha256(path)?)),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
