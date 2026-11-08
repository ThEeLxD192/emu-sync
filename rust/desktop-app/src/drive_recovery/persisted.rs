use super::{RecoveryError, require_current};
use emusync_data_drive::DriveListing;
use emusync_data_local::{
    RecoveryCredentials, RecoveryState, RecoveryStore, TrackedFileSync, Unverifiable,
    UploadEvidence,
};
use emusync_domain::{config::CredentialTransaction, sync::RemoteState};
use std::{io, sync::Arc};

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
    store: Arc<RecoveryStore>,
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
