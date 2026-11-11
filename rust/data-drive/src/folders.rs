use crate::{
    ResponseMetadataError,
    http::{escape_query, is_valid_id, item_url, read_metadata},
};
use emusync_domain::sync::CloudFolderStore;
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use serde_json::json;
use std::{fmt, time::Duration};

const MIME: &str = "application/vnd.google-apps.folder";
#[derive(Debug)]
pub enum FolderError {
    Transport(reqwest::Error),
    /// A read-only response body was too large, stalled or invalid.
    Metadata(ResponseMetadataError),
    MutationOutcomeUnknown(reqwest::Error),
    /// The mutation response body was too large, stalled or invalid; the outcome is unknown.
    MutationResponseUnknown(ResponseMetadataError),
    ServerOutcomeUnknown(StatusCode),
    MutationIdentityUnknown,
    InvalidFolderIdentity,
    Http(StatusCode),
    InvalidEndpoint,
}
impl fmt::Display for FolderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "Drive request failed: {e}"),
            Self::Metadata(e) => write!(f, "Drive response was unusable: {e}"),
            Self::MutationResponseUnknown(e) => {
                write!(f, "Drive mutation outcome is unknown: {e}")
            }
            Self::MutationOutcomeUnknown(e) => write!(f, "Drive mutation outcome is unknown: {e}"),
            Self::ServerOutcomeUnknown(status) => {
                write!(f, "Drive mutation outcome is unknown after {status}")
            }
            Self::MutationIdentityUnknown => write!(
                f,
                "created folder has no usable identity; outcome is unknown"
            ),
            Self::InvalidFolderIdentity => {
                write!(f, "Drive returned a folder without a usable identity")
            }
            Self::Http(s) => write!(f, "Drive returned {s}"),
            Self::InvalidEndpoint => write!(f, "invalid Drive endpoint"),
        }
    }
}
impl std::error::Error for FolderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(e) | Self::MutationOutcomeUnknown(e) => Some(e),
            Self::Metadata(e) | Self::MutationResponseUnknown(e) => Some(e),
            Self::ServerOutcomeUnknown(_)
            | Self::MutationIdentityUnknown
            | Self::InvalidFolderIdentity
            | Self::Http(_)
            | Self::InvalidEndpoint => None,
        }
    }
}
impl From<reqwest::Error> for FolderError {
    fn from(e: reqwest::Error) -> Self {
        Self::Transport(e)
    }
}
#[derive(Deserialize)]
struct Folder {
    id: String,
}
#[derive(Deserialize)]
struct FolderPage {
    #[serde(default)]
    files: Vec<Folder>,
}

/// Drive v3 folder operations. No cache is shared across OAuth connections.
#[derive(Clone)]
pub struct DriveFolders {
    client: Client,
    files_url: Url,
    response_inactivity: Duration,
}
impl DriveFolders {
    /// The client must disable redirects and implicit retries for remote mutations.
    pub fn new(client: Client, files_url: Url) -> Self {
        Self {
            client,
            files_url,
            response_inactivity: Duration::from_secs(60),
        }
    }
    /// Limit the silence between response body fragments.
    pub fn with_response_inactivity(mut self, inactivity: Duration) -> Self {
        self.response_inactivity = inactivity;
        self
    }
    fn item_url(&self, id: &str) -> Result<Url, FolderError> {
        item_url(
            &self.files_url,
            id,
            FolderError::InvalidFolderIdentity,
            FolderError::InvalidEndpoint,
        )
    }
    async fn search(
        &self,
        token: &str,
        name: &str,
        parent: Option<&str>,
    ) -> Result<Option<String>, FolderError> {
        let mut query = format!(
            "name = '{}' and mimeType = '{MIME}' and trashed = false",
            escape_query(name)
        );
        if let Some(parent) = parent {
            query.push_str(&format!(" and '{}' in parents", escape_query(parent)));
        }
        let response = self
            .client
            .get(self.files_url.clone())
            .bearer_auth(token)
            .query(&[
                ("q", query.as_str()),
                ("fields", "files(id,name)"),
                ("pageSize", "1"),
            ])
            .send()
            .await?;
        if response.status() != StatusCode::OK {
            return Err(FolderError::Http(response.status()));
        }
        read_metadata::<FolderPage>(response, self.response_inactivity)
            .await
            .map_err(FolderError::Metadata)?
            .files
            .into_iter()
            .next()
            .map(|folder| {
                if folder.id.trim().is_empty() {
                    Err(FolderError::InvalidFolderIdentity)
                } else {
                    Ok(folder.id)
                }
            })
            .transpose()
    }
    async fn create(
        &self,
        token: &str,
        name: &str,
        parent: Option<&str>,
    ) -> Result<String, FolderError> {
        let parents = match parent {
            Some(p) => &[p][..],
            None => &[][..],
        };
        let response = self
            .client
            .post(self.files_url.clone())
            .bearer_auth(token)
            .json(&json!({"name":name,"mimeType":MIME,"parents":parents}))
            .send()
            .await
            .map_err(FolderError::MutationOutcomeUnknown)?;
        if response.status().is_server_error() {
            return Err(FolderError::ServerOutcomeUnknown(response.status()));
        }
        if !matches!(response.status(), StatusCode::OK | StatusCode::CREATED) {
            return Err(FolderError::Http(response.status()));
        }
        let folder = read_metadata::<Folder>(response, self.response_inactivity)
            .await
            .map_err(FolderError::MutationResponseUnknown)?;
        if folder.id.trim().is_empty() {
            return Err(FolderError::MutationIdentityUnknown);
        }
        Ok(folder.id)
    }
    async fn path(
        &self,
        token: &str,
        segments: &[String],
        create: bool,
    ) -> Result<Option<String>, FolderError> {
        let mut parent: Option<String> = None;
        for segment in std::iter::once("EmuSync")
            .chain(segments.iter().map(String::as_str))
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let found = self.search(token, segment, parent.as_deref()).await?;
            parent = match found {
                Some(id) => Some(id),
                None if create => Some(self.create(token, segment, parent.as_deref()).await?),
                None => return Ok(None),
            };
        }
        Ok(parent)
    }
    /// Delete only on explicit request; no path or migration operation calls this implicitly.
    pub async fn delete_folder(&self, token: &str, folder: &str) -> Result<(), FolderError> {
        let response = self
            .client
            .delete(self.item_url(folder)?)
            .bearer_auth(token)
            .send()
            .await
            .map_err(FolderError::MutationOutcomeUnknown)?;
        if response.status().is_server_error() {
            return Err(FolderError::ServerOutcomeUnknown(response.status()));
        }
        if !matches!(response.status(), StatusCode::OK | StatusCode::NO_CONTENT) {
            return Err(FolderError::Http(response.status()));
        }
        Ok(())
    }
    async fn patch(
        &self,
        token: &str,
        id: &str,
        name: Option<&str>,
        add: Option<&str>,
        remove: Option<&str>,
    ) -> Result<(), FolderError> {
        let mut request = self.client.patch(self.item_url(id)?).bearer_auth(token);
        if let Some(add) = add {
            request = request.query(&[("addParents", add)]);
        }
        if let Some(remove) = remove {
            request = request.query(&[("removeParents", remove)]);
        }
        if let Some(name) = name {
            request = request.json(&json!({"name":name}));
        }
        let response = request
            .send()
            .await
            .map_err(FolderError::MutationOutcomeUnknown)?;
        if response.status().is_server_error() {
            return Err(FolderError::ServerOutcomeUnknown(response.status()));
        }
        if response.status() != StatusCode::OK {
            return Err(FolderError::Http(response.status()));
        }
        Ok(())
    }
}
impl CloudFolderStore for DriveFolders {
    type Error = FolderError;
    async fn ensure_path(&self, token: &str, segments: &[String]) -> Result<String, Self::Error> {
        self.path(token, segments, true)
            .await?
            .ok_or(FolderError::InvalidEndpoint)
    }
    async fn find_path(
        &self,
        token: &str,
        segments: &[String],
    ) -> Result<Option<String>, Self::Error> {
        self.path(token, segments, false).await
    }
    async fn rename_folder(
        &self,
        token: &str,
        folder: &str,
        name: &str,
        add: Option<&str>,
        remove: Option<&str>,
    ) -> Result<(), Self::Error> {
        if name.trim().is_empty()
            || add.is_some_and(|p| !is_valid_id(p))
            || remove.is_some_and(|p| !is_valid_id(p))
        {
            return Err(FolderError::InvalidFolderIdentity);
        }
        self.patch(token, folder, Some(name), add, remove).await
    }
    async fn move_file(
        &self,
        token: &str,
        file: &str,
        add: &str,
        remove: &str,
    ) -> Result<(), Self::Error> {
        if !is_valid_id(add) || !is_valid_id(remove) {
            return Err(FolderError::InvalidFolderIdentity);
        }
        self.patch(token, file, None, Some(add), Some(remove)).await
    }
}
