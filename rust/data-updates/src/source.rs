use crate::{
    downloader::UpdateDownloader,
    error::UpdateError,
    github::{GitHubRelease, find_best_asset_for_platform, is_newer_version},
};
use emusync_domain::update::{UpdateInfo, UpdateSource};
use reqwest::header::{ACCEPT, USER_AGENT};
use std::{future::Future, path::Path};

#[derive(Clone, Debug)]
pub struct GitHubUpdateSource {
    client: reqwest::Client,
    owner: String,
    repo: String,
    current_version: String,
    is_windows: bool,
    base_url: Option<String>,
    downloader: UpdateDownloader,
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
        }
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
            let response = match client
                .get(&url)
                .header(ACCEPT, "application/vnd.github+json")
                .header(USER_AGENT, "EmuSync-Desktop-App")
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(_) => return Ok(None),
            };

            if !response.status().is_success() {
                return Ok(None);
            }

            let release: GitHubRelease = match response.json().await {
                Ok(rel) => rel,
                Err(_) => return Ok(None),
            };

            let remote_version = release
                .tag_name
                .trim_start_matches(['v', 'V'])
                .trim()
                .to_string();

            if !is_newer_version(&current_version, &remote_version) {
                return Ok(None);
            }

            let asset = match find_best_asset_for_platform(&release.assets, is_windows) {
                Some(a) => a,
                None => return Ok(None),
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
            }))
        }
    }

    fn download(
        &self,
        info: &UpdateInfo,
        destination: &str,
        on_progress: impl FnMut(f32) + Send,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let downloader = self.downloader.clone();
        let download_url = info.download_url.clone();
        let destination_path = Path::new(destination).to_path_buf();

        async move {
            downloader
                .download(&download_url, &destination_path, on_progress)
                .await
        }
    }
}
