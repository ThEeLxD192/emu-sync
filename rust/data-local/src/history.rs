use crate::storage::{absolute, atomic_write, lock_for, restrict_directory, restrict_file};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCheckpoint {
    pub local_sha256: String,
    pub remote_id: String,
    pub remote_modified_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_version: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct History {
    #[serde(default = "version_one", skip_serializing_if = "is_version_one")]
    format_version: u32,
    #[serde(default)]
    saves: BTreeMap<String, HistoryCheckpoint>,
}
fn version_one() -> u32 {
    1
}
fn is_version_one(version: &u32) -> bool {
    *version == 1
}

/// Synchronization evidence and recovery copies, serialized across instances in this process.
#[derive(Clone)]
pub struct SyncHistoryStore {
    state_file: PathBuf,
    backup_directory: PathBuf,
    lock: Arc<Mutex<()>>,
}
impl SyncHistoryStore {
    pub fn new(config: &Path) -> io::Result<Self> {
        let config = absolute(config)?;
        let name = config
            .file_name()
            .ok_or_else(|| io::Error::other("configuration has no filename"))?;
        let mut state = name.to_os_string();
        state.push(".sync-state.json");
        let mut backups = name.to_os_string();
        backups.push(".sync-backups");
        let state_file = config.with_file_name(state);
        let backup_directory = config.with_file_name(backups);
        let lock = lock_for(&state_file)?;
        Ok(Self {
            state_file,
            backup_directory,
            lock,
        })
    }
    pub fn state_file(&self) -> &Path {
        &self.state_file
    }
    pub fn backup_directory(&self) -> &Path {
        &self.backup_directory
    }
    pub fn key(&self, file: &Path, folder_id: &str, remote_name: &str) -> io::Result<String> {
        let path = absolute(file)?;
        let mut hash = Sha256::new();
        hash.update(path.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(folder_id.as_bytes());
        hash.update([0]);
        hash.update(remote_name.as_bytes());
        Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }
    pub fn get(&self, key: &str) -> io::Result<Option<HistoryCheckpoint>> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| io::Error::other("history lock poisoned"))?;
        Ok(self.read()?.saves.remove(key))
    }
    pub fn record(&self, key: &str, checkpoint: HistoryCheckpoint) -> io::Result<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| io::Error::other("history lock poisoned"))?;
        let mut history = self.read()?;
        if history.saves.get(key) != Some(&checkpoint) {
            history.saves.insert(key.to_owned(), checkpoint);
            atomic_write(&self.state_file, &serde_json::to_vec_pretty(&history)?)?;
        }
        Ok(())
    }
    fn read(&self) -> io::Result<History> {
        let contents = match fs::read(&self.state_file) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(History {
                    format_version: 1,
                    saves: BTreeMap::new(),
                });
            }
            Err(error) => return Err(error),
        };
        let history: History = serde_json::from_slice(&contents)?;
        if history.format_version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported synchronization history format",
            ));
        }
        Ok(history)
    }
    /// Preserve a verified copy before replacement. `key` and `digest` must be SHA-256 hex.
    pub fn backup(&self, key: &str, file: &Path, digest: &str) -> io::Result<()> {
        self.backup_from(key, file, file, digest)
    }
    pub(crate) fn backup_from(
        &self,
        key: &str,
        file: &Path,
        original: &Path,
        digest: &str,
    ) -> io::Result<()> {
        if !valid_digest(key) || !valid_digest(digest) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid backup key or digest",
            ));
        }
        let _guard = self
            .lock
            .lock()
            .map_err(|_| io::Error::other("history lock poisoned"))?;
        let location = self.backup_directory.join(key);
        let version = location.join(digest);
        fs::create_dir_all(&version)?;
        restrict_directory(&version)?;
        let destination = version.join(
            original
                .file_name()
                .ok_or_else(|| io::Error::other("save has no filename"))?,
        );
        if destination.exists() {
            if file_sha256(&destination)? != digest {
                return Err(io::Error::other("existing recovery copy was modified"));
            }
        } else {
            let temp = tempfile::Builder::new()
                .prefix(".save.")
                .suffix(".tmp")
                .tempfile_in(&version)?;
            fs::copy(file, temp.path())?;
            restrict_file(temp.path())?;
            if file_sha256(temp.path())? != digest {
                return Err(io::Error::other("save changed while creating backup"));
            }
            temp.as_file().sync_all()?;
            temp.persist_noclobber(&destination).map_err(|e| e.error)?;
        }
        let original = if original.is_absolute() {
            original.to_owned()
        } else {
            std::env::current_dir()?.join(original)
        };
        let source = serde_json::json!({"originalPath": original.to_string_lossy()});
        atomic_write(
            &location.join("source.json"),
            &serde_json::to_vec_pretty(&source)?,
        )
    }
}
fn valid_digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn file_sha256(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    reader_sha256(&mut file)
}
pub(crate) fn reader_sha256(file: &mut impl Read) -> io::Result<String> {
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
