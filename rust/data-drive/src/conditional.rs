use crate::{
    ResponseMetadataError, TransferProgress,
    http::{HeadersTimedOut, await_headers_after_body, observe_body_end, read_metadata},
};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode, Url, header};
use serde::Deserialize;
use std::{fmt, future::Future, path::Path, sync::Arc, time::Duration};
use tokio_util::{io::ReaderStream, sync::CancellationToken};

const FIELDS: &str = "id,title,modifiedDate,etag,version,sha256Checksum";

/// A previously observed Drive revision. At least one comparable version or hash is required.
#[derive(Clone, Debug)]
pub struct FileRevision {
    pub id: String,
    pub version: Option<String>,
    pub sha256: Option<String>,
}

/// Metadata returned by Drive after the conditional update.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadedFile {
    pub id: String,
    pub title: String,
    pub modified_date: String,
    pub version: Option<String>,
    pub sha256_checksum: Option<String>,
    etag: Option<String>,
}

#[derive(Debug)]
pub enum ConditionalError {
    Changed,
    MissingStrongEtag,
    UnexpectedResponse(Box<UploadedFile>),
    ServerOutcomeUnknown(StatusCode),
    Http(StatusCode),
    Transport(reqwest::Error),
    OutcomeUnknown(reqwest::Error),
    /// No response headers arrived after the whole body had been handed to the transport.
    HeadersTimedOut,
    /// Cancellation interrupted a request whose remote outcome is not known.
    CancelledOutcomeUnknown,
    ResponseMetadata(ResponseMetadataError),
    ControlMetadata(ResponseMetadataError),
    Io(std::io::Error),
    InvalidEndpoint,
    Cancelled,
}
impl fmt::Display for ConditionalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Changed => write!(f, "cloud file changed; both copies must be kept"),
            Self::MissingStrongEtag => write!(f, "cloud file has no strong ETag"),
            Self::UnexpectedResponse(_) => write!(f, "unexpected cloud file identity after update"),
            Self::ServerOutcomeUnknown(status) => {
                write!(f, "conditional update outcome is unknown after {status}")
            }
            Self::Http(status) => write!(f, "Drive returned {status}"),
            Self::Transport(e) => write!(f, "Drive transport failed: {e}"),
            Self::OutcomeUnknown(e) => write!(f, "conditional update outcome is unknown: {e}"),
            Self::HeadersTimedOut => {
                write!(
                    f,
                    "conditional update outcome is unknown: response timed out"
                )
            }
            Self::CancelledOutcomeUnknown => {
                write!(f, "conditional update cancelled; remote outcome is unknown")
            }
            Self::ResponseMetadata(error) => {
                write!(f, "conditional update outcome is unknown: {error}")
            }
            Self::ControlMetadata(error) => write!(f, "could not verify cloud metadata: {error}"),
            Self::Io(e) => write!(f, "snapshot I/O failed: {e}"),
            Self::InvalidEndpoint => write!(f, "invalid Drive endpoint"),
            Self::Cancelled => write!(f, "conditional upload cancelled"),
        }
    }
}
impl std::error::Error for ConditionalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(e) | Self::OutcomeUnknown(e) => Some(e),
            Self::ResponseMetadata(e) => Some(e),
            Self::ControlMetadata(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Changed
            | Self::MissingStrongEtag
            | Self::UnexpectedResponse(_)
            | Self::ServerOutcomeUnknown(_)
            | Self::Http(_)
            | Self::HeadersTimedOut
            | Self::CancelledOutcomeUnknown
            | Self::InvalidEndpoint
            | Self::Cancelled => None,
        }
    }
}
impl From<reqwest::Error> for ConditionalError {
    fn from(e: reqwest::Error) -> Self {
        Self::Transport(e)
    }
}
impl From<std::io::Error> for ConditionalError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

async fn finish_or_retain_unknown<T>(
    request: impl Future<Output = Result<T, ConditionalError>>,
    cancel: &CancellationToken,
) -> Result<T, ConditionalError> {
    tokio::pin!(request);
    tokio::select! {
        result = &mut request => result,
        () = cancel.cancelled() => match tokio::time::timeout(Duration::from_secs(1), &mut request).await {
            Ok(result) => result,
            Err(_) => Err(ConditionalError::CancelledOutcomeUnknown),
        }
    }
}

/// A v2 conditional update; endpoint injection allows isolated transport tests.
#[derive(Clone)]
pub struct ConditionalUpdate {
    metadata: Client,
    transfer: Client,
    empty_transfer: Client,
    files: Url,
    uploads: Url,
    response_inactivity: Duration,
}
impl ConditionalUpdate {
    /// The client must disable redirects and implicit retries for remote mutations.
    pub fn new(client: Client, files: Url, uploads: Url) -> Self {
        Self::with_clients(client.clone(), client, files, uploads)
    }
    /// Both clients must disable redirects and implicit retries; production policy
    /// is provided by `DriveHttpPolicy`.
    pub fn with_clients(metadata: Client, transfer: Client, files: Url, uploads: Url) -> Self {
        Self::with_transfer_clients(metadata, transfer.clone(), transfer, files, uploads)
    }
    /// As above, with a separate client for uploads with no request body.
    pub fn with_transfer_clients(
        metadata: Client,
        transfer: Client,
        empty_transfer: Client,
        files: Url,
        uploads: Url,
    ) -> Self {
        Self {
            metadata,
            transfer,
            empty_transfer,
            files,
            uploads,
            response_inactivity: Duration::from_secs(60),
        }
    }
    pub fn with_response_inactivity(mut self, inactivity: Duration) -> Self {
        self.response_inactivity = inactivity;
        self
    }
    pub fn google(client: Client) -> Self {
        Self::new(
            client,
            Url::parse("https://www.googleapis.com/drive/v2/files/").expect("static URL"),
            Url::parse("https://www.googleapis.com/upload/drive/v2/files/").expect("static URL"),
        )
    }
    fn file_url(base: &Url, id: &str) -> Result<Url, ConditionalError> {
        crate::http::item_url(
            base,
            id,
            ConditionalError::InvalidEndpoint,
            ConditionalError::InvalidEndpoint,
        )
    }
    /// The source must be an immutable snapshot retained for the duration of this request.
    pub async fn update(
        &self,
        token: &str,
        observed: &FileRevision,
        snapshot: &Path,
        remote_name: &str,
    ) -> Result<UploadedFile, ConditionalError> {
        self.update_with_progress(token, observed, snapshot, remote_name, Arc::new(|_| {}))
            .await
    }
    /// Reports bytes read into the outgoing request; completion remains conditional on Drive's response.
    pub async fn update_with_progress(
        &self,
        token: &str,
        observed: &FileRevision,
        snapshot: &Path,
        remote_name: &str,
        on_progress: Arc<dyn Fn(TransferProgress) + Send + Sync>,
    ) -> Result<UploadedFile, ConditionalError> {
        self.update_with_cancel(
            token,
            observed,
            snapshot,
            remote_name,
            on_progress,
            &CancellationToken::new(),
        )
        .await
    }
    /// Cancellation before the PUT prevents a write. An in-flight PUT gets a short
    /// grace period to finish; otherwise its outcome must be reconciled by the owner.
    pub async fn update_with_cancel(
        &self,
        token: &str,
        observed: &FileRevision,
        snapshot: &Path,
        remote_name: &str,
        on_progress: Arc<dyn Fn(TransferProgress) + Send + Sync>,
        cancel: &CancellationToken,
    ) -> Result<UploadedFile, ConditionalError> {
        if cancel.is_cancelled() {
            return Err(ConditionalError::Cancelled);
        }
        let get_url = Self::file_url(&self.files, &observed.id)?;
        let put_url = Self::file_url(&self.uploads, &observed.id)?;
        let observed_version = observed.version.as_deref().filter(|v| !v.trim().is_empty());
        let observed_sha256 = observed.sha256.as_deref().filter(|h| !h.trim().is_empty());
        if observed_version.is_none() && observed_sha256.is_none() {
            return Err(ConditionalError::Changed);
        }
        let request = self
            .metadata
            .get(get_url)
            .bearer_auth(token)
            .query(&[("fields", FIELDS)])
            .send();
        let response = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(ConditionalError::Cancelled),
            result = request => result?,
        };
        if response.status() == StatusCode::NOT_FOUND {
            return Err(ConditionalError::Changed);
        }
        if response.status() != StatusCode::OK {
            return Err(ConditionalError::Http(response.status()));
        }
        let current: UploadedFile = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(ConditionalError::Cancelled),
            result = read_metadata(response, self.response_inactivity) => result.map_err(ConditionalError::ControlMetadata)?,
        };
        let current_version = current.version.as_deref().filter(|v| !v.trim().is_empty());
        let current_sha256 = current
            .sha256_checksum
            .as_deref()
            .filter(|h| !h.trim().is_empty());
        let unchanged = match (observed_version, current_version) {
            (Some(a), Some(b)) => a == b,
            _ => {
                matches!((observed_sha256, current_sha256), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b))
            }
        };
        let hash_changed = matches!((observed_sha256, current_sha256), (Some(a), Some(b)) if !a.eq_ignore_ascii_case(b));
        if !unchanged || hash_changed || current.id != observed.id || current.title != remote_name {
            return Err(ConditionalError::Changed);
        }
        let etag = current
            .etag
            .as_deref()
            .filter(|s| {
                s.len() >= 3 && s.starts_with('"') && s.ends_with('"') && !s.contains(['\r', '\n'])
            })
            .ok_or(ConditionalError::MissingStrongEtag)?;
        let file = tokio::fs::File::open(snapshot).await?;
        let length = file.metadata().await?.len();
        let progress_name = snapshot
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        let mut sent = 0u64;
        let body = ReaderStream::new(file).map(move |chunk| {
            if let Ok(bytes) = &chunk {
                sent = sent.saturating_add(bytes.len() as u64);
                on_progress(TransferProgress {
                    name: progress_name.clone(),
                    transferred: sent,
                    total: Some(length),
                });
            }
            chunk
        });
        if cancel.is_cancelled() {
            return Err(ConditionalError::Cancelled);
        }
        let client = if length == 0 {
            &self.empty_transfer
        } else {
            &self.transfer
        };
        let (request_body, body_end) = observe_body_end(body, length);
        let request = client
            .put(put_url)
            .bearer_auth(token)
            .header(header::IF_MATCH, etag)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CONTENT_LENGTH, length)
            .query(&[("uploadType", "media"), ("fields", FIELDS)])
            .body(request_body)
            .send();
        let response = finish_or_retain_unknown(
            async {
                match await_headers_after_body(request, body_end, self.response_inactivity).await {
                    Ok(result) => result.map_err(ConditionalError::OutcomeUnknown),
                    Err(HeadersTimedOut) => Err(ConditionalError::HeadersTimedOut),
                }
            },
            cancel,
        )
        .await?;
        if matches!(
            response.status(),
            StatusCode::PRECONDITION_FAILED | StatusCode::NOT_FOUND | StatusCode::CONFLICT
        ) {
            return Err(ConditionalError::Changed);
        }
        if response.status().is_server_error() {
            return Err(ConditionalError::ServerOutcomeUnknown(response.status()));
        }
        if response.status() != StatusCode::OK {
            return Err(ConditionalError::Http(response.status()));
        }
        let uploaded: UploadedFile = finish_or_retain_unknown(
            async {
                read_metadata(response, self.response_inactivity)
                    .await
                    .map_err(ConditionalError::ResponseMetadata)
            },
            cancel,
        )
        .await?;
        if uploaded.id != observed.id || uploaded.title != remote_name {
            return Err(ConditionalError::UnexpectedResponse(Box::new(uploaded)));
        }
        Ok(uploaded)
    }
}
