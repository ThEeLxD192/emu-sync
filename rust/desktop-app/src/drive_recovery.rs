use crate::{OperationFailure, OperationSupervisor};
use emusync_data_drive::{
    ConditionalError, DriveListing, ListingError, ReconcileStatus, ResumableUpload, TransferError,
    UploadError, UploadSession,
};
use emusync_data_local::{
    RecoveryCredentials, RecoveryState, RecoveryStore, TrackedError, TrackedFileSync, Unverifiable,
    UploadContext, UploadEvidence,
};
use emusync_domain::{config::CredentialTransaction, sync::RemoteState};
use std::{fmt, future::Future, io};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

#[derive(Debug, PartialEq, Eq)]
pub enum RecoveryStatus {
    Recovered,
    Incomplete { received: u64 },
    ExpiredOutcomeUnknown,
    RemoteUnconfirmed,
}
#[derive(Debug)]
pub enum RecoveryError {
    NoFailure,
    UnsupportedFailure,
    Poisoned,
    Upload(UploadError),
    Listing(ListingError),
    Metadata(TransferError),
    Local(io::Error),
    FailureStillReferenced,
}
impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoFailure => write!(f, "no operation failure to recover"),
            Self::UnsupportedFailure => write!(f, "operation requires a different recovery path"),
            Self::Poisoned => write!(f, "operation recovery state poisoned"),
            Self::Upload(e) => e.fmt(f),
            Self::Listing(e) => e.fmt(f),
            Self::Metadata(e) => e.fmt(f),
            Self::Local(e) => e.fmt(f),
            Self::FailureStillReferenced => write!(f, "recovery state still referenced"),
        }
    }
}
impl std::error::Error for RecoveryError {}

async fn require_current<S>(credentials: RecoveryCredentials<S>) -> Result<(), RecoveryError>
where
    S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
{
    tokio::task::spawn_blocking(move || credentials.ensure_current())
        .await
        .map_err(|error| {
            RecoveryError::Local(io::Error::other(format!(
                "credential worker failed: {error}"
            )))
        })?
        .map_err(RecoveryError::Local)
}
/// A transfer failure bound to the connection revision used for its token.
pub struct AuthorizedTransferFailure<S> {
    pub error: TrackedError<TransferError>,
    credentials: RecoveryCredentials<S>,
}
impl<S> AuthorizedTransferFailure<S> {
    fn new(error: TrackedError<TransferError>, credentials: RecoveryCredentials<S>) -> Self {
        Self { error, credentials }
    }
}
impl<S> fmt::Debug for AuthorizedTransferFailure<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthorizedTransferFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
/// Bind the authorization proof before spawning the operation, not after it fails.
pub fn start_authorized_transfer<S, T, F, Fut>(
    supervisor: &mut OperationSupervisor<AuthorizedTransferFailure<S>>,
    credentials: RecoveryCredentials<S>,
    operation: F,
) -> Result<oneshot::Receiver<Option<T>>, crate::StartError>
where
    S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
    T: Send + 'static,
    F: FnOnce(CancellationToken, RecoveryCredentials<S>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, TrackedError<TransferError>>> + Send + 'static,
{
    supervisor.start(move |cancel| async move {
        let check = credentials.clone();
        let current = tokio::task::spawn_blocking(move || check.ensure_current())
            .await
            .map_err(|error| io::Error::other(format!("credential worker failed: {error}")))
            .and_then(|result| result);
        if let Err(error) = current {
            return Err(AuthorizedTransferFailure::new(
                TrackedError::Local(error),
                credentials,
            ));
        }
        if cancel.is_cancelled() {
            let error = io::Error::new(
                io::ErrorKind::Interrupted,
                "operation cancelled before start",
            );
            return Err(AuthorizedTransferFailure::new(
                TrackedError::Local(error),
                credentials,
            ));
        }
        let operation_credentials = credentials.clone();
        operation(cancel, operation_credentials)
            .await
            .map_err(|error| AuthorizedTransferFailure::new(error, credentials))
    })
}
enum RemoteRecovery {
    Query(Box<UploadSession>),
    Known(RemoteState),
    Reported(Box<emusync_data_drive::CreatedFile>),
}
struct Inputs<S> {
    remote: RemoteRecovery,
    evidence: UploadEvidence,
    context: UploadContext,
    credentials: RecoveryCredentials<S>,
    digest: String,
}

/// Reconcile the retained upload and record its original context before releasing exclusion.
/// Incomplete, expired, invalid, or failed results keep the failure and snapshot retained.
pub async fn reconcile_tracked_upload<T, S>(
    supervisor: &mut OperationSupervisor<AuthorizedTransferFailure<S>>,
    resumable: &ResumableUpload,
    tracked: &TrackedFileSync<T>,
    token: &str,
) -> Result<RecoveryStatus, RecoveryError>
where
    S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
{
    reconcile_tracked_upload_inner(supervisor, resumable, None, tracked, token).await
}

/// As above, also recovering a completed session whose timestamp is invalid by
/// fetching its confirmed ID and verifying the original snapshot checksum.
pub async fn reconcile_tracked_upload_with_listing<T, S>(
    supervisor: &mut OperationSupervisor<AuthorizedTransferFailure<S>>,
    resumable: &ResumableUpload,
    listing: &DriveListing,
    tracked: &TrackedFileSync<T>,
    token: &str,
) -> Result<RecoveryStatus, RecoveryError>
where
    S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
{
    reconcile_tracked_upload_inner(supervisor, resumable, Some(listing), tracked, token).await
}

async fn reconcile_tracked_upload_inner<T, S>(
    supervisor: &mut OperationSupervisor<AuthorizedTransferFailure<S>>,
    resumable: &ResumableUpload,
    listing: Option<&DriveListing>,
    tracked: &TrackedFileSync<T>,
    token: &str,
) -> Result<RecoveryStatus, RecoveryError>
where
    S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
{
    let failure = supervisor.failure().ok_or(RecoveryError::NoFailure)?;
    let inputs = {
        let guard = failure.lock().map_err(|_| RecoveryError::Poisoned)?;
        let operation = match &*guard {
            OperationFailure::Operation(operation) => operation,
            _ => return Err(RecoveryError::UnsupportedFailure),
        };
        match &operation.error {
            TrackedError::Upload {
                source: TransferError::Upload(UploadError::RecoveryFailed { cause, .. }),
                snapshot,
                context,
            } if matches!(cause.as_ref(), UploadError::PrematureCompletion(_)) => {
                let UploadError::PrematureCompletion(file) = cause.as_ref() else {
                    unreachable!()
                };
                Inputs {
                    remote: RemoteRecovery::Reported(file.clone()),
                    evidence: snapshot.evidence(),
                    context: (**context).clone(),
                    credentials: operation.credentials.clone(),
                    digest: snapshot.digest().to_owned(),
                }
            }
            TrackedError::Upload {
                source: TransferError::Upload(UploadError::OutcomeUnknown(session)),
                snapshot,
                context,
            }
            | TrackedError::Upload {
                source: TransferError::Upload(UploadError::RecoveryFailed { session, .. }),
                snapshot,
                context,
            } => Inputs {
                remote: RemoteRecovery::Query(session.clone()),
                evidence: snapshot.evidence(),
                context: (**context).clone(),
                credentials: operation.credentials.clone(),
                digest: snapshot.digest().to_owned(),
            },
            TrackedError::AfterUpload {
                snapshot,
                remote,
                context,
                ..
            } => Inputs {
                remote: RemoteRecovery::Known((**remote).clone()),
                evidence: snapshot.evidence(),
                context: (**context).clone(),
                credentials: operation.credentials.clone(),
                digest: snapshot.digest().to_owned(),
            },
            _ => return Err(RecoveryError::UnsupportedFailure),
        }
    };
    require_current(inputs.credentials.clone()).await?;
    let remote = match inputs.remote {
        RemoteRecovery::Known(remote) => remote,
        RemoteRecovery::Reported(file) => {
            if file.id.trim().is_empty() || file.name != inputs.context.remote_name {
                return Ok(RecoveryStatus::RemoteUnconfirmed);
            }
            let Some(listing) = listing else {
                return Ok(RecoveryStatus::RemoteUnconfirmed);
            };
            let Some(remote) = listing
                .get_by_id_in_parent(token, &file.id, &inputs.context.folder)
                .await
                .map_err(RecoveryError::Listing)?
            else {
                return Ok(RecoveryStatus::RemoteUnconfirmed);
            };
            if remote.id != file.id
                || remote.name != inputs.context.remote_name
                || remote.is_folder
                || !remote
                    .sha256
                    .as_deref()
                    .is_some_and(|hash| hash.eq_ignore_ascii_case(&inputs.digest))
            {
                return Ok(RecoveryStatus::RemoteUnconfirmed);
            }
            RemoteState {
                id: remote.id,
                modified: remote.modified,
                sha256: remote.sha256,
                version: remote.version,
            }
        }
        RemoteRecovery::Query(session) => match resumable
            .reconcile(token, &session)
            .await
            .map_err(RecoveryError::Upload)?
        {
            ReconcileStatus::Complete(file) => {
                let id = file.id.clone();
                let verified = file
                    .sha256_checksum
                    .as_deref()
                    .is_some_and(|hash| hash.eq_ignore_ascii_case(&inputs.digest))
                    && file
                        .parents
                        .iter()
                        .any(|parent| parent == &inputs.context.folder);
                let parsed = RemoteState::try_from(file);
                if verified && let Ok(remote) = parsed.as_ref() {
                    remote.clone()
                } else if let Some(listing) = listing {
                    let Some(remote) = listing
                        .get_by_id_in_parent(token, &id, &inputs.context.folder)
                        .await
                        .map_err(RecoveryError::Listing)?
                    else {
                        return Ok(RecoveryStatus::RemoteUnconfirmed);
                    };
                    if remote.id != id
                        || remote.name != inputs.context.remote_name
                        || remote.is_folder
                        || !remote
                            .sha256
                            .as_deref()
                            .is_some_and(|hash| hash.eq_ignore_ascii_case(&inputs.digest))
                    {
                        return Ok(RecoveryStatus::RemoteUnconfirmed);
                    }
                    RemoteState {
                        id: remote.id,
                        modified: remote.modified,
                        sha256: remote.sha256,
                        version: remote.version,
                    }
                } else if let Err(error) = parsed {
                    return Err(RecoveryError::Metadata(error));
                } else {
                    return Ok(RecoveryStatus::RemoteUnconfirmed);
                }
            }
            ReconcileStatus::Incomplete { received } => {
                return Ok(RecoveryStatus::Incomplete { received });
            }
            ReconcileStatus::ExpiredOutcomeUnknown => {
                return Ok(RecoveryStatus::ExpiredOutcomeUnknown);
            }
        },
    };
    let local = inputs.context.local.to_str().ok_or_else(|| {
        RecoveryError::Local(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path is not UTF-8",
        ))
    })?;
    tracked
        .record_reconciled_upload_guarded(
            inputs.evidence,
            remote,
            local,
            &inputs.context.folder,
            &inputs.context.remote_name,
            inputs.credentials,
        )
        .await
        .map_err(RecoveryError::Local)?;
    tracked
        .resolve_recovery(inputs.context.recovery_id.as_deref())
        .await
        .map_err(RecoveryError::Local)?;
    drop(failure);
    if !supervisor.resolve() {
        return Err(RecoveryError::FailureStillReferenced);
    }
    Ok(RecoveryStatus::Recovered)
}

/// Reconcile an uncertain conditional replacement without resending the PUT.
/// A missing or different checksum remains unresolved, even if other metadata looks familiar.
pub async fn reconcile_conditional_upload<T, S>(
    supervisor: &mut OperationSupervisor<AuthorizedTransferFailure<S>>,
    listing: &DriveListing,
    tracked: &TrackedFileSync<T>,
    token: &str,
) -> Result<RecoveryStatus, RecoveryError>
where
    S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
{
    reconcile_upload_by_id(supervisor, listing, tracked, token, false).await
}

/// Reconcile a successful creation whose returned timestamp could not be parsed.
/// No second create request is sent; the reported ID and snapshot checksum are verified.
pub async fn reconcile_created_upload<T, S>(
    supervisor: &mut OperationSupervisor<AuthorizedTransferFailure<S>>,
    listing: &DriveListing,
    tracked: &TrackedFileSync<T>,
    token: &str,
) -> Result<RecoveryStatus, RecoveryError>
where
    S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
{
    reconcile_upload_by_id(supervisor, listing, tracked, token, true).await
}

async fn reconcile_upload_by_id<T, S>(
    supervisor: &mut OperationSupervisor<AuthorizedTransferFailure<S>>,
    listing: &DriveListing,
    tracked: &TrackedFileSync<T>,
    token: &str,
    created: bool,
) -> Result<RecoveryStatus, RecoveryError>
where
    S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
{
    let failure = supervisor.failure().ok_or(RecoveryError::NoFailure)?;
    let (context, evidence, digest, credentials, id) = {
        let guard = failure.lock().map_err(|_| RecoveryError::Poisoned)?;
        let operation = match &*guard {
            OperationFailure::Operation(operation) => operation,
            _ => return Err(RecoveryError::UnsupportedFailure),
        };
        match &operation.error {
            TrackedError::Upload {
                source:
                    TransferError::Conditional(
                        ConditionalError::OutcomeUnknown(_)
                        | ConditionalError::HeadersTimedOut
                        | ConditionalError::CancelledOutcomeUnknown
                        | ConditionalError::ResponseMetadata(_)
                        | ConditionalError::ServerOutcomeUnknown(_)
                        | ConditionalError::UnexpectedResponse(_),
                    ),
                snapshot,
                context,
            } if !created => (
                (**context).clone(),
                snapshot.evidence(),
                snapshot.digest().to_owned(),
                operation.credentials.clone(),
                context
                    .observed
                    .as_ref()
                    .ok_or(RecoveryError::UnsupportedFailure)?
                    .id
                    .clone(),
            ),
            TrackedError::Upload {
                source: TransferError::InvalidUpdatedTimestamp(_),
                snapshot,
                context,
            } if !created => (
                (**context).clone(),
                snapshot.evidence(),
                snapshot.digest().to_owned(),
                operation.credentials.clone(),
                context
                    .observed
                    .as_ref()
                    .ok_or(RecoveryError::UnsupportedFailure)?
                    .id
                    .clone(),
            ),
            TrackedError::Upload {
                source: TransferError::InvalidCreatedTimestamp(file),
                snapshot,
                context,
            } if created
                && context.observed.is_none()
                && !file.id.trim().is_empty()
                && file.name == context.remote_name =>
            {
                (
                    (**context).clone(),
                    snapshot.evidence(),
                    snapshot.digest().to_owned(),
                    operation.credentials.clone(),
                    file.id.clone(),
                )
            }
            _ => return Err(RecoveryError::UnsupportedFailure),
        }
    };
    require_current(credentials.clone()).await?;
    let Some(file) = listing
        .get_by_id_in_parent(token, &id, &context.folder)
        .await
        .map_err(RecoveryError::Listing)?
    else {
        return Ok(RecoveryStatus::RemoteUnconfirmed);
    };
    if file.id != id
        || file.name != context.remote_name
        || file.is_folder
        || !file
            .sha256
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(&digest))
    {
        return Ok(RecoveryStatus::RemoteUnconfirmed);
    }
    let remote = RemoteState {
        id: file.id,
        modified: file.modified,
        sha256: file.sha256,
        version: file.version,
    };
    let local = context.local.to_str().ok_or_else(|| {
        RecoveryError::Local(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path is not UTF-8",
        ))
    })?;
    tracked
        .record_reconciled_upload_guarded(
            evidence,
            remote,
            local,
            &context.folder,
            &context.remote_name,
            credentials.clone(),
        )
        .await
        .map_err(RecoveryError::Local)?;
    tracked
        .resolve_recovery(context.recovery_id.as_deref())
        .await
        .map_err(RecoveryError::Local)?;
    drop(failure);
    if !supervisor.resolve() {
        return Err(RecoveryError::FailureStillReferenced);
    }
    Ok(RecoveryStatus::Recovered)
}

/// Outcome of reconciling one entry that survived a restart.
#[derive(Debug, PartialEq, Eq)]
pub enum PersistedRecoveryStatus {
    Recovered,
    /// The remote does not (yet) prove the upload; the entry is kept.
    RemoteUnconfirmed,
    /// The entry was written under another connection; it is neither used nor removed.
    ConnectionChanged,
    /// The entry cannot be trusted; it is neither used nor removed.
    Unverifiable(Unverifiable),
}

/// Reconcile uploads persisted by an earlier process. Verified entries are matched
/// to the remote by identity, or by folder, name and checksum when the server never
/// reported an identity. No write is ever sent: an entry is removed only after the
/// remote proves the snapshot checksum and the history has been recorded.
pub async fn reconcile_persisted_uploads<T, S>(
    store: std::sync::Arc<RecoveryStore>,
    listing: &DriveListing,
    tracked: &TrackedFileSync<T>,
    token: &str,
    credentials: RecoveryCredentials<S>,
) -> Result<Vec<(String, PersistedRecoveryStatus)>, RecoveryError>
where
    S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
{
    let entries = tokio::task::spawn_blocking({
        let store = store.clone();
        move || store.list()
    })
    .await
    .map_err(|error| {
        RecoveryError::Local(io::Error::other(format!("recovery worker failed: {error}")))
    })?
    .map_err(RecoveryError::Local)?;
    let mut results = Vec::new();
    for entry in entries {
        let (record, snapshot) = match entry.state {
            RecoveryState::Verified { record, snapshot } => (record, snapshot),
            RecoveryState::Unverifiable(reason) => {
                results.push((entry.id, PersistedRecoveryStatus::Unverifiable(reason)));
                continue;
            }
        };
        if record.connection != credentials.fingerprint() {
            results.push((entry.id, PersistedRecoveryStatus::ConnectionChanged));
            continue;
        }
        require_current(credentials.clone()).await?;
        let file = match &record.remote_id {
            Some(id) => listing
                .get_by_id_in_parent(token, id, &record.folder)
                .await
                .map_err(RecoveryError::Listing)?,
            None => listing
                .find_by_name(token, &record.remote_name, Some(&record.folder))
                .await
                .map_err(RecoveryError::Listing)?,
        };
        let confirmed = file.filter(|file| {
            !file.is_folder
                && file.name == record.remote_name
                && record.remote_id.as_ref().is_none_or(|id| *id == file.id)
                && file
                    .sha256
                    .as_deref()
                    .is_some_and(|hash| hash.eq_ignore_ascii_case(&record.digest))
        });
        let Some(file) = confirmed else {
            results.push((entry.id, PersistedRecoveryStatus::RemoteUnconfirmed));
            continue;
        };
        tracked
            .record_reconciled_upload_guarded(
                UploadEvidence::recovered(snapshot, record.digest.clone()),
                RemoteState {
                    id: file.id,
                    modified: file.modified,
                    sha256: file.sha256,
                    version: file.version,
                },
                &record.local,
                &record.folder,
                &record.remote_name,
                credentials.clone(),
            )
            .await
            .map_err(RecoveryError::Local)?;
        tracked
            .resolve_recovery(Some(&entry.id))
            .await
            .map_err(RecoveryError::Local)?;
        results.push((entry.id, PersistedRecoveryStatus::Recovered));
    }
    Ok(results)
}
