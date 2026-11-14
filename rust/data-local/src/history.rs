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
    /// The history as last read, so a synchronization does not parse the whole file for
    /// every save it checks. Only used while `lock` is held.
    cache: Arc<Mutex<Option<CachedHistory>>>,
}

/// A parsed history and the identity of the file it was read from.
struct CachedHistory {
    stamp: Option<FileStamp>,
    history: History,
}

/// Identifies one version of the state file. Every write replaces the file by renaming a
/// new one over it, which gives the path a new file identity (device and inode on Unix,
/// volume and file index on Windows), so a write by this or another process is never
/// mistaken for the version already read, even with the same size and modification time.
/// Windows creation times cannot serve: NTFS carries the old file's over to a file that
/// replaces it soon after.
#[derive(Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: u128,
    identity: (u64, u64),
}

impl FileStamp {
    #[cfg(unix)]
    fn of(file: &fs::File) -> io::Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        Ok(Self {
            len: metadata.len(),
            modified: (i128::from(metadata.mtime()) * 1_000_000_000
                + i128::from(metadata.mtime_nsec())) as u128,
            identity: (metadata.dev(), metadata.ino()),
        })
    }

    #[cfg(windows)]
    fn of(file: &fs::File) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::FILETIME,
            Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
        };
        let never = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut info = BY_HANDLE_FILE_INFORMATION {
            dwFileAttributes: 0,
            ftCreationTime: never,
            ftLastAccessTime: never,
            ftLastWriteTime: never,
            dwVolumeSerialNumber: 0,
            nFileSizeHigh: 0,
            nFileSizeLow: 0,
            nNumberOfLinks: 0,
            nFileIndexHigh: 0,
            nFileIndexLow: 0,
        };
        // SAFETY: the handle belongs to `file`, which stays open for the call, and `info`
        // is a valid, writable BY_HANDLE_FILE_INFORMATION the call fills in.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let wide = |high: u32, low: u32| (u64::from(high) << 32) | u64::from(low);
        Ok(Self {
            len: wide(info.nFileSizeHigh, info.nFileSizeLow),
            modified: u128::from(wide(
                info.ftLastWriteTime.dwHighDateTime,
                info.ftLastWriteTime.dwLowDateTime,
            )),
            identity: (
                u64::from(info.dwVolumeSerialNumber),
                wide(info.nFileIndexHigh, info.nFileIndexLow),
            ),
        })
    }
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
            cache: Arc::default(),
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
        Ok(bytes_to_hex(&hash.finalize()))
    }
    pub fn get(&self, key: &str) -> io::Result<Option<HistoryCheckpoint>> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| io::Error::other("history lock poisoned"))?;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        Ok(self.current(&mut cache)?.saves.get(key).cloned())
    }
    pub fn record(&self, key: &str, checkpoint: HistoryCheckpoint) -> io::Result<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| io::Error::other("history lock poisoned"))?;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if self.current(&mut cache)?.saves.get(key) != Some(&checkpoint) {
            // Taken out of the cache: the next read parses the file again, because the
            // stamp of the new file cannot be taken without racing another process that
            // replaces it meanwhile.
            let mut history = cache.take().expect("read above").history;
            history.saves.insert(key.to_owned(), checkpoint);
            atomic_write(&self.state_file, &serde_json::to_vec_pretty(&history)?)?;
        }
        Ok(())
    }
    /// The history on disk, parsed again only when the file changed since the last read.
    fn current<'a>(&self, cache: &'a mut Option<CachedHistory>) -> io::Result<&'a History> {
        let stamp = match fs::File::open(&self.state_file) {
            Ok(file) => Some(FileStamp::of(&file)?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if cache.as_ref().is_none_or(|cached| cached.stamp != stamp) {
            *cache = None;
            let read = self.read()?;
            *cache = Some(read);
        }
        Ok(&cache.as_ref().expect("filled above").history)
    }
    /// Reads the history and the stamp of the version read, both from one open file.
    fn read(&self) -> io::Result<CachedHistory> {
        let mut file = match fs::File::open(&self.state_file) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(CachedHistory {
                    stamp: None,
                    history: History {
                        format_version: 1,
                        saves: BTreeMap::new(),
                    },
                });
            }
            Err(error) => return Err(error),
        };
        let stamp = FileStamp::of(&file)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        let history: History = serde_json::from_slice(&contents)?;
        if history.format_version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported synchronization history format",
            ));
        }
        Ok(CachedHistory {
            stamp: Some(stamp),
            history,
        })
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
        restrict_directory(&self.backup_directory)?;
        restrict_directory(&location)?;
        restrict_directory(&version)?;
        let destination = version.join(
            original
                .file_name()
                .ok_or_else(|| io::Error::other("save has no filename"))?,
        );
        match fs::symlink_metadata(&destination) {
            Ok(meta) if meta.file_type().is_file() => {
                if !file_sha256(&destination)?.eq_ignore_ascii_case(digest) {
                    return Err(io::Error::other("existing recovery copy was modified"));
                }
            }
            Ok(_) => {
                return Err(io::Error::other(
                    "existing recovery copy is not a regular file",
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let temp = tempfile::Builder::new()
                    .prefix(".save.")
                    .suffix(".tmp")
                    .tempfile_in(&version)?;
                fs::copy(file, temp.path())?;
                restrict_file(temp.path())?;
                if !file_sha256(temp.path())?.eq_ignore_ascii_case(digest) {
                    return Err(io::Error::other("save changed while creating backup"));
                }
                temp.as_file().sync_all()?;
                temp.persist_noclobber(&destination).map_err(|e| e.error)?;
            }
            Err(e) => return Err(e),
        }
        let original = absolute(original)?;
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
    // Large reads keep the hash, not system calls, the bottleneck on big saves.
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(bytes_to_hex(&hash.finalize()))
}

pub(crate) fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}
