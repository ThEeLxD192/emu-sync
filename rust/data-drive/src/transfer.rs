use crate::{
    ConditionalError, ConditionalUpdate, CreatedFile, DownloadError, DownloadOptions,
    DriveDownload, DriveHttpClients, DriveHttpPolicy, FileRevision, ResumableUpload,
    TransferProgress, UploadError, UploadedFile,
};
use emusync_domain::sync::{CloudSaveTransfer, RemoteState, Timestamp};
use reqwest::{Client, Url};
use std::{fmt, path::Path, sync::Arc};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub enum TransferError {
    Download(DownloadError),
    Upload(UploadError),
    Conditional(ConditionalError),
    InvalidCreatedTimestamp(Box<CreatedFile>),
    InvalidUpdatedTimestamp(Box<UploadedFile>),
    Cancelled,
}
impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Download(e) => e.fmt(f),
            Self::Upload(e) => e.fmt(f),
            Self::Conditional(e) => e.fmt(f),
            Self::InvalidCreatedTimestamp(_) | Self::InvalidUpdatedTimestamp(_) => {
                write!(f, "invalid Drive timestamp after remote write")
            }
            Self::Cancelled => write!(f, "transfer cancelled"),
        }
    }
}
impl std::error::Error for TransferError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Download(e) => Some(e),
            Self::Upload(e) => Some(e),
            Self::Conditional(e) => Some(e),
            Self::InvalidCreatedTimestamp(_)
            | Self::InvalidUpdatedTimestamp(_)
            | Self::Cancelled => None,
        }
    }
}
impl TransferError {
    /// True only when the failure proves that no remote write took effect: a
    /// cancellation before any write, a failed precondition or other rejection by
    /// the server, or a failure before the first request. Everything else keeps
    /// its recovery state because the remote outcome may be unknown.
    pub fn leaves_remote_unchanged(&self) -> bool {
        match self {
            Self::Cancelled
            | Self::Upload(
                UploadError::Cancelled
                | UploadError::Io(_)
                | UploadError::InvalidSession
                | UploadError::Transport(_)
                | UploadError::Http(_),
            ) => true,
            Self::Conditional(error) => matches!(
                error,
                ConditionalError::Changed
                    | ConditionalError::MissingStrongEtag
                    | ConditionalError::ControlMetadata(_)
                    | ConditionalError::InvalidEndpoint
                    | ConditionalError::Cancelled
                    | ConditionalError::Transport(_)
                    | ConditionalError::Io(_)
                    | ConditionalError::Http(_)
            ),
            _ => false,
        }
    }
}
fn timestamp(value: &str) -> Option<Timestamp> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|v| v.unix_timestamp_nanos())
        .ok()
}

impl TryFrom<CreatedFile> for RemoteState {
    type Error = TransferError;
    fn try_from(result: CreatedFile) -> Result<Self, Self::Error> {
        let Some(modified) = timestamp(&result.modified_time) else {
            return Err(TransferError::InvalidCreatedTimestamp(Box::new(result)));
        };
        Ok(Self {
            id: result.id,
            modified,
            sha256: result.sha256_checksum,
            version: result.version,
        })
    }
}

/// An operation-scoped adapter. The owner must await it and retain exclusion on an
/// unknown upload outcome until it has been reconciled; cancellation is cooperative.
#[derive(Clone)]
pub struct DriveTransfer {
    download: DriveDownload,
    upload: ResumableUpload,
    conditional: ConditionalUpdate,
    cancel: CancellationToken,
    progress: Arc<dyn Fn(TransferProgress) + Send + Sync>,
}
impl DriveTransfer {
    pub fn new(
        client: Client,
        files_v3: Url,
        upload_v3: Url,
        files_v2: Url,
        upload_v2: Url,
        cancel: CancellationToken,
        progress: Arc<dyn Fn(TransferProgress) + Send + Sync>,
    ) -> Self {
        Self {
            download: DriveDownload::new(client.clone(), files_v3),
            upload: ResumableUpload::new(client.clone(), upload_v3),
            conditional: ConditionalUpdate::new(client, files_v2, upload_v2),
            cancel,
            progress,
        }
    }
    /// Production endpoints and separate control, download and upload policies.
    pub fn google(
        cancel: CancellationToken,
        progress: Arc<dyn Fn(TransferProgress) + Send + Sync>,
    ) -> Result<Self, reqwest::Error> {
        let clients = DriveHttpPolicy::default().build()?;
        Ok(Self::with_clients(
            clients,
            Url::parse("https://www.googleapis.com/drive/v3/files").expect("static URL"),
            Url::parse("https://www.googleapis.com/upload/drive/v3/files").expect("static URL"),
            Url::parse("https://www.googleapis.com/drive/v2/files").expect("static URL"),
            Url::parse("https://www.googleapis.com/upload/drive/v2/files").expect("static URL"),
            cancel,
            progress,
        ))
    }
    pub fn with_clients(
        clients: DriveHttpClients,
        files_v3: Url,
        upload_v3: Url,
        files_v2: Url,
        upload_v2: Url,
        cancel: CancellationToken,
        progress: Arc<dyn Fn(TransferProgress) + Send + Sync>,
    ) -> Self {
        Self {
            download: DriveDownload::new(clients.transfer.clone(), files_v3),
            upload: ResumableUpload::with_transfer_clients(
                clients.metadata.clone(),
                clients.upload.clone(),
                clients.empty_upload.clone(),
                upload_v3,
            )
            .with_response_inactivity(clients.inactivity),
            conditional: ConditionalUpdate::with_transfer_clients(
                clients.metadata,
                clients.upload,
                clients.empty_upload,
                files_v2,
                upload_v2,
            )
            .with_response_inactivity(clients.inactivity),
            cancel,
            progress,
        }
    }
    /// Reconcile a session retained by `UploadError` before reattempting creation.
    pub fn resumable(&self) -> &ResumableUpload {
        &self.upload
    }
}
impl CloudSaveTransfer for DriveTransfer {
    type Error = TransferError;
    async fn download(
        &self,
        token: &str,
        id: &str,
        destination: &str,
        modified: Option<Timestamp>,
        name: &str,
    ) -> Result<(), Self::Error> {
        self.download
            .download_with(
                token,
                id,
                Path::new(destination),
                &self.cancel,
                DownloadOptions {
                    modified,
                    progress_name: name,
                    on_progress: self.progress.as_ref(),
                },
            )
            .await
            .map_err(TransferError::Download)
    }
    async fn upload(
        &self,
        token: &str,
        source: &str,
        name: &str,
        parent: &str,
    ) -> Result<RemoteState, Self::Error> {
        let result = self
            .upload
            .upload_with_progress(
                token,
                Path::new(source),
                name,
                parent,
                &self.cancel,
                self.progress.as_ref(),
            )
            .await
            .map_err(TransferError::Upload)?;
        RemoteState::try_from(result)
    }
    async fn replace(
        &self,
        token: &str,
        observed: &RemoteState,
        source: &str,
        name: &str,
    ) -> Result<RemoteState, Self::Error> {
        if self.cancel.is_cancelled() {
            return Err(TransferError::Cancelled);
        }
        let revision = FileRevision {
            id: observed.id.clone(),
            version: observed.version.clone(),
            sha256: observed.sha256.clone(),
        };
        // An interrupted in-flight PUT reports an unknown outcome for supervised recovery.
        let result = self
            .conditional
            .update_with_cancel(
                token,
                &revision,
                Path::new(source),
                name,
                self.progress.clone(),
                &self.cancel,
            )
            .await
            .map_err(TransferError::Conditional)?;
        let Some(modified) = timestamp(&result.modified_date) else {
            return Err(TransferError::InvalidUpdatedTimestamp(Box::new(result)));
        };
        Ok(RemoteState {
            id: result.id,
            modified,
            sha256: result.sha256_checksum,
            version: result.version,
        })
    }
}
