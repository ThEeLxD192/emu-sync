use crate::TransferProgress;
use reqwest::{Client, StatusCode, Url};
use std::{fmt, io, path::Path};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub enum DownloadError {
    Cancelled,
    Http(StatusCode),
    Transport(reqwest::Error),
    Io(io::Error),
    InvalidEndpoint,
    Worker,
}
impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "download cancelled"),
            Self::Http(s) => write!(f, "Drive returned {s}"),
            Self::Transport(e) => write!(f, "Drive download failed: {e}"),
            Self::Io(e) => write!(f, "download file failed: {e}"),
            Self::InvalidEndpoint => write!(f, "invalid Drive endpoint"),
            Self::Worker => write!(f, "download worker failed"),
        }
    }
}
impl std::error::Error for DownloadError {}
impl From<reqwest::Error> for DownloadError {
    fn from(e: reqwest::Error) -> Self {
        Self::Transport(e)
    }
}
impl From<io::Error> for DownloadError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub struct DownloadOptions<'a> {
    pub modified: Option<i128>,
    pub progress_name: &'a str,
    pub on_progress: &'a (dyn Fn(TransferProgress) + Send + Sync),
}

#[derive(Clone)]
pub struct DriveDownload {
    client: Client,
    files_url: Url,
}
impl DriveDownload {
    pub fn new(client: Client, files_url: Url) -> Self {
        Self { client, files_url }
    }
    /// Cancellation must be signalled with `cancel`, and the returned future awaited to completion.
    /// The caller is responsible for validating and protecting the destination directory.
    pub async fn download(
        &self,
        token: &str,
        id: &str,
        destination: &Path,
        cancel: &CancellationToken,
    ) -> Result<(), DownloadError> {
        self.download_with(
            token,
            id,
            destination,
            cancel,
            DownloadOptions {
                modified: None,
                progress_name: "",
                on_progress: &|_| {},
            },
        )
        .await
    }
    /// Report received bytes and apply an optional nanosecond timestamp after publication.
    pub async fn download_with(
        &self,
        token: &str,
        id: &str,
        destination: &Path,
        cancel: &CancellationToken,
        options: DownloadOptions<'_>,
    ) -> Result<(), DownloadError> {
        let mut url = self.files_url.clone();
        url.path_segments_mut()
            .map_err(|_| DownloadError::InvalidEndpoint)?
            .pop_if_empty()
            .push(id);
        let request = self
            .client
            .get(url)
            .bearer_auth(token)
            .query(&[("alt", "media")])
            .send();
        let mut response = tokio::select! { biased; () = cancel.cancelled() => return Err(DownloadError::Cancelled), result = request => result? };
        if response.status() != StatusCode::OK {
            return Err(DownloadError::Http(response.status()));
        }
        let total = response.content_length();
        let mut transferred = 0u64;
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = parent.to_owned();
        let (file, temp) = tokio::task::spawn_blocking(move || -> io::Result<_> {
            std::fs::create_dir_all(&parent)?;
            Ok(tempfile::Builder::new()
                .prefix(".emusync-download-")
                .suffix(".part")
                .tempfile_in(parent)?
                .into_parts())
        })
        .await
        .map_err(|_| DownloadError::Worker)??;
        let mut file = tokio::fs::File::from_std(file);
        let result = async {
            loop {
                let chunk = tokio::select! { biased; () = cancel.cancelled() => return Err(DownloadError::Cancelled), result = response.chunk() => result? };
                match chunk {
                    Some(bytes) => {
                        file.write_all(&bytes).await?;
                        transferred = transferred.saturating_add(bytes.len() as u64);
                        (options.on_progress)(TransferProgress { name: options.progress_name.to_owned(), transferred, total });
                    }
                    None => break
                }
            }
            file.flush().await?;
            if cancel.is_cancelled() { return Err(DownloadError::Cancelled); }
            Ok(())
        }.await;
        // Wait for any pending file operation before closing and removing/publishing the stage.
        let flush = file.flush().await;
        let file = file.into_std().await;
        let destination = destination.to_owned();
        let cancellation = cancel.clone();
        let modified = options.modified;
        tokio::task::spawn_blocking(move || -> Result<(), DownloadError> {
            if let Err(error) = result {
                drop(file);
                temp.close()?;
                return Err(error);
            }
            if let Err(error) = flush {
                drop(file);
                temp.close()?;
                return Err(error.into());
            }
            if cancellation.is_cancelled() {
                drop(file);
                temp.close()?;
                return Err(DownloadError::Cancelled);
            }
            if let Some(nanos) = modified {
                let seconds = i64::try_from(nanos.div_euclid(1_000_000_000));
                let fraction = u32::try_from(nanos.rem_euclid(1_000_000_000));
                if let (Ok(seconds), Ok(fraction)) = (seconds, fraction) {
                    let _ = filetime::set_file_handle_times(
                        &file,
                        None,
                        Some(filetime::FileTime::from_unix_time(seconds, fraction)),
                    );
                }
            }
            drop(file);
            temp.persist(&destination)
                .map_err(|e| DownloadError::Io(e.error))?;
            Ok(())
        })
        .await
        .map_err(|_| DownloadError::Worker)?
    }
}
