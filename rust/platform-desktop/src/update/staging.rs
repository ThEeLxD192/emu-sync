use emusync_domain::update::{UpdateFiles, UpdateInfo};
use std::{
    fs,
    future::Future,
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct UpdateStagingFiles {
    is_windows: bool,
    current_app_image_path: Option<PathBuf>,
}

impl UpdateStagingFiles {
    pub fn new(is_windows: bool, current_app_image_path: Option<PathBuf>) -> Self {
        Self {
            is_windows,
            current_app_image_path,
        }
    }

    pub fn default_for_platform() -> Self {
        let is_windows = cfg!(windows);
        let app_image = std::env::var("APPIMAGE")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from);
        Self::new(is_windows, app_image)
    }
}

impl UpdateFiles for UpdateStagingFiles {
    type Error = io::Error;

    fn prepare_destination(
        &self,
        info: &UpdateInfo,
    ) -> impl Future<Output = Result<String, Self::Error>> + Send {
        let is_windows = self.is_windows;
        let asset_name = info.asset_name.clone();
        let current_app_image = self.current_app_image_path.clone();

        async move {
            let extension = Path::new(&asset_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or(if is_windows { "msi" } else { "AppImage" });

            let directory = if !is_windows {
                current_app_image
                    .as_ref()
                    .and_then(|p| p.parent())
                    .filter(|p| {
                        // Check if writable by creating/deleting a probe file
                        let probe = p.join(".probe_write");
                        if fs::write(&probe, b"").is_ok() {
                            let _ = fs::remove_file(&probe);
                            true
                        } else {
                            false
                        }
                    })
                    .map(Path::to_path_buf)
                    .unwrap_or_else(std::env::temp_dir)
            } else {
                std::env::temp_dir()
            };

            let random: u64 = {
                use std::time::SystemTime;
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(777)
            };

            let path = directory.join(format!(".EmuSync-update-{random}.{extension}"));
            Ok(path.to_string_lossy().to_string())
        }
    }

    fn size(&self, path: &str) -> Result<u64, Self::Error> {
        fs::metadata(path).map(|m| m.len())
    }

    fn delete(&self, path: &str) -> Result<(), Self::Error> {
        let p = Path::new(path);
        if p.exists() {
            fs::remove_file(p)?;
        }
        Ok(())
    }
}
