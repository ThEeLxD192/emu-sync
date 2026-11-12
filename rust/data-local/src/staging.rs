use crate::{SyncHistoryStore, file_sha256, storage::absolute};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Evidence referring to a retained upload snapshot. The snapshot owner must outlive its use.
#[derive(Clone, Debug)]
pub struct UploadEvidence {
    pub(crate) path: PathBuf,
    pub(crate) digest: String,
}

impl UploadEvidence {
    /// Evidence for a snapshot retained by a persistent recovery entry. The caller
    /// passes only a path and digest verified by `RecoveryStore::list`.
    pub fn recovered(path: PathBuf, digest: String) -> Self {
        Self { path, digest }
    }
}

/// Stable upload copy; the temporary directory removes itself when it is dropped.
#[derive(Debug)]
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
    pub fn evidence(&self) -> UploadEvidence {
        UploadEvidence {
            path: self.path.clone(),
            digest: self.digest.clone(),
        }
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
    logical_destination: PathBuf,
    original_digest: Option<String>,
    #[cfg(any(target_os = "linux", windows))]
    pinned: Option<crate::PinnedRemoteDestination>,
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
            logical_destination: destination.clone(),
            destination,
            original_digest,
            #[cfg(any(target_os = "linux", windows))]
            pinned: None,
        })
    }
    /// Prepare under a pinned root, creating missing descendant directories without following links.
    #[cfg(any(target_os = "linux", windows))]
    pub fn prepare_pinned(root: &Path, remote_name: &str) -> io::Result<Option<Self>> {
        let Some(pinned) = crate::PinnedRemoteDestination::open_or_create(root, remote_name)?
        else {
            return Ok(None);
        };
        let destination = pinned.destination_path();
        let original_digest = pinned
            .open_existing()?
            .map(|file| digest_open_file(&file))
            .transpose()?;
        let temp = tempfile::Builder::new()
            .prefix(".emusync.")
            .suffix(".restore")
            .tempfile_in(pinned.parent_path())?;
        let logical_destination = absolute(pinned.logical_path())?;
        Ok(Some(Self {
            temp,
            destination,
            logical_destination,
            original_digest,
            pinned: Some(pinned),
        }))
    }
    pub fn path(&self) -> &Path {
        self.temp.path()
    }
    pub fn logical_destination(&self) -> &Path {
        &self.logical_destination
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
        self.temp.as_file().sync_all()?;
        #[cfg(any(target_os = "linux", windows))]
        if let Some(pinned) = &self.pinned {
            pinned.ensure_current()?;
            let current = pinned.open_existing()?;
            let current_digest = current.as_ref().map(digest_open_file).transpose()?;
            if current_digest != self.original_digest {
                return Err(io::Error::other("save changed during download"));
            }
            if let (Some(old), Some(file)) = (&self.original_digest, current.as_ref())
                && old != &digest
            {
                let source = pinned.source_path(file);
                history.backup_from(key, &source, pinned.logical_path(), old)?;
            }
            // Release the handle before replacing the file it refers to.
            drop(current);
            if self.original_digest.is_none() {
                self.temp
                    .persist_noclobber(&self.destination)
                    .map_err(|e| e.error)?;
            } else {
                self.temp.persist(&self.destination).map_err(|e| e.error)?;
            }
            pinned.ensure_current()?;
            return Ok(digest);
        }
        if digest_if_file(&self.destination)? != self.original_digest {
            return Err(io::Error::other("save changed during download"));
        }
        if let Some(old) = &self.original_digest
            && old != &digest
        {
            history.backup(key, &self.destination, old)?;
        }
        if self.original_digest.is_none() {
            self.temp
                .persist_noclobber(&self.destination)
                .map_err(|e| e.error)?;
        } else {
            self.temp.persist(&self.destination).map_err(|e| e.error)?;
        }
        Ok(digest)
    }
}
pub(crate) fn digest_if_file(path: &Path) -> io::Result<Option<String>> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(file_sha256(path)?)),
        Ok(_) => Err(io::Error::other("save is not a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "linux")]
fn digest_open_file(file: &fs::File) -> io::Result<String> {
    use std::os::fd::AsRawFd;
    file_sha256(Path::new(&format!("/proc/self/fd/{}", file.as_raw_fd())))
}

#[cfg(windows)]
fn digest_open_file(file: &fs::File) -> io::Result<String> {
    let mut handle = file;
    crate::history::reader_sha256(&mut handle)
}
