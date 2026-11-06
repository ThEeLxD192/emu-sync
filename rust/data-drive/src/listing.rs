use crate::{ResponseMetadataError, http::read_metadata};
use emusync_domain::sync::{CloudFileListing, RemoteFile};
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use std::{collections::HashSet, fmt, time::Duration};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Debug)]
pub enum ListingError {
    Transport(reqwest::Error),
    /// The response body was too large, stalled or not valid metadata.
    Metadata(ResponseMetadataError),
    Http(StatusCode),
    Timestamp,
    RepeatedPage,
    InvalidEndpoint,
    InvalidIdentity,
}
impl fmt::Display for ListingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "Drive request failed: {e}"),
            Self::Metadata(e) => write!(f, "Drive response was unusable: {e}"),
            Self::Http(s) => write!(f, "Drive returned {s}"),
            Self::Timestamp => write!(f, "invalid Drive timestamp"),
            Self::RepeatedPage => write!(f, "Drive repeated a page token"),
            Self::InvalidEndpoint => write!(f, "invalid Drive endpoint"),
            Self::InvalidIdentity => write!(f, "Drive returned a file without a usable identity"),
        }
    }
}
impl std::error::Error for ListingError {}
impl From<reqwest::Error> for ListingError {
    fn from(e: reqwest::Error) -> Self {
        Self::Transport(e)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    id: String,
    name: String,
    modified_time: String,
    mime_type: Option<String>,
    sha256_checksum: Option<String>,
    version: Option<String>,
    #[serde(default)]
    parents: Vec<String>,
}
impl TryFrom<File> for RemoteFile {
    type Error = ListingError;
    fn try_from(file: File) -> Result<Self, Self::Error> {
        if file.id.trim().is_empty() {
            return Err(ListingError::InvalidIdentity);
        }
        Ok(Self {
            id: file.id,
            name: file.name,
            modified: OffsetDateTime::parse(&file.modified_time, &Rfc3339)
                .map_err(|_| ListingError::Timestamp)?
                .unix_timestamp_nanos(),
            is_folder: file.mime_type.as_deref() == Some("application/vnd.google-apps.folder"),
            sha256: file.sha256_checksum,
            version: file.version,
        })
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    #[serde(default)]
    files: Vec<File>,
    next_page_token: Option<String>,
}
/// Drive v3 listing, with an injectable files endpoint for isolated testing.
pub struct DriveListing {
    client: Client,
    files_url: Url,
    response_inactivity: Duration,
}
impl DriveListing {
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
    async fn read<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, ListingError> {
        read_metadata(response, self.response_inactivity)
            .await
            .map_err(ListingError::Metadata)
    }
    /// Find the newest exact name in the optional parent folder.
    pub async fn find_by_name(
        &self,
        token: &str,
        name: &str,
        parent: Option<&str>,
    ) -> Result<Option<RemoteFile>, ListingError> {
        let escape = |value: &str| value.replace('\\', "\\\\").replace('\'', "\\'");
        let mut query = format!("name = '{}' and trashed = false", escape(name));
        if let Some(parent) = parent {
            query.push_str(&format!(" and '{}' in parents", escape(parent)));
        }
        let response = self
            .client
            .get(self.files_url.clone())
            .bearer_auth(token)
            .query(&[
                ("q", query.as_str()),
                (
                    "fields",
                    "files(id,name,modifiedTime,sha256Checksum,version)",
                ),
                ("orderBy", "modifiedTime desc"),
                ("pageSize", "1"),
            ])
            .send()
            .await?;
        if response.status() != StatusCode::OK {
            return Err(ListingError::Http(response.status()));
        }
        self.read::<Page>(response)
            .await?
            .files
            .into_iter()
            .next()
            .map(RemoteFile::try_from)
            .transpose()
    }
    /// Fetch metadata for a single Drive ID.
    pub async fn get_by_id(&self, token: &str, id: &str) -> Result<RemoteFile, ListingError> {
        let mut url = self.files_url.clone();
        url.path_segments_mut()
            .map_err(|_| ListingError::InvalidEndpoint)?
            .pop_if_empty()
            .push(id);
        let response = self
            .client
            .get(url)
            .bearer_auth(token)
            .query(&[(
                "fields",
                "id,name,modifiedTime,mimeType,sha256Checksum,version",
            )])
            .send()
            .await?;
        if response.status() != StatusCode::OK {
            return Err(ListingError::Http(response.status()));
        }
        RemoteFile::try_from(self.read::<File>(response).await?)
    }
    /// Fetch metadata only if the file is still in the expected parent folder.
    pub async fn get_by_id_in_parent(
        &self,
        token: &str,
        id: &str,
        parent: &str,
    ) -> Result<Option<RemoteFile>, ListingError> {
        let mut url = self.files_url.clone();
        url.path_segments_mut()
            .map_err(|_| ListingError::InvalidEndpoint)?
            .pop_if_empty()
            .push(id);
        let response = self
            .client
            .get(url)
            .bearer_auth(token)
            .query(&[(
                "fields",
                "id,name,modifiedTime,mimeType,sha256Checksum,version,parents",
            )])
            .send()
            .await?;
        if response.status() != StatusCode::OK {
            return Err(ListingError::Http(response.status()));
        }
        let file = self.read::<File>(response).await?;
        if !file.parents.iter().any(|value| value == parent) {
            return Ok(None);
        }
        RemoteFile::try_from(file).map(Some)
    }
    async fn list(
        &self,
        token: &str,
        folder: &str,
        include_folders: bool,
    ) -> Result<Vec<RemoteFile>, ListingError> {
        let query = format!(
            "'{}' in parents and trashed = false",
            folder.replace('\\', "\\\\").replace('\'', "\\'")
        );
        let mut next: Option<String> = None;
        let mut seen = HashSet::new();
        let mut files = Vec::new();
        loop {
            let mut request = self
                .client
                .get(self.files_url.clone())
                .bearer_auth(token)
                .query(&[
                    ("q", query.as_str()),
                    (
                        "fields",
                        "nextPageToken,files(id,name,modifiedTime,mimeType,sha256Checksum,version)",
                    ),
                    ("orderBy", "modifiedTime desc"),
                    ("pageSize", "1000"),
                ]);
            if let Some(page) = &next {
                request = request.query(&[("pageToken", page)]);
            }
            let response = request.send().await?;
            if response.status() != StatusCode::OK {
                return Err(ListingError::Http(response.status()));
            }
            let page: Page = self.read(response).await?;
            for file in page.files {
                let file = RemoteFile::try_from(file)?;
                if include_folders || !file.is_folder {
                    files.push(file);
                }
            }
            next = page.next_page_token.filter(|s| !s.trim().is_empty());
            match &next {
                None => return Ok(files),
                Some(token) if !seen.insert(token.clone()) => {
                    return Err(ListingError::RepeatedPage);
                }
                Some(_) => {}
            }
        }
    }
}
impl CloudFileListing for DriveListing {
    type Error = ListingError;
    async fn list_files(&self, token: &str, folder: &str) -> Result<Vec<RemoteFile>, Self::Error> {
        self.list(token, folder, false).await
    }
    async fn list_children(
        &self,
        token: &str,
        folder: &str,
    ) -> Result<Vec<RemoteFile>, Self::Error> {
        self.list(token, folder, true).await
    }
}
