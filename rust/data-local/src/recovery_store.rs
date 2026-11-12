//! Durable record of uploads whose remote outcome is not yet reconciled.
//!
//! The store keeps the immutable snapshot and the original upload context so a
//! later process can reconcile by remote identity or checksum. It never stores
//! a resumable-session URL: that URL is a bearer capability and no secret store
//! is used, so recovery after a restart reconciles by identity or by name and
//! checksum and never resends a write.

use crate::{
    UploadSnapshot, file_sha256,
    storage::{absolute, atomic_write, restrict_directory, restrict_file},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const FORMAT_VERSION: u32 = 1;
const MANIFEST: &str = "manifest.json";
const SNAPSHOT: &str = "snapshot";
const LOCK: &str = ".lock";
const ENTRY_PREFIX: &str = "op-";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

/// Remote state observed before the write, kept as text to avoid lossy JSON numbers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedRemote {
    pub id: String,
    pub modified_unix_nanos: String,
    pub sha256: Option<String>,
    pub version: Option<String>,
}

/// Inputs to reconcile an upload without repeating it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryRecord {
    /// Remote file identity when the server reported one; otherwise reconcile by
    /// folder, name and checksum.
    pub remote_id: Option<String>,
    pub local: String,
    pub folder: String,
    pub remote_name: String,
    pub observed: Option<ObservedRemote>,
    pub digest: String,
    /// Fingerprint of the connection that authorized the write. OAuth revisions are
    /// per-process counters, so recovery compares this instead and never reuses
    /// another connection.
    pub connection: String,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    #[serde(flatten)]
    record: RecoveryRecord,
}

/// Why an entry cannot be used for automatic reconciliation. It is kept untouched.
#[derive(Debug, PartialEq, Eq)]
pub enum Unverifiable {
    UnreadableManifest,
    UnknownVersion(u32),
    SnapshotMissing,
    DigestMismatch,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecoveryState {
    Verified {
        record: Box<RecoveryRecord>,
        snapshot: PathBuf,
    },
    Unverifiable(Unverifiable),
}

#[derive(Debug)]
pub struct RecoveryEntry {
    pub id: String,
    pub state: RecoveryState,
}

/// Directory next to the configuration file holding unresolved uploads.
pub struct RecoveryStore {
    root: PathBuf,
}

/// Exclusive cross-process lock; released when dropped.
struct DirectoryLock(fs::File);
impl Drop for DirectoryLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

impl RecoveryStore {
    pub fn new(config: &Path) -> io::Result<Self> {
        let config = absolute(config)?;
        let name = config
            .file_name()
            .ok_or_else(|| io::Error::other("configuration has no filename"))?;
        let mut directory = name.to_os_string();
        directory.push(".sync-recovery");
        Ok(Self {
            root: config.with_file_name(directory),
        })
    }

    fn ensure_root(&self) -> io::Result<()> {
        match fs::symlink_metadata(&self.root) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::other("recovery directory is a symbolic link"));
            }
            Ok(meta) if !meta.file_type().is_dir() => {
                return Err(io::Error::other("recovery path is not a directory"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(&self.root)?;
                let meta = fs::symlink_metadata(&self.root)?;
                if !meta.file_type().is_dir() || meta.file_type().is_symlink() {
                    return Err(io::Error::other("recovery directory is a symbolic link"));
                }
            }
            Err(e) => return Err(e),
        }
        restrict_directory(&self.root)
    }

    fn lock(&self) -> io::Result<DirectoryLock> {
        self.ensure_root()?;
        let lock_path = self.root.join(LOCK);
        match fs::symlink_metadata(&lock_path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::other("recovery lock is a symbolic link"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path)?;
        file.lock()?;
        Ok(DirectoryLock(file))
    }

    /// Persist the snapshot and manifest. Call before sending the write, and
    /// keep the entry until history records the reconciled upload.
    pub fn stage(&self, snapshot: &UploadSnapshot, record: RecoveryRecord) -> io::Result<String> {
        if record.digest != snapshot.digest() {
            return Err(io::Error::other("recovery record does not match snapshot"));
        }
        self.stage_file(snapshot.path(), record)
    }

    /// As `stage`, from the snapshot path. The copy must hash to `record.digest`.
    pub fn stage_file(&self, snapshot: &Path, record: RecoveryRecord) -> io::Result<String> {
        let _guard = self.lock()?;
        let directory = tempfile::Builder::new()
            .prefix(ENTRY_PREFIX)
            .tempdir_in(&self.root)?
            .keep();
        let result = (|| {
            restrict_directory(&directory)?;
            let target = directory.join(SNAPSHOT);
            let mut source = fs::File::open(snapshot)?;
            let mut copy = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            restrict_file(&target)?;
            io::copy(&mut source, &mut copy)?;
            copy.flush()?;
            copy.sync_all()?;
            if file_sha256(&target)? != record.digest {
                return Err(io::Error::other("snapshot changed while staging recovery"));
            }
            let bytes = serde_json::to_vec(&Manifest {
                version: FORMAT_VERSION,
                record,
            })
            .map_err(io::Error::other)?;
            atomic_write(&directory.join(MANIFEST), &bytes)
        })();
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&directory);
            return Err(error);
        }
        let id = directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| io::Error::other("invalid recovery entry name"))?
            .to_owned();
        Ok(id)
    }

    /// List every entry. Entries that cannot be verified are reported, never repaired or removed.
    pub fn list(&self) -> io::Result<Vec<RecoveryEntry>> {
        match fs::symlink_metadata(&self.root) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::other("recovery directory is a symbolic link"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        }
        let _guard = self.lock()?;
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.root)? {
            let item = item?;
            let Ok(id) = item.file_name().into_string() else {
                continue;
            };
            if !valid_id(&id) || !item.file_type()?.is_dir() {
                continue;
            }
            entries.push(RecoveryEntry {
                state: Self::inspect(&item.path()),
                id,
            });
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(entries)
    }

    fn inspect(directory: &Path) -> RecoveryState {
        let manifest = match read_regular(&directory.join(MANIFEST), MAX_MANIFEST_BYTES) {
            Ok(bytes) => bytes,
            Err(_) => return RecoveryState::Unverifiable(Unverifiable::UnreadableManifest),
        };
        #[derive(Deserialize)]
        struct Version {
            version: u32,
        }
        match serde_json::from_slice::<Version>(&manifest) {
            Ok(Version { version }) if version != FORMAT_VERSION => {
                return RecoveryState::Unverifiable(Unverifiable::UnknownVersion(version));
            }
            Ok(_) => {}
            Err(_) => return RecoveryState::Unverifiable(Unverifiable::UnreadableManifest),
        }
        let Ok(Manifest { record, .. }) = serde_json::from_slice::<Manifest>(&manifest) else {
            return RecoveryState::Unverifiable(Unverifiable::UnreadableManifest);
        };
        let snapshot = directory.join(SNAPSHOT);
        match fs::symlink_metadata(&snapshot) {
            Ok(meta) if meta.file_type().is_file() => {}
            _ => return RecoveryState::Unverifiable(Unverifiable::SnapshotMissing),
        }
        match file_sha256(&snapshot) {
            Ok(digest) if digest == record.digest => RecoveryState::Verified {
                record: Box::new(record),
                snapshot,
            },
            Ok(_) => RecoveryState::Unverifiable(Unverifiable::DigestMismatch),
            Err(_) => RecoveryState::Unverifiable(Unverifiable::SnapshotMissing),
        }
    }

    /// Whether a verified, unresolved entry exists for this save. A new write must
    /// wait for its recovery so the remote outcome is never overwritten blindly.
    pub fn has_unresolved(
        &self,
        local: &Path,
        folder: &str,
        remote_name: &str,
    ) -> io::Result<bool> {
        let local = crate::path_identity::local_identity(local)?;
        for entry in self.list()? {
            if let RecoveryState::Verified { record, .. } = entry.state
                && record.folder == folder
                && record.remote_name == remote_name
                // A save reachable under another spelling is the same save: a case-only
                // difference must not let a new write overtake an uncertain upload.
                && crate::path_identity::local_identity(Path::new(&record.local))? == local
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Remove an entry after it was reconciled and recorded, or when the user
    /// explicitly discards it. Never called implicitly for unresolved entries.
    pub fn remove(&self, id: &str) -> io::Result<()> {
        if !valid_id(id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid recovery entry identifier",
            ));
        }
        let _guard = self.lock()?;
        let directory = self.root.join(id);
        match fs::symlink_metadata(&directory) {
            Ok(meta) if meta.file_type().is_dir() => fs::remove_dir_all(directory),
            Ok(_) => Err(io::Error::other("recovery entry is not a directory")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn valid_id(id: &str) -> bool {
    id.starts_with(ENTRY_PREFIX)
        && id.len() > ENTRY_PREFIX.len()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn read_regular(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(io::Error::other("not a regular file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("file exceeds the permitted size"));
    }
    Ok(bytes)
}
