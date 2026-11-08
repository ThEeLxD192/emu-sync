//! Operation recovery for Google Drive transfers and persisted sessions.

mod live;
mod persisted;

pub use live::{
    reconcile_conditional_upload, reconcile_created_upload, reconcile_tracked_upload,
    reconcile_tracked_upload_with_listing,
};
pub use persisted::{PersistedRecoveryStatus, reconcile_persisted_uploads};

use crate::{OperationFailure, OperationSupervisor};
use emusync_data_drive::{ListingError, TransferError, UploadError};
use emusync_data_local::{RecoveryCredentials, TrackedError};
use emusync_domain::config::CredentialTransaction;
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

pub(crate) async fn require_current<S>(
    credentials: RecoveryCredentials<S>,
) -> Result<(), RecoveryError>
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
    pub credentials: RecoveryCredentials<S>,
}

impl<S> AuthorizedTransferFailure<S> {
    pub fn new(error: TrackedError<TransferError>, credentials: RecoveryCredentials<S>) -> Self {
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
