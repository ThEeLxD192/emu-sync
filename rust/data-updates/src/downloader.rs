use crate::error::UpdateError;
use futures_util::StreamExt;
use reqwest::header::USER_AGENT;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

struct CleanupGuard {
    path: Option<PathBuf>,
}

impl CleanupGuard {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) -> Option<PathBuf> {
        self.path.take()
    }
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Clone, Debug)]
pub struct UpdateDownloader {
    client: reqwest::Client,
}

impl UpdateDownloader {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub async fn download(
        &self,
        download_url: &str,
        destination: &Path,
        mut on_progress: impl FnMut(f32) + Send,
    ) -> Result<(), UpdateError> {
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        tokio::fs::create_dir_all(parent).await?;

        let file_name = destination
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("update");
        let random_suffix: u64 = {
            use std::time::SystemTime;
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(12345)
        };
        let staged_path = parent.join(format!(".{file_name}.{random_suffix}.part"));
        let mut cleanup = CleanupGuard::new(staged_path.clone());

        let response = self
            .client
            .get(download_url)
            .header(USER_AGENT, "EmuSync-Desktop-App")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(UpdateError::Http(format!(
                "HTTP {} {}",
                response.status().as_u16(),
                response.status().canonical_reason().unwrap_or("")
            )));
        }

        let total_bytes = response.content_length();
        let mut stream = response.bytes_stream();

        let mut file = tokio::fs::File::create(&staged_path).await?;
        let mut bytes_read = 0u64;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if !chunk.is_empty() {
                file.write_all(&chunk).await?;
                bytes_read += chunk.len() as u64;

                if let Some(total) = total_bytes
                    && total > 0
                {
                    let progress = (bytes_read as f32 / total as f32).clamp(0.0, 1.0);
                    on_progress(progress);
                }
            }
        }

        file.flush().await?;
        file.sync_all().await?;
        drop(file);

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

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&staged_path, std::fs::Permissions::from_mode(0o755));
        }

        std::fs::rename(&staged_path, destination)?;
        cleanup.disarm();

        on_progress(1.0);
        Ok(())
    }
}
