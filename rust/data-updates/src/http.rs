//! HTTP policy for release checks and update downloads.

use std::time::Duration;

/// Longest wait to open a connection to the release server.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Which URL schemes the release client may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseTransport {
    /// Refuse plain HTTP, including a redirect to it: a release build fetches itself
    /// only over TLS.
    HttpsOnly,
    /// Also accept plain HTTP, for a controlled release server in development.
    AllowPlainHttp,
}

/// Client for [`GitHubUpdateSource`](crate::GitHubUpdateSource) and its downloads.
///
/// It sets no read or overall timeout: the release check has its own overall limit, and
/// [`UpdateDownloader`](crate::UpdateDownloader) fails a download that stops receiving
/// data, without limiting how long a download that progresses may take.
pub fn release_client(transport: ReleaseTransport) -> reqwest::Result<reqwest::Client> {
    release_client_builder(transport).build()
}

/// The policy of [`release_client`], for a caller that must add to it, such as a test
/// that keeps the environment's proxy away from a local server.
pub fn release_client_builder(transport: ReleaseTransport) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .https_only(transport == ReleaseTransport::HttpsOnly)
        .connect_timeout(CONNECT_TIMEOUT)
}
