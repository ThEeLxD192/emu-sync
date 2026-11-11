use crate::error::UpdateError;
use futures_util::StreamExt;
use reqwest::header::USER_AGENT;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Default socket inactivity timeout during update downloads, matching
/// the Kotlin reference implementation (`UPDATE_SOCKET_TIMEOUT_MS = 60_000L`).
pub const DEFAULT_INACTIVITY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[cfg(test)]
type CandidateProvider = std::sync::Arc<dyn Fn(&Path, &str) -> Vec<PathBuf> + Send + Sync>;

/// RAII guard that manages the staging file during an update download.
///
/// Ensures exclusive ownership and drop safety:
/// - Acquired only upon successful atomic exclusive file creation (`create_new(true)`).
/// - If cancelled or dropped before publishing, handles are closed and the temporary `.part` file is deleted.
/// - Unacquired paths are never armed for cleanup.
/// - Cleanup responsibility is retained until atomic rename succeeds.
///
/// # Asynchronous Cancellation Limit
/// Dropping the outer download [`Future`] performs best-effort RAII cleanup of this staging file.
/// However, full cancellation safety (guaranteeing that all background I/O and cleanup have
/// completely finished before releasing mutual exclusion or closing runtimes) requires cooperative
/// cancellation at the coordinator/task-owner level (e.g. signaling cancellation and awaiting completion).
struct StagedFile {
    path: PathBuf,
    std_file: Option<std::fs::File>,
    async_file: Option<tokio::fs::File>,
    active: bool,
}

impl StagedFile {
    fn new_std(path: PathBuf, file: std::fs::File) -> Self {
        Self {
            path,
            std_file: Some(file),
            async_file: None,
            active: true,
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn activate_async(&mut self) {
        if let Some(file) = self.std_file.take() {
            self.async_file = Some(tokio::fs::File::from_std(file));
        }
    }

    async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        let file = self
            .async_file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("staging file is not in async mode"))?;
        file.write_all(buf).await
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        let file = self
            .async_file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("staging file is not in async mode"))?;
        file.flush().await
    }

    async fn sync_all(&mut self) -> std::io::Result<()> {
        let file = self
            .async_file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("staging file is not in async mode"))?;
        file.sync_all().await
    }

    async fn finish_async(&mut self) -> std::io::Result<()> {
        if let Some(async_file) = self.async_file.take() {
            let std_file = async_file.into_std().await;
            self.std_file = Some(std_file);
        }
        Ok(())
    }

    fn close_handles(&mut self) {
        drop(self.std_file.take());
        drop(self.async_file.take());
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        if self.active {
            drop(self.std_file.take());
            drop(self.async_file.take());
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn generate_candidate_path(parent: &Path, file_name: &str, attempt: usize) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::SystemTime;

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    parent.join(format!(".{file_name}.{nanos}_{pid}_{count}_{attempt}.part"))
}

#[derive(Clone)]
pub struct UpdateDownloader {
    client: reqwest::Client,
    inactivity_timeout: std::time::Duration,
    #[cfg(test)]
    candidate_provider: Option<CandidateProvider>,
    #[cfg(test)]
    staging_started: Option<std::sync::Arc<tokio::sync::Notify>>,
}

impl std::fmt::Debug for UpdateDownloader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateDownloader")
            .field("client", &self.client)
            .field("inactivity_timeout", &self.inactivity_timeout)
            .finish()
    }
}

impl UpdateDownloader {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            inactivity_timeout: DEFAULT_INACTIVITY_TIMEOUT,
            #[cfg(test)]
            candidate_provider: None,
            #[cfg(test)]
            staging_started: None,
        }
    }

    pub async fn download(
        &self,
        download_url: &str,
        destination: &Path,
        mut on_progress: impl FnMut(f32) + Send,
    ) -> Result<(), UpdateError> {
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        tokio::fs::create_dir_all(parent).await?;

        let file_name = destination
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("update");

        let request = self
            .client
            .get(download_url)
            .header(USER_AGENT, "EmuSync-Desktop-App");

        let response = match tokio::time::timeout(self.inactivity_timeout, request.send()).await {
            Ok(result) => result?,
            Err(_) => {
                return Err(UpdateError::Network(format!(
                    "download request timed out after {:?} of inactivity",
                    self.inactivity_timeout
                )));
            }
        };

        if response.status() != reqwest::StatusCode::OK {
            return Err(UpdateError::Http(format!(
                "HTTP {} {}",
                response.status().as_u16(),
                response.status().canonical_reason().unwrap_or("")
            )));
        }

        let total_bytes = response.content_length();
        let mut stream = response.bytes_stream();

        #[cfg(test)]
        if let Some(notify) = &self.staging_started {
            notify.notify_one();
        }

        let parent_buf = parent.to_path_buf();
        let file_name_str = file_name.to_string();
        #[cfg(test)]
        let candidate_provider = self.candidate_provider.clone();

        let mut staged_file =
            tokio::task::spawn_blocking(move || -> std::io::Result<StagedFile> {
                #[cfg(test)]
                if let Some(ref provider) = candidate_provider {
                    let candidates = provider(&parent_buf, &file_name_str);
                    for candidate in candidates {
                        match std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&candidate)
                        {
                            Ok(file) => {
                                return Ok(StagedFile::new_std(candidate, file));
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                                continue;
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }

                const MAX_DEFAULT_ATTEMPTS: usize = 32;
                for attempt in 0..MAX_DEFAULT_ATTEMPTS {
                    let candidate = generate_candidate_path(&parent_buf, &file_name_str, attempt);
                    match std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&candidate)
                    {
                        Ok(file) => {
                            return Ok(StagedFile::new_std(candidate, file));
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }

                Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "failed to create unique temporary staging file after multiple attempts",
                ))
            })
            .await
            .map_err(|e| std::io::Error::other(format!("staging worker join error: {e}")))??;

        staged_file.activate_async();
        let mut bytes_read = 0u64;

        while let Some(chunk_res) = tokio::time::timeout(self.inactivity_timeout, stream.next())
            .await
            .map_err(|_| {
                UpdateError::Network(format!(
                    "download stalled: no data received for {:?}",
                    self.inactivity_timeout
                ))
            })?
        {
            let chunk = chunk_res?;
            if !chunk.is_empty() {
                staged_file.write_all(&chunk).await?;
                bytes_read += chunk.len() as u64;

                if let Some(total) = total_bytes
                    && total > 0
                {
                    let progress = (bytes_read as f32 / total as f32).clamp(0.0, 1.0);
                    on_progress(progress);
                }
            }
        }

        staged_file.flush().await?;
        staged_file.sync_all().await?;

        if bytes_read == 0 {
            return Err(UpdateError::EmptyDownload);
        }

        if let Some(total) = total_bytes
            && bytes_read != total
        {
            return Err(UpdateError::IncompleteDownload {
                expected: total,
                received: bytes_read,
            });
        }

        staged_file.finish_async().await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                staged_file.path(),
                std::fs::Permissions::from_mode(0o755),
            );
        }

        staged_file.close_handles();
        std::fs::rename(staged_file.path(), destination)?;
        staged_file.disarm();

        on_progress(1.0);
        Ok(())
    }
}

#[cfg(test)]
impl UpdateDownloader {
    pub(crate) fn with_candidate_provider<F>(mut self, provider: F) -> Self
    where
        F: Fn(&Path, &str) -> Vec<PathBuf> + Send + Sync + 'static,
    {
        self.candidate_provider = Some(std::sync::Arc::new(provider));
        self
    }

    pub(crate) fn with_staging_started_notify(
        mut self,
        notify: std::sync::Arc<tokio::sync::Notify>,
    ) -> Self {
        self.staging_started = Some(notify);
        self
    }
}

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod tests;
