use crate::{HistoryCheckpoint, StagedRestore, SyncHistoryStore, UploadSnapshot, file_sha256};
use emusync_domain::sync::{
    Checkpoint, CloudSaveTransfer, Decision, LocalSaveFiles, RemoteState, plan_tracked,
};
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Debug)]
pub enum TrackedError<E> {
    Local(io::Error),
    Transfer(E),
}
impl<E: fmt::Display> fmt::Display for TrackedError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(e) => e.fmt(f),
            Self::Transfer(e) => e.fmt(f),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for TrackedError<E> {}
impl<E> From<io::Error> for TrackedError<E> {
    fn from(e: io::Error) -> Self {
        Self::Local(e)
    }
}

/// The operation owner must await each future through completion, including cancellation cleanup.
pub struct TrackedFileSync<T> {
    transfer: T,
    history: Arc<SyncHistoryStore>,
}
impl<T> TrackedFileSync<T> {
    pub fn new(transfer: T, history: Arc<SyncHistoryStore>) -> Self {
        Self { transfer, history }
    }
}
async fn blocking<R: Send + 'static>(
    task: impl FnOnce() -> io::Result<R> + Send + 'static,
) -> io::Result<R> {
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|e| io::Error::other(format!("local worker failed: {e}")))?
}
fn as_checkpoint(c: HistoryCheckpoint) -> io::Result<Checkpoint> {
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
fn history_checkpoint(digest: String, remote: &RemoteState) -> io::Result<HistoryCheckpoint> {
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
fn optional_hash(path: &Path) -> io::Result<Option<String>> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(file_sha256(path)?)),
        Ok(_) => Ok(None),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn text_path(path: &Path) -> io::Result<&str> {
    path.to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path is not UTF-8"))
}
impl<T> LocalSaveFiles for TrackedFileSync<T>
where
    T: CloudSaveTransfer + Sync,
    T::Error: Send,
{
    type Error = TrackedError<T::Error>;
    async fn plan(
        &self,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
    ) -> Result<Decision, Self::Error> {
        let path = PathBuf::from(local);
        let history = self.history.clone();
        let folder = folder.to_owned();
        let name = remote_name.to_owned();
        let remote = remote.cloned();
        Ok(blocking(move || {
            let digest = optional_hash(&path)?;
            let key = history.key(&path, &folder, &name)?;
            let previous = history.get(&key)?.map(as_checkpoint).transpose()?;
            Ok(plan_tracked(
                digest.as_deref(),
                remote.as_ref(),
                previous.as_ref(),
            ))
        })
        .await?)
    }
    async fn remember(
        &self,
        local: &str,
        remote: &RemoteState,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        let path = PathBuf::from(local);
        let history = self.history.clone();
        let folder = folder.to_owned();
        let name = remote_name.to_owned();
        let remote = remote.clone();
        blocking(move || {
            let digest = file_sha256(&path)?;
            let key = history.key(&path, &folder, &name)?;
            let previous = history.get(&key)?.map(as_checkpoint).transpose()?;
            if plan_tracked(Some(&digest), Some(&remote), previous.as_ref()) != Decision::InSync {
                return Err(io::Error::other("save changed during synchronization"));
            }
            history.record(&key, history_checkpoint(digest, &remote)?)
        })
        .await?;
        Ok(())
    }
    async fn download(
        &self,
        token: &str,
        remote: &RemoteState,
        local: &str,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        let path = PathBuf::from(local);
        let stage = blocking({
            let path = path.clone();
            move || StagedRestore::prepare(&path)
        })
        .await?;
        self.transfer
            .download(
                token,
                &remote.id,
                text_path(stage.path())?,
                Some(remote.modified),
                path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
            )
            .await
            .map_err(TrackedError::Transfer)?;
        let history = self.history.clone();
        let folder = folder.to_owned();
        let name = remote_name.to_owned();
        let remote = remote.clone();
        blocking(move || {
            let key = history.key(&path, &folder, &name)?;
            let digest = stage.commit(remote.sha256.as_deref(), &history, &key)?;
            history.record(&key, history_checkpoint(digest, &remote)?)
        })
        .await?;
        Ok(())
    }
    async fn upload(
        &self,
        token: &str,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        let path = PathBuf::from(local);
        let snapshot = blocking({
            let path = path.clone();
            move || UploadSnapshot::prepare(&path)
        })
        .await?;
        let result = if let Some(remote) = remote {
            self.transfer
                .replace(token, remote, text_path(snapshot.path())?, remote_name)
                .await
        } else {
            self.transfer
                .upload(token, text_path(snapshot.path())?, remote_name, folder)
                .await
        }
        .map_err(TrackedError::Transfer)?;
        let history = self.history.clone();
        let folder = folder.to_owned();
        let name = remote_name.to_owned();
        blocking(move || {
            if result
                .sha256
                .as_ref()
                .is_some_and(|h| !h.eq_ignore_ascii_case(snapshot.digest()))
            {
                return Err(io::Error::other("uploaded content could not be verified"));
            }
            let key = history.key(&path, &folder, &name)?;
            history.record(
                &key,
                history_checkpoint(snapshot.digest().to_owned(), &result)?,
            )?;
            snapshot.verify_uploaded(result.sha256.as_deref(), &path)?;
            let seconds = result.modified.div_euclid(1_000_000_000);
            let nanos = result.modified.rem_euclid(1_000_000_000);
            let seconds = i64::try_from(seconds).map_err(io::Error::other)?;
            let nanos = u32::try_from(nanos).map_err(io::Error::other)?;
            filetime::set_file_mtime(&path, filetime::FileTime::from_unix_time(seconds, nanos))?;
            Ok(())
        })
        .await?;
        Ok(())
    }
}
