use crate::{
    ResponseMetadataError, TransferProgress,
    http::{HeadersTimedOut, await_headers_after_body, observe_body_end, read_metadata},
};
use reqwest::{Client, StatusCode, Url, header};
use serde::Deserialize;
use serde_json::json;
use std::{fmt, io, path::Path, time::Duration};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::sync::CancellationToken;

const CHUNK: u64 = 8 * 1024 * 1024;
/// Size of the pieces in which an upload chunk is handed to the HTTP stack.
const BODY_SLICE: usize = 64 * 1024;
const FIELDS: &str = "id,name,modifiedTime,sha256Checksum,version,parents";
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedFile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub modified_time: String,
    pub sha256_checksum: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub parents: Vec<String>,
}
/// Opaque session capability. Treat it as a secret and do not log it.
#[derive(Clone)]
pub struct UploadSession {
    url: Url,
    total: u64,
    expected_name: String,
}
impl fmt::Debug for UploadSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadSession")
            .field("total", &self.total)
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub enum ReconcileStatus {
    Complete(CreatedFile),
    Incomplete {
        received: u64,
    },
    /// The session is unavailable; this does not prove that no file was created.
    ExpiredOutcomeUnknown,
}
#[derive(Debug)]
pub enum UploadError {
    Cancelled,
    OutcomeUnknown(Box<UploadSession>),
    RecoveryFailed {
        session: Box<UploadSession>,
        cause: Box<UploadError>,
    },
    Http(StatusCode),
    Transport(reqwest::Error),
    Io(io::Error),
    InvalidSession,
    InvalidRange,
    NoProgress,
    ResponseMetadata(ResponseMetadataError),
    /// Drive returned an identity before all snapshot bytes were sent.
    PrematureCompletion(Box<CreatedFile>),
}
impl fmt::Display for UploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "upload cancelled"),
            Self::OutcomeUnknown(_) => write!(f, "upload interrupted; remote outcome is unknown"),
            Self::RecoveryFailed { cause, .. } => {
                write!(f, "upload status could not be verified: {cause}")
            }
            Self::Http(s) => write!(f, "Drive returned {s}"),
            Self::Transport(e) => write!(f, "Drive upload failed: {e}"),
            Self::Io(e) => write!(f, "snapshot I/O failed: {e}"),
            Self::InvalidSession => write!(f, "invalid upload session"),
            Self::InvalidRange => write!(f, "invalid upload range"),
            Self::NoProgress => write!(f, "upload made no progress"),
            Self::ResponseMetadata(error) => {
                write!(f, "upload response could not be read: {error}")
            }
            Self::PrematureCompletion(_) => {
                write!(f, "upload completed before all bytes were sent")
            }
        }
    }
}
impl std::error::Error for UploadError {}
impl From<reqwest::Error> for UploadError {
    fn from(e: reqwest::Error) -> Self {
        Self::Transport(e)
    }
}
impl From<io::Error> for UploadError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
enum Progress {
    Complete(CreatedFile),
    Offset(u64),
    Expired,
    Retry,
}
/// Resumable creation of new Drive files. Existing files must use conditional updates.
#[derive(Clone)]
pub struct ResumableUpload {
    control: Client,
    transfer: Client,
    empty_transfer: Client,
    upload_url: Url,
    response_inactivity: Duration,
}
impl ResumableUpload {
    /// The supplied client must disable redirects and implicit retries; use
    /// `DriveHttpPolicy` for production.
    pub fn new(client: Client, upload_url: Url) -> Self {
        Self::with_clients(client.clone(), client, upload_url)
    }
    /// Both clients must disable redirects and implicit retries so the adapter controls
    /// all mutation attempts. `DriveHttpPolicy` configures both requirements.
    pub fn with_clients(control: Client, transfer: Client, upload_url: Url) -> Self {
        Self::with_transfer_clients(control, transfer.clone(), transfer, upload_url)
    }
    /// As above, with a separate client for uploads with no request body.
    pub fn with_transfer_clients(
        control: Client,
        transfer: Client,
        empty_transfer: Client,
        upload_url: Url,
    ) -> Self {
        Self {
            control,
            transfer,
            empty_transfer,
            upload_url,
            response_inactivity: Duration::from_secs(60),
        }
    }
    pub fn with_response_inactivity(mut self, inactivity: Duration) -> Self {
        self.response_inactivity = inactivity;
        self
    }
    async fn start(
        &self,
        token: &str,
        name: &str,
        parent: &str,
        size: u64,
    ) -> Result<Url, UploadError> {
        let response = self
            .control
            .post(self.upload_url.clone())
            .bearer_auth(token)
            .query(&[("uploadType", "resumable"), ("fields", FIELDS)])
            .header("X-Upload-Content-Type", "application/octet-stream")
            .header("X-Upload-Content-Length", size)
            .json(&json!({"name":name,"parents":[parent]}))
            .send()
            .await?;
        if !matches!(response.status(), StatusCode::OK | StatusCode::CREATED) {
            return Err(UploadError::Http(response.status()));
        }
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(UploadError::InvalidSession)?;
        let url = self
            .upload_url
            .join(location)
            .map_err(|_| UploadError::InvalidSession)?;
        // Never send bearer credentials to a different origin supplied in a response.
        if url.origin() != self.upload_url.origin()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(UploadError::InvalidSession);
        }
        Ok(url)
    }
    async fn decode(
        response: reqwest::Response,
        total: u64,
        inactivity: Duration,
    ) -> Result<Progress, UploadError> {
        let status = response.status();
        if matches!(status, StatusCode::OK | StatusCode::CREATED) {
            return Ok(Progress::Complete(
                read_metadata(response, inactivity)
                    .await
                    .map_err(UploadError::ResponseMetadata)?,
            ));
        }
        if status == StatusCode::PERMANENT_REDIRECT {
            let Some(range) = response.headers().get(header::RANGE) else {
                return Ok(Progress::Offset(0));
            };
            let range = range.to_str().map_err(|_| UploadError::InvalidRange)?;
            let end = range
                .strip_prefix("bytes=0-")
                .ok_or(UploadError::InvalidRange)?
                .parse::<u64>()
                .map_err(|_| UploadError::InvalidRange)?;
            let next = end
                .checked_add(1)
                .filter(|n| *n <= total)
                .ok_or(UploadError::InvalidRange)?;
            return Ok(Progress::Offset(next));
        }
        if status == StatusCode::NOT_FOUND {
            return Ok(Progress::Expired);
        }
        if status.is_server_error() {
            return Ok(Progress::Retry);
        }
        Err(UploadError::Http(status))
    }
    fn recovery_error(session: &UploadSession, cause: UploadError) -> UploadError {
        UploadError::RecoveryFailed {
            session: Box::new(session.clone()),
            cause: Box::new(cause),
        }
    }
    async fn query(&self, token: &str, session: &UploadSession) -> Result<Progress, UploadError> {
        let total = session.total;
        for attempt in 0..3 {
            let result = self
                .control
                .put(session.url.clone())
                .bearer_auth(token)
                .header(header::CONTENT_LENGTH, 0)
                .header(header::CONTENT_RANGE, format!("bytes */{total}"))
                .send()
                .await;
            if let Ok(response) = result {
                match Self::decode(response, total, self.response_inactivity).await {
                    Ok(Progress::Retry)
                    | Err(UploadError::Transport(_) | UploadError::ResponseMetadata(_)) => {}
                    Ok(progress) => return Ok(progress),
                    Err(error) => {
                        return Err(UploadError::RecoveryFailed {
                            session: Box::new(session.clone()),
                            cause: Box::new(error),
                        });
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(250 << attempt)).await;
        }
        Err(UploadError::OutcomeUnknown(Box::new(session.clone())))
    }
    async fn query_cancellable(
        &self,
        token: &str,
        session: &UploadSession,
        cancel: &CancellationToken,
    ) -> Result<Progress, UploadError> {
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(UploadError::OutcomeUnknown(Box::new(session.clone()))),
            result = self.query(token, session) => result,
        }
    }
    /// Query an uncertain session. An incomplete status is an observation, not proof
    /// that a previously interrupted request cannot still finish.
    pub async fn reconcile(
        &self,
        token: &str,
        session: &UploadSession,
    ) -> Result<ReconcileStatus, UploadError> {
        match self.query(token, session).await? {
            Progress::Complete(file)
                if !file.id.trim().is_empty() && file.name == session.expected_name =>
            {
                Ok(ReconcileStatus::Complete(file))
            }
            Progress::Complete(_) => Err(UploadError::OutcomeUnknown(Box::new(session.clone()))),
            Progress::Offset(received) => Ok(ReconcileStatus::Incomplete { received }),
            Progress::Expired => Ok(ReconcileStatus::ExpiredOutcomeUnknown),
            Progress::Retry => Err(UploadError::OutcomeUnknown(Box::new(session.clone()))),
        }
    }
    /// Upload an immutable snapshot. An interrupted in-flight request returns `OutcomeUnknown`;
    /// the caller must reconcile it before retrying instead of assuming no remote file exists.
    pub async fn upload(
        &self,
        token: &str,
        snapshot: &Path,
        name: &str,
        parent: &str,
        cancel: &CancellationToken,
    ) -> Result<CreatedFile, UploadError> {
        self.upload_with_progress(token, snapshot, name, parent, cancel, &|_| {})
            .await
    }
    /// Reports bytes acknowledged by the server, including completion.
    pub async fn upload_with_progress(
        &self,
        token: &str,
        snapshot: &Path,
        name: &str,
        parent: &str,
        cancel: &CancellationToken,
        on_progress: &(dyn Fn(TransferProgress) + Send + Sync),
    ) -> Result<CreatedFile, UploadError> {
        let mut file = tokio::fs::File::open(snapshot).await?;
        let total = file.metadata().await?.len();
        let mut restarts = 0;
        loop {
            if cancel.is_cancelled() {
                return Err(UploadError::Cancelled);
            }
            // Do not drop session creation on cancellation: its remote outcome could be unknown.
            let session = UploadSession {
                url: self.start(token, name, parent, total).await?,
                total,
                expected_name: name.to_owned(),
            };
            let mut offset = 0u64;
            let mut no_progress = 0u32;
            loop {
                if cancel.is_cancelled() {
                    return Err(UploadError::Cancelled);
                }
                file.seek(io::SeekFrom::Start(offset))
                    .await
                    .map_err(|error| Self::recovery_error(&session, error.into()))?;
                let size = usize::try_from((total - offset).min(CHUNK))
                    .map_err(|_| Self::recovery_error(&session, UploadError::InvalidRange))?;
                let mut chunk = vec![0; size];
                file.read_exact(&mut chunk)
                    .await
                    .map_err(|error| Self::recovery_error(&session, error.into()))?;
                let client = if total == 0 {
                    &self.empty_transfer
                } else {
                    &self.transfer
                };
                let mut request = client
                    .put(session.url.clone())
                    .bearer_auth(token)
                    .header(header::CONTENT_TYPE, "application/octet-stream")
                    .header(header::CONTENT_LENGTH, size);
                if total > 0 {
                    request = request.header(
                        header::CONTENT_RANGE,
                        format!("bytes {}-{}/{total}", offset, offset + size as u64 - 1),
                    );
                }
                // Small slices make hyper pull the body at the pace the transport
                // accepts it, so the end-of-body signal is not raised while most of
                // a large chunk is still queued.
                let bytes = bytes::Bytes::from(chunk);
                let slices = (0..bytes.len())
                    .step_by(BODY_SLICE)
                    .map(|start| {
                        Ok::<_, io::Error>(
                            bytes.slice(start..(start + BODY_SLICE).min(bytes.len())),
                        )
                    })
                    .collect::<Vec<_>>();
                let (request_body, body_end) =
                    observe_body_end(futures_util::stream::iter(slices), size as u64);
                let request = request.body(request_body).send();
                let result = tokio::select! {
                    biased;
                    () = cancel.cancelled() => return Err(UploadError::OutcomeUnknown(Box::new(session.clone()))),
                    // A silent response after the whole chunk was handed over is an
                    // unknown outcome: the session query below decides what happened.
                    result = async {
                        match await_headers_after_body(request, body_end, self.response_inactivity).await {
                            Ok(result) => result.map_err(drop),
                            Err(HeadersTimedOut) => Err(()),
                        }
                    } => result,
                };
                let direct = match result {
                    Ok(response) => match tokio::select! {
                        biased;
                        () = cancel.cancelled() => return Err(UploadError::OutcomeUnknown(Box::new(session.clone()))),
                        result = Self::decode(response, total, self.response_inactivity) => result,
                    } {
                        Ok(progress) => Some(progress),
                        Err(UploadError::Transport(_) | UploadError::ResponseMetadata(_)) => None,
                        Err(error) => return Err(Self::recovery_error(&session, error)),
                    },
                    Err(_) => None,
                };
                let needs_query = direct.as_ref().is_none_or(|p| matches!(p, Progress::Retry));
                let mut progress = if needs_query {
                    self.query_cancellable(token, &session, cancel).await?
                } else {
                    direct.expect("non-retry direct response")
                };
                // A response cannot acknowledge bytes beyond this request. Trusting
                // such an offset would skip bytes from the immutable snapshot.
                if matches!(progress, Progress::Offset(next) if next > offset + size as u64) {
                    return Err(Self::recovery_error(&session, UploadError::InvalidRange));
                }
                // A completion response before the last snapshot byte was sent
                // cannot prove that the remote file contains the snapshot.
                if offset + size as u64 != total
                    && let Progress::Complete(file) = &progress
                {
                    return Err(Self::recovery_error(
                        &session,
                        UploadError::PrematureCompletion(Box::new(file.clone())),
                    ));
                }
                // A lost response may have committed the file before the session expired.
                // A 404 status query cannot establish that no file was created.
                if needs_query && matches!(progress, Progress::Expired) {
                    return Err(UploadError::OutcomeUnknown(Box::new(session.clone())));
                }
                // A 308 can acknowledge all bytes before metadata is available.
                // Query completion rather than resending an empty chunk or claiming success.
                if matches!(progress, Progress::Offset(next) if next == total && total > 0) {
                    progress = self.query_cancellable(token, &session, cancel).await?;
                    if !matches!(progress, Progress::Complete(_)) {
                        return Err(UploadError::OutcomeUnknown(Box::new(session.clone())));
                    }
                }
                match progress {
                    Progress::Complete(result) => {
                        if result.name != name || result.id.trim().is_empty() {
                            return Err(UploadError::OutcomeUnknown(Box::new(session.clone())));
                        }
                        on_progress(TransferProgress {
                            name: name.to_owned(),
                            transferred: total,
                            total: Some(total),
                        });
                        return Ok(result);
                    }
                    Progress::Expired if restarts == 0 => {
                        restarts += 1;
                        break;
                    }
                    Progress::Expired => {
                        return Err(Self::recovery_error(
                            &session,
                            UploadError::Http(StatusCode::NOT_FOUND),
                        ));
                    }
                    Progress::Offset(next) if next < offset || next > total => {
                        return Err(Self::recovery_error(&session, UploadError::InvalidRange));
                    }
                    Progress::Offset(next) if next == offset => {
                        no_progress += 1;
                        if no_progress > 3 {
                            return Err(Self::recovery_error(&session, UploadError::NoProgress));
                        }
                        tokio::time::sleep(Duration::from_millis(250 << (no_progress - 1))).await;
                    }
                    Progress::Offset(next) => {
                        offset = next;
                        no_progress = 0;
                        on_progress(TransferProgress {
                            name: name.to_owned(),
                            transferred: next,
                            total: Some(total),
                        });
                    }
                    Progress::Retry => {
                        return Err(UploadError::OutcomeUnknown(Box::new(session.clone())));
                    }
                }
            }
        }
    }
}
