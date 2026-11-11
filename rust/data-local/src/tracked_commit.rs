use crate::{HistoryCheckpoint, SyncHistoryStore, UploadEvidence, file_sha256};
use emusync_domain::sync::{Checkpoint, RemoteState};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(crate) fn record_reconciled(
    history: &SyncHistoryStore,
    evidence: UploadEvidence,
    remote: RemoteState,
    local: PathBuf,
    folder: String,
    name: String,
) -> io::Result<()> {
    if file_sha256(&evidence.path)? != evidence.digest
        || remote
            .sha256
            .as_ref()
            .is_some_and(|hash| !hash.eq_ignore_ascii_case(&evidence.digest))
    {
        return Err(io::Error::other(
            "reconciled upload content could not be verified",
        ));
    }
    let key = history.key(&local, &folder, &name)?;
    history.record(&key, history_checkpoint(evidence.digest.clone(), &remote)?)?;
    set_checked_path_mtime(&local, &evidence.digest, remote.modified)
}

pub(crate) fn set_checked_path_mtime(path: &Path, digest: &str, modified: i128) -> io::Result<()> {
    let mut file = fs::File::open(path)?;
    #[cfg(unix)]
    {
        set_checked_file_mtime(&mut file, digest, modified)
    }
    #[cfg(not(unix))]
    {
        if crate::history::reader_sha256(&mut file)? != digest {
            return Err(io::Error::other("save changed during upload"));
        }
        let seconds =
            i64::try_from(modified.div_euclid(1_000_000_000)).map_err(io::Error::other)?;
        let nanos = u32::try_from(modified.rem_euclid(1_000_000_000)).map_err(io::Error::other)?;
        filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(seconds, nanos))
    }
}

#[cfg(unix)]
pub(crate) fn set_checked_file_mtime(
    file: &mut fs::File,
    digest: &str,
    modified: i128,
) -> io::Result<()> {
    if crate::history::reader_sha256(file)? != digest {
        return Err(io::Error::other("save changed during upload"));
    }
    let seconds = i64::try_from(modified.div_euclid(1_000_000_000)).map_err(io::Error::other)?;
    let nanos = u32::try_from(modified.rem_euclid(1_000_000_000)).map_err(io::Error::other)?;
    filetime::set_file_handle_times(
        file,
        None,
        Some(filetime::FileTime::from_unix_time(seconds, nanos)),
    )
}

pub(crate) fn as_checkpoint(c: HistoryCheckpoint) -> io::Result<Checkpoint> {
    let modified = OffsetDateTime::parse(&c.remote_modified_time, &Rfc3339)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        .unix_timestamp_nanos();
    Ok(Checkpoint {
        local_sha256: c.local_sha256,
        remote_id: c.remote_id,
        remote_modified: modified,
        remote_sha256: c.remote_sha256,
        remote_version: c.remote_version,
    })
}

pub(crate) fn history_checkpoint(
    digest: String,
    remote: &RemoteState,
) -> io::Result<HistoryCheckpoint> {
    let modified = OffsetDateTime::from_unix_timestamp_nanos(remote.modified)
        .map_err(io::Error::other)?
        .format(&Rfc3339)
        .map_err(io::Error::other)?;
    Ok(HistoryCheckpoint {
        local_sha256: digest,
        remote_id: remote.id.clone(),
        remote_modified_time: modified,
        remote_sha256: remote.sha256.clone(),
        remote_version: remote.version.clone(),
    })
}

pub(crate) fn text_path(path: &Path) -> io::Result<&str> {
    path.to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path is not UTF-8"))
}

#[cfg(all(test, unix))]
mod timestamp_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn replacing_path_after_open_does_not_change_other_file_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save");
        let moved = dir.path().join("moved");
        let other = dir.path().join("other");
        fs::write(&path, b"abc").unwrap();
        fs::write(&other, b"abc").unwrap();
        filetime::set_file_mtime(&other, filetime::FileTime::from_unix_time(100, 0)).unwrap();
        let digest = file_sha256(&path).unwrap();
        let mut file = fs::File::open(&path).unwrap();
        fs::rename(&path, &moved).unwrap();
        symlink(&other, &path).unwrap();
        set_checked_file_mtime(&mut file, &digest, 200_000_000_000).unwrap();
        assert_eq!(
            filetime::FileTime::from_last_modification_time(&fs::metadata(&other).unwrap())
                .unix_seconds(),
            100
        );
        assert_eq!(
            filetime::FileTime::from_last_modification_time(&fs::metadata(&moved).unwrap())
                .unix_seconds(),
            200
        );
    }
}
