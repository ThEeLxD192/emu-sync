use reqwest::{Client, ClientBuilder, redirect::Policy};
use serde::de::DeserializeOwned;
use std::{fmt, time::Duration};

const MAX_RESPONSE_METADATA_BYTES: usize = 1024 * 1024;
const INITIAL_METADATA_CAP: usize = 64 * 1024;

#[derive(Debug)]
pub enum ResponseMetadataError {
    Transport(reqwest::Error),
    InvalidJson(serde_json::Error),
    Inactive,
    TooLarge,
}
impl fmt::Display for ResponseMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => write!(f, "response read failed: {error}"),
            Self::InvalidJson(error) => write!(f, "invalid response metadata: {error}"),
            Self::Inactive => write!(f, "response metadata stopped arriving"),
            Self::TooLarge => write!(f, "response metadata exceeds the permitted size"),
        }
    }
}
impl std::error::Error for ResponseMetadataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            Self::InvalidJson(error) => Some(error),
            Self::Inactive | Self::TooLarge => None,
        }
    }
}

pub(crate) async fn read_metadata<T: DeserializeOwned>(
    mut response: reqwest::Response,
    inactivity: Duration,
) -> Result<T, ResponseMetadataError> {
    let initial_cap = response
        .content_length()
        .and_then(|cl| usize::try_from(cl).ok())
        .map_or(0, |cl| {
            cl.min(MAX_RESPONSE_METADATA_BYTES)
                .min(INITIAL_METADATA_CAP)
        });
    let mut body = Vec::with_capacity(initial_cap);
    loop {
        let chunk = tokio::time::timeout(inactivity, response.chunk())
            .await
            .map_err(|_| ResponseMetadataError::Inactive)?
            .map_err(ResponseMetadataError::Transport)?;
        match chunk {
            Some(chunk) => {
                if chunk.len() > MAX_RESPONSE_METADATA_BYTES.saturating_sub(body.len()) {
                    return Err(ResponseMetadataError::TooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            None => {
                return serde_json::from_slice(&body).map_err(ResponseMetadataError::InvalidJson);
            }
        }
    }
}

pub(crate) fn escape_query(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains(['\\', '\'']) {
        std::borrow::Cow::Borrowed(s)
    } else {
        let mut escaped = String::with_capacity(s.len() + 4);
        for c in s.chars() {
            match c {
                '\\' => escaped.push_str("\\\\"),
                '\'' => escaped.push_str("\\'"),
                _ => escaped.push(c),
            }
        }
        std::borrow::Cow::Owned(escaped)
    }
}

pub(crate) fn is_valid_id(id: &str) -> bool {
    !id.trim().is_empty() && id != "." && id != ".." && !id.contains(['/', '\\', '\0'])
}

pub(crate) fn item_url<E>(
    base: &reqwest::Url,
    id: &str,
    invalid_id: E,
    invalid_endpoint: E,
) -> Result<reqwest::Url, E> {
    if !is_valid_id(id) {
        return Err(invalid_id);
    }
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| invalid_endpoint)?
        .pop_if_empty()
        .push(id);
    Ok(url)
}

/// Wraps an upload body so that its hand-over to the HTTP stack can be observed.
/// The signal fires when the chunk containing the last of `length` declared bytes
/// is yielded, or immediately for an empty body. hyper stops polling a body once
/// the declared length is written, so the end of the stream is not observable.
/// Earlier chunks have already been accepted by the transport's write buffer.
pub(crate) fn observe_body_end<S>(
    body: S,
    length: u64,
) -> (reqwest::Body, tokio::sync::watch::Receiver<bool>)
where
    S: futures_util::Stream<Item = std::io::Result<bytes::Bytes>> + Send + 'static,
{
    use futures_util::StreamExt;
    let (tx, rx) = tokio::sync::watch::channel(length == 0);
    let mut seen = 0u64;
    let body = body.map(move |chunk| {
        if let Ok(bytes) = &chunk {
            seen = seen.saturating_add(bytes.len() as u64);
            if seen >= length && !*tx.borrow() {
                tx.send_replace(true);
            }
        }
        chunk
    });
    (reqwest::Body::wrap_stream(body), rx)
}

/// The response headers did not arrive within the limit after the body ended.
#[derive(Debug)]
pub(crate) struct HeadersTimedOut;

/// Await a request, limiting only the silent wait that starts after its body has
/// ended. Time spent producing or transmitting body data is never limited here.
pub(crate) async fn await_headers_after_body<F>(
    request: F,
    body_end: tokio::sync::watch::Receiver<bool>,
    limit: Duration,
) -> Result<Result<reqwest::Response, reqwest::Error>, HeadersTimedOut>
where
    F: std::future::Future<Output = Result<reqwest::Response, reqwest::Error>>,
{
    let mut body_end = body_end;
    let deadline = async {
        if body_end.wait_for(|ended| *ended).await.is_err() {
            return std::future::pending::<()>().await;
        }
        tokio::time::sleep(limit).await;
    };
    tokio::pin!(request, deadline);
    tokio::select! {
        result = &mut request => Ok(result),
        () = &mut deadline => Err(HeadersTimedOut),
    }
}

/// Separate control, download and upload policies. reqwest's read timeout also
/// runs before response headers, so it cannot safely limit long active uploads.
/// On Linux, TCP bounds unacknowledged or zero-window stalled upload data; this
/// does not limit the wait for response headers while the body is still being
/// handed over. Once the body ends, the upload's `response_inactivity` limits the
/// silent wait; empty uploads also use a header read timeout, and response
/// metadata uses a per-chunk limit.
#[derive(Clone, Copy, Debug)]
pub struct DriveHttpPolicy {
    pub connect: Duration,
    pub metadata: Duration,
    pub inactivity: Duration,
}
impl Default for DriveHttpPolicy {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(3),
            metadata: Duration::from_secs(15),
            inactivity: Duration::from_secs(60),
        }
    }
}
#[derive(Clone)]
pub struct DriveHttpClients {
    pub metadata: Client,
    pub transfer: Client,
    pub upload: Client,
    pub empty_upload: Client,
    pub inactivity: Duration,
}
impl DriveHttpPolicy {
    pub fn build(self) -> Result<DriveHttpClients, reqwest::Error> {
        self.build_with(Client::builder)
    }
    /// Apply the production policy to independently created clients, such as
    /// clients configured with an isolated test certificate authority.
    pub fn build_with(
        self,
        builder: impl Fn() -> ClientBuilder,
    ) -> Result<DriveHttpClients, reqwest::Error> {
        let base = || {
            builder()
                .connect_timeout(self.connect)
                .redirect(Policy::none())
                .retry(reqwest::retry::never())
        };
        // The TCP timeout is connection-scoped; avoid multiplexing unrelated
        // uploads on HTTP/2, where activity in one stream can hide another stall.
        let upload = || {
            let builder = base().http1_only();
            #[cfg(target_os = "linux")]
            let builder = builder.tcp_user_timeout(self.inactivity);
            builder
        };
        Ok(DriveHttpClients {
            metadata: base()
                .read_timeout(self.inactivity)
                .timeout(self.metadata)
                .build()?,
            transfer: base().read_timeout(self.inactivity).build()?,
            upload: upload().build()?,
            empty_upload: upload().read_timeout(self.inactivity).build()?,
            inactivity: self.inactivity,
        })
    }
}
