use crate::{
    downloader::UpdateDownloader,
    error::UpdateError,
    github::{GitHubRelease, find_best_asset_for_platform, is_newer_version},
    signature::{MANIFEST_NAME, ReleaseKey, SIGNATURE_NAME, signed_sha256},
};
use emusync_domain::update::{UpdateInfo, UpdateSource};
use reqwest::header::{ACCEPT, USER_AGENT};
use std::{future::Future, path::Path, time::Duration};

/// Longest a release check may take, reading the response included.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(60);

/// Largest manifest or signature accepted; real ones are a few hundred bytes.
const SIGNED_FILE_LIMIT: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct GitHubUpdateSource {
    client: reqwest::Client,
    owner: String,
    repo: String,
    current_version: String,
    is_windows: bool,
    base_url: Option<String>,
    downloader: UpdateDownloader,
    release_key: ReleaseKey,
}

impl GitHubUpdateSource {
    pub fn new(
        client: reqwest::Client,
        owner: impl Into<String>,
        repo: impl Into<String>,
        current_version: impl Into<String>,
        is_windows: bool,
    ) -> Self {
        let downloader = UpdateDownloader::new(client.clone());
        Self {
            client,
            owner: owner.into(),
            repo: repo.into(),
            current_version: current_version.into(),
            is_windows,
            base_url: None,
            downloader,
            release_key: ReleaseKey::emusync(),
        }
    }

    /// Replaces the key release manifests must be signed with, for a controlled release
    /// server.
    pub fn with_release_key(mut self, release_key: ReleaseKey) -> Self {
        self.release_key = release_key;
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }
}

impl UpdateSource for GitHubUpdateSource {
    type Error = UpdateError;

    fn check(&self) -> impl Future<Output = Result<Option<UpdateInfo>, Self::Error>> + Send {
        let client = self.client.clone();
        let base_url = self
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.github.com".to_string());
        let owner = self.owner.clone();
        let repo = self.repo.clone();
        let current_version = self.current_version.clone();
        let is_windows = self.is_windows;

        async move {
            let url = format!("{base_url}/repos/{owner}/{repo}/releases/latest");
            // A check that could not be made is an error, never "no newer version": the
            // user who asked must not be told they are up to date.
            let response = client
                .get(&url)
                .header(ACCEPT, "application/vnd.github+json")
                .header(USER_AGENT, "EmuSync-Desktop-App")
                .timeout(CHECK_TIMEOUT)
                .send()
                .await?;

            let status = response.status();
            // GitHub answers 404 when the repository has no published release yet.
            if status == reqwest::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if status != reqwest::StatusCode::OK {
                return Err(UpdateError::Http(status));
            }

            // Reading the body can fail like any transfer; a body that arrives whole but
            // is not a release is invalid data, not a network failure.
            let body = response.bytes().await?;
            let release: GitHubRelease = serde_json::from_slice(&body)
                .map_err(|error| UpdateError::InvalidRelease(error.to_string()))?;

            // Paridad con Kotlin: `removePrefix("v").removePrefix("V")` quita como máximo
            // un prefijo de cada tipo, en ese orden.
            let tag = release.tag_name.as_str();
            let tag = tag.strip_prefix('v').unwrap_or(tag);
            let tag = tag.strip_prefix('V').unwrap_or(tag);
            let remote_version = tag.trim().to_string();

            if !is_newer_version(&current_version, &remote_version) {
                return Ok(None);
            }

            let asset = match find_best_asset_for_platform(&release.assets, is_windows) {
                Some(a) => a,
                None => return Ok(None),
            };
            // A newer release that cannot be verified is not offered as an update.
            let signed_file_url = |name: &str| {
                release
                    .assets
                    .iter()
                    .find(|asset| asset.name == name)
                    .map(|asset| asset.browser_download_url.clone())
            };
            let (Some(manifest_url), Some(signature_url)) = (
                signed_file_url(MANIFEST_NAME),
                signed_file_url(SIGNATURE_NAME),
            ) else {
                return Err(UpdateError::Verification(format!(
                    "release {} is not signed",
                    release.tag_name
                )));
            };

            let release_notes = release
                .body
                .or(release.name)
                .unwrap_or_else(|| format!("New version {remote_version} available."));

            Ok(Some(UpdateInfo {
                version: remote_version,
                tag_name: release.tag_name,
                release_notes,
                download_url: asset.browser_download_url.clone(),
                asset_size: asset.size,
                asset_name: asset.name.clone(),
                manifest_url,
                signature_url,
            }))
        }
    }

    fn download(
        &self,
        info: &UpdateInfo,
        destination: &str,
        on_progress: impl FnMut(f32) + Send,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let client = self.client.clone();
        let downloader = self.downloader.clone();
        let release_key = self.release_key.clone();
        let info = info.clone();
        let destination_path = Path::new(destination).to_path_buf();

        async move {
            // Verify before downloading, so an unsigned asset is never fetched.
            let manifest = fetch_signed_file(&client, &info.manifest_url).await?;
            let signature = fetch_signed_file(&client, &info.signature_url).await?;
            let sha256 = signed_sha256(
                &release_key,
                &info.tag_name,
                &info.asset_name,
                &manifest,
                &signature,
            )?;
            downloader
                .download_verified(&info.download_url, &destination_path, sha256, on_progress)
                .await
        }
    }
}

/// The bytes of a release's manifest or signature, refused beyond [`SIGNED_FILE_LIMIT`].
async fn fetch_signed_file(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, UpdateError> {
    let response = client
        .get(url)
        .header(USER_AGENT, "EmuSync-Desktop-App")
        .timeout(CHECK_TIMEOUT)
        .send()
        .await?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(UpdateError::Http(response.status()));
    }
    let too_large = || {
        UpdateError::Verification(format!(
            "the release manifest or signature is larger than {SIGNED_FILE_LIMIT} bytes"
        ))
    };
    if response
        .content_length()
        .is_some_and(|length| length > SIGNED_FILE_LIMIT as u64)
    {
        return Err(too_large());
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = futures_util::StreamExt::next(&mut stream).await {
        body.extend_from_slice(&chunk?);
        if body.len() > SIGNED_FILE_LIMIT {
            return Err(too_large());
        }
    }
    Ok(body)
}
