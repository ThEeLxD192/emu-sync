use crate::{
    HistoryCheckpoint, RecoveryRecord, RecoveryStore, StagedRestore, SyncHistoryStore,
    UploadEvidence, UploadSnapshot, file_sha256,
};
use emusync_domain::config::{AppConfig, CredentialSnapshot, CredentialTransaction};
use emusync_domain::sync::{
    Checkpoint, CloudSaveTransfer, Decision, DirectoryDownload, LocalSaveFiles, RemoteState,
    plan_tracked,
};
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// Original inputs retained with an uncertain or locally incomplete upload.
#[derive(Clone, Debug)]
pub struct UploadContext {
    pub local: PathBuf,
    pub folder: String,
    pub remote_name: String,
    pub observed: Option<RemoteState>,
    /// Persistent recovery entry for this upload, when recovery storage is configured.
    pub recovery_id: Option<String>,
}

#[derive(Debug)]
pub enum TrackedError<E> {
    Local(io::Error),
    Transfer(E),
    /// The caller owns the snapshot until it resolves the remote outcome.
    Upload {
        source: E,
        snapshot: UploadSnapshot,
        context: Box<UploadContext>,
    },
    /// The remote write succeeded, but local verification or history failed.
    AfterUpload {
        source: io::Error,
        snapshot: UploadSnapshot,
        remote: Box<RemoteState>,
        context: Box<UploadContext>,
    },
}
impl<E: fmt::Display> fmt::Display for TrackedError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(e) => e.fmt(f),
            Self::Transfer(e) => e.fmt(f),
            Self::Upload { source, .. } => source.fmt(f),
            Self::AfterUpload { source, .. } => source.fmt(f),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for TrackedError<E> {}
impl<E> From<io::Error> for TrackedError<E> {
    fn from(e: io::Error) -> Self {
        Self::Local(e)
    }
}

/// Connection captured when the access token was authorized. Keep this guard with
/// the operation, never reconstruct it from a newer configuration during recovery.
pub struct RecoveryCredentials<S> {
    store: Arc<S>,
    expected: CredentialSnapshot,
    fallback: AppConfig,
}
impl<S> Clone for RecoveryCredentials<S> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            expected: self.expected.clone(),
            fallback: self.fallback.clone(),
        }
    }
}
impl<S> RecoveryCredentials<S> {
    pub fn new(store: Arc<S>, expected: CredentialSnapshot, fallback: AppConfig) -> Self {
        Self {
            store,
            expected,
            fallback,
        }
    }
}
impl<S> RecoveryCredentials<S> {
    /// Stable identity of the authorizing connection, comparable across processes.
    /// Only a digest is derived; no credential material is stored or logged.
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        if let Some(credentials) = &self.expected.credentials {
            for part in [
                credentials.client_id.as_str(),
                credentials.client_secret.as_str(),
                credentials.refresh_token.as_deref().unwrap_or(""),
            ] {
                hash.update((part.len() as u64).to_le_bytes());
                hash.update(part.as_bytes());
            }
        }
        crate::history::bytes_to_hex(&hash.finalize())
    }
}
impl<S: CredentialTransaction<Error = io::Error>> RecoveryCredentials<S> {
    pub fn ensure_current(&self) -> io::Result<()> {
        self.store
            .while_current(&self.expected, &self.fallback, || ())?
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::PermissionDenied, "Drive connection changed")
            })
    }
}
/// Inputs of one guarded upload.
#[derive(Clone, Copy, Debug)]
pub struct UploadRequest<'a> {
    pub token: &'a str,
    pub local: &'a str,
    pub remote: Option<&'a RemoteState>,
    pub folder: &'a str,
    pub remote_name: &'a str,
}

/// The operation owner must await each future through completion, including cancellation cleanup.
#[derive(Clone)]
pub struct TrackedFileSync<T> {
    transfer: T,
    history: Arc<SyncHistoryStore>,
    recovery: Option<Arc<RecoveryStore>>,
}
impl<T> TrackedFileSync<T> {
    pub fn new(transfer: T, history: Arc<SyncHistoryStore>) -> Self {
        Self {
            transfer,
            history,
            recovery: None,
        }
    }
    /// Persist each guarded upload before its first write, so a later process can
    /// reconcile it. Without this, retained uploads exist only in memory.
    pub fn with_recovery(mut self, recovery: Arc<RecoveryStore>) -> Self {
        self.recovery = Some(recovery);
        self
    }

    /// Record an in-sync observation only while the authorizing connection is current.
    pub async fn remember_guarded<S>(
        &self,
        local: &str,
        remote: &RemoteState,
        folder: &str,
        remote_name: &str,
        credentials: RecoveryCredentials<S>,
    ) -> io::Result<()>
    where
        S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
    {
        let path = PathBuf::from(local);
        let history = self.history.clone();
        let folder = folder.to_owned();
        let name = remote_name.to_owned();
        let remote = remote.clone();
        blocking(move || {
            credentials
                .store
                .while_current(&credentials.expected, &credentials.fallback, || {
                    let digest = file_sha256(&path)?;
                    let key = history.key(&path, &folder, &name)?;
                    let previous = history.get(&key)?.map(as_checkpoint).transpose()?;
                    if plan_tracked(Some(&digest), Some(&remote), previous.as_ref())
                        != Decision::InSync
                    {
                        return Err(io::Error::other("save changed during synchronization"));
                    }
                    history.record(&key, history_checkpoint(digest, &remote)?)
                })?
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "Drive connection changed during synchronization",
                    )
                })?
        })
        .await
    }
    /// Remove a persistent recovery entry once its outcome is reconciled and recorded.
    pub async fn resolve_recovery(&self, id: Option<&str>) -> io::Result<()> {
        let (Some(store), Some(id)) = (self.recovery.clone(), id.map(str::to_owned)) else {
            return Ok(());
        };
        blocking(move || store.remove(&id)).await
    }
    /// Record a reconciled remote write under the originally authorized connection.
    /// The snapshot owner must remain alive until the operation completes.
    pub async fn record_reconciled_upload_guarded<S>(
        &self,
        evidence: UploadEvidence,
        remote: RemoteState,
        local: &str,
        folder: &str,
        remote_name: &str,
        credentials: RecoveryCredentials<S>,
    ) -> io::Result<()>
    where
        S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
    {
        let history = self.history.clone();
        let local = PathBuf::from(local);
        let folder = folder.to_owned();
        let name = remote_name.to_owned();
        blocking(move || {
            let result = credentials.store.while_current(
                &credentials.expected,
                &credentials.fallback,
                || record_reconciled(&history, evidence, remote, local, folder, name),
            )?;
            result.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Drive connection changed during recovery",
                )
            })?
        })
        .await
    }
    /// Unguarded primitive for contexts which already serialize connection changes.
    pub async fn record_reconciled_upload(
        &self,
        evidence: UploadEvidence,
        remote: RemoteState,
        local: &str,
        folder: &str,
        remote_name: &str,
    ) -> io::Result<()> {
        let history = self.history.clone();
        let local = PathBuf::from(local);
        let folder = folder.to_owned();
        let name = remote_name.to_owned();
        blocking(move || record_reconciled(&history, evidence, remote, local, folder, name)).await
    }
}
impl<T> TrackedFileSync<T>
where
    T: CloudSaveTransfer + Sync,
    T::Error: Send,
{
    /// Stage a download and commit it only under the original connection revision.
    /// Callers must retain operation exclusion and await the future to completion.
    pub async fn download_guarded<S>(
        &self,
        token: &str,
        remote: &RemoteState,
        local: &str,
        folder: &str,
        remote_name: &str,
        credentials: RecoveryCredentials<S>,
    ) -> Result<(), TrackedError<T::Error>>
    where
        S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
    {
        let check = credentials.clone();
        blocking(move || check.ensure_current()).await?;
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
            let result = credentials.store.while_current(
                &credentials.expected,
                &credentials.fallback,
                || {
                    let key = history.key(&path, &folder, &name)?;
                    let digest = stage.commit(remote.sha256.as_deref(), &history, &key)?;
                    history.record(&key, history_checkpoint(digest, &remote)?)
                },
            )?;
            result.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Drive connection changed during download",
                )
            })?
        })
        .await?;
        Ok(())
    }

    /// Upload with the original OAuth revision guarding the history commit.
    /// The supervisor must retain and await this future through cleanup and recovery.
    /// Every failure keeps its persistent recovery entry; see `upload_guarded_classified`.
    pub async fn upload_guarded<S>(
        &self,
        token: &str,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
        credentials: RecoveryCredentials<S>,
    ) -> Result<(), TrackedError<T::Error>>
    where
        S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
    {
        let request = UploadRequest {
            token,
            local,
            remote,
            folder,
            remote_name,
        };
        self.upload_guarded_classified(request, credentials, |_| false)
            .await
    }

    /// As `upload_guarded`, discarding the persistent recovery entry when
    /// `definitive_failure` proves the remote was left unchanged. Any other failure
    /// keeps the entry. Staging failures and unresolved entries for the same save
    /// stop the upload before any write is sent.
    pub async fn upload_guarded_classified<S>(
        &self,
        request: UploadRequest<'_>,
        credentials: RecoveryCredentials<S>,
        definitive_failure: impl Fn(&T::Error) -> bool,
    ) -> Result<(), TrackedError<T::Error>>
    where
        S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
    {
        let UploadRequest {
            token,
            local,
            remote,
            folder,
            remote_name,
        } = request;
        let check = credentials.clone();
        blocking(move || check.ensure_current()).await?;
        let path = PathBuf::from(local);
        let mut context = Box::new(UploadContext {
            local: crate::storage::absolute(&path)?,
            folder: folder.to_owned(),
            remote_name: remote_name.to_owned(),
            observed: remote.cloned(),
            recovery_id: None,
        });
        if let Some(store) = self.recovery.clone() {
            let (path, folder, name) = (
                context.local.clone(),
                folder.to_owned(),
                remote_name.to_owned(),
            );
            if blocking(move || store.has_unresolved(&path, &folder, &name)).await? {
                return Err(TrackedError::Local(io::Error::new(
                    io::ErrorKind::ResourceBusy,
                    "an earlier upload of this save has an unresolved remote outcome",
                )));
            }
        }
        let snapshot = blocking({
            let path = path.clone();
            move || UploadSnapshot::prepare(&path)
        })
        .await?;
        if let Some(store) = self.recovery.clone() {
            let record = RecoveryRecord {
                remote_id: remote.map(|state| state.id.clone()),
                local: context
                    .local
                    .to_str()
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "path is not UTF-8")
                    })?
                    .to_owned(),
                folder: folder.to_owned(),
                remote_name: remote_name.to_owned(),
                observed: remote.map(|state| crate::ObservedRemote {
                    id: state.id.clone(),
                    modified_unix_nanos: state.modified.to_string(),
                    sha256: state.sha256.clone(),
                    version: state.version.clone(),
                }),
                digest: snapshot.digest().to_owned(),
                connection: credentials.fingerprint(),
            };
            // Fail closed: no write is sent unless its recovery state is durable.
            let source = snapshot.path().to_owned();
            context.recovery_id = Some(blocking(move || store.stage_file(&source, record)).await?);
        }
        let result = if let Some(remote) = remote {
            self.transfer
                .replace(token, remote, text_path(snapshot.path())?, remote_name)
                .await
        } else {
            self.transfer
                .upload(token, text_path(snapshot.path())?, remote_name, folder)
                .await
        };
        let result = match result {
            Ok(result) => result,
            Err(source) => {
                if definitive_failure(&source) {
                    // Best effort: a leftover entry only causes a later, harmless reconciliation.
                    let _ = self.resolve_recovery(context.recovery_id.as_deref()).await;
                    context.recovery_id = None;
                }
                return Err(TrackedError::Upload {
                    source,
                    snapshot,
                    context,
                });
            }
        };
        match self
            .record_reconciled_upload_guarded(
                snapshot.evidence(),
                result.clone(),
                local,
                folder,
                remote_name,
                credentials,
            )
            .await
        {
            Ok(()) => {
                let _ = self.resolve_recovery(context.recovery_id.as_deref()).await;
                Ok(())
            }
            Err(source) => Err(TrackedError::AfterUpload {
                source,
                snapshot,
                remote: Box::new(result),
                context,
            }),
        }
    }
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

#[cfg(target_os = "linux")]
impl<T> TrackedFileSync<T>
where
    T: CloudSaveTransfer + Sync,
    T::Error: Send,
{
    /// Download under a pinned root and commit only while the OAuth revision remains current.
    pub async fn download_pinned_guarded<S>(
        &self,
        token: &str,
        remote: &RemoteState,
        root: &str,
        folder: &str,
        remote_name: &str,
        credentials: RecoveryCredentials<S>,
    ) -> Result<(), TrackedError<T::Error>>
    where
        S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
    {
        let local = Path::new(root).join(remote_name);
        self.download_directory_guarded(
            DirectoryDownload {
                token,
                remote,
                local: text_path(&local)?,
                root,
                relative: remote_name,
                folder,
                remote_name,
            },
            credentials,
        )
        .await
    }

    /// Commit a remote-derived relative path beneath the selected pinned root.
    pub async fn download_directory_guarded<S>(
        &self,
        request: DirectoryDownload<'_>,
        credentials: RecoveryCredentials<S>,
    ) -> Result<(), TrackedError<T::Error>>
    where
        S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
    {
        let DirectoryDownload {
            token,
            remote,
            local,
            root,
            relative,
            folder,
            remote_name,
        } = request;
        if crate::storage::absolute(&Path::new(root).join(relative))?
            != crate::storage::absolute(Path::new(local))?
        {
            return Err(TrackedError::Local(io::Error::new(
                io::ErrorKind::InvalidInput,
                "directory destination does not match the selected root",
            )));
        }
        let check = credentials.clone();
        blocking(move || check.ensure_current()).await?;
        let root = PathBuf::from(root);
        let name = relative.to_owned();
        let stage = blocking(move || {
            StagedRestore::prepare_pinned(&root, &name)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid remote path"))
        })
        .await?;
        let local = stage.logical_destination().to_owned();
        self.transfer
            .download(
                token,
                &remote.id,
                text_path(stage.path())?,
                Some(remote.modified),
                remote_name,
            )
            .await
            .map_err(TrackedError::Transfer)?;
        let history = self.history.clone();
        let folder = folder.to_owned();
        let name = remote_name.to_owned();
        let remote = remote.clone();
        blocking(move || {
            let result = credentials.store.while_current(
                &credentials.expected,
                &credentials.fallback,
                || {
                    let key = history.key(&local, &folder, &name)?;
                    let digest = stage.commit(remote.sha256.as_deref(), &history, &key)?;
                    history.record(&key, history_checkpoint(digest, &remote)?)
                },
            )?;
            result.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Drive connection changed during download",
                )
            })?
        })
        .await?;
        Ok(())
    }

    /// Download a remote name relative to a pinned root; the caller retains operation exclusion.
    pub async fn download_pinned(
        &self,
        token: &str,
        remote: &RemoteState,
        root: &str,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), TrackedError<T::Error>> {
        let root = PathBuf::from(root);
        let name = remote_name.to_owned();
        let stage = blocking(move || {
            StagedRestore::prepare_pinned(&root, &name)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid remote path"))
        })
        .await?;
        let local = stage.logical_destination().to_owned();
        self.transfer
            .download(
                token,
                &remote.id,
                text_path(stage.path())?,
                Some(remote.modified),
                remote_name,
            )
            .await
            .map_err(TrackedError::Transfer)?;
        let history = self.history.clone();
        let folder = folder.to_owned();
        let name = remote_name.to_owned();
        let remote = remote.clone();
        blocking(move || {
            let key = history.key(&local, &folder, &name)?;
            let digest = stage.commit(remote.sha256.as_deref(), &history, &key)?;
            history.record(&key, history_checkpoint(digest, &remote)?)
        })
        .await?;
        Ok(())
    }
}

async fn blocking<R: Send + 'static>(
    task: impl FnOnce() -> io::Result<R> + Send + 'static,
) -> io::Result<R> {
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|e| io::Error::other(format!("local worker failed: {e}")))?
}
fn record_reconciled(
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
fn set_checked_path_mtime(path: &Path, digest: &str, modified: i128) -> io::Result<()> {
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
fn set_checked_file_mtime(file: &mut fs::File, digest: &str, modified: i128) -> io::Result<()> {
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
        let context = Box::new(UploadContext {
            local: crate::storage::absolute(&path)?,
            folder: folder.to_owned(),
            remote_name: remote_name.to_owned(),
            observed: remote.cloned(),
            recovery_id: None,
        });
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
        };
        let result = match result {
            Ok(result) => result,
            Err(source) => {
                return Err(TrackedError::Upload {
                    source,
                    snapshot,
                    context,
                });
            }
        };
        let history = self.history.clone();
        let folder = folder.to_owned();
        let name = remote_name.to_owned();
        tokio::task::spawn_blocking(move || {
            let verify = || -> io::Result<()> {
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
                set_checked_path_mtime(&path, snapshot.digest(), result.modified)?;
                Ok(())
            };
            match verify() {
                Ok(()) => Ok(()),
                Err(source) => Err(Box::new((source, snapshot, result))),
            }
        })
        .await
        .map_err(|error| {
            TrackedError::Local(io::Error::other(format!("local worker failed: {error}")))
        })?
        .map_err(|failure| {
            let (source, snapshot, remote) = *failure;
            TrackedError::AfterUpload {
                source,
                snapshot,
                remote: Box::new(remote),
                context,
            }
        })?;
        Ok(())
    }
}
