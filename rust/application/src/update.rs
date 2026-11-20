//! Update coordinator managing update checks, downloads, and restarts.

use emusync_domain::update::{UpdateFiles, UpdateInfo, UpdateInstaller, UpdateSource};
use std::fmt::Display;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Clone, Debug, Default, PartialEq)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    Available(UpdateInfo),
    Downloading {
        progress: f32,
        info: UpdateInfo,
    },
    ReadyToRestart {
        downloaded_path: String,
        is_steam_game_mode: bool,
    },
    /// A release check that could not be made; holds the detail from the source, for
    /// the interface to put in its own words.
    CheckFailed(String),
    /// A download that did not leave an update ready to install.
    DownloadFailed(DownloadFailure),
    /// The installer did not apply the update; the downloaded file is kept at
    /// `downloaded_path` so the user can install it by hand.
    RestartFailed {
        downloaded_path: String,
    },
}

/// Why a download did not leave an update ready to install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadFailure {
    /// The local file the update is downloaded to could not be prepared (for example,
    /// a folder without write permission); holds the detail.
    DestinationUnavailable(String),
    /// Transferring the file failed; holds the detail from the source.
    Transfer(String),
    /// The size of the downloaded file could not be read; holds the detail.
    SizeUnreadable(String),
    /// The downloaded file is not the size the release announces.
    SizeMismatch {
        expected_bytes: u64,
        received_bytes: u64,
    },
}

pub struct UpdateCoordinator<S, F, I> {
    source: S,
    files: F,
    installer: I,
    state: Mutex<UpdateState>,
    download_busy: AtomicBool,
    generation: AtomicU64,
    on_state: Box<dyn Fn(&UpdateState) + Send + Sync + 'static>,
}

struct DownloadGuard<'a, S: UpdateSource + Sync, F: UpdateFiles + Sync, I: UpdateInstaller + Sync> {
    coord: &'a UpdateCoordinator<S, F, I>,
    destination: Option<String>,
    keep_download: bool,
    fallback_info: UpdateInfo,
}

impl<'a, S: UpdateSource + Sync, F: UpdateFiles + Sync, I: UpdateInstaller + Sync> Drop
    for DownloadGuard<'a, S, F, I>
{
    fn drop(&mut self) {
        let mut pending_notify = None;
        if !self.keep_download {
            if let Some(ref dest) = self.destination {
                let _ = self.coord.files.delete(dest);
            }
            let mut state = self.coord.state.lock().unwrap();
            if matches!(*state, UpdateState::Downloading { .. }) {
                *state = UpdateState::Available(self.fallback_info.clone());
                pending_notify = Some(state.clone());
            }
        }
        self.coord.download_busy.store(false, Ordering::SeqCst);
        if let Some(new_state) = pending_notify {
            (self.coord.on_state)(&new_state);
        }
    }
}

struct CancelGuard<'a, S: UpdateSource + Sync, F: UpdateFiles + Sync, I: UpdateInstaller + Sync> {
    coord: &'a UpdateCoordinator<S, F, I>,
    request: u64,
    previous: Option<UpdateState>,
}

impl<'a, S: UpdateSource + Sync, F: UpdateFiles + Sync, I: UpdateInstaller + Sync> Drop
    for CancelGuard<'a, S, F, I>
{
    fn drop(&mut self) {
        if let Some(prev) = self.previous.take()
            && self.coord.generation.load(Ordering::SeqCst) == self.request
            && !self.coord.download_busy.load(Ordering::SeqCst)
        {
            self.coord.publish(prev);
        }
    }
}

impl<S, F, I> UpdateCoordinator<S, F, I>
where
    S: UpdateSource + Sync,
    F: UpdateFiles + Sync,
    I: UpdateInstaller + Sync,
{
    pub fn new(
        source: S,
        files: F,
        installer: I,
        on_state: impl Fn(&UpdateState) + Send + Sync + 'static,
    ) -> Self {
        Self {
            source,
            files,
            installer,
            state: Mutex::new(UpdateState::Idle),
            download_busy: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            on_state: Box::new(on_state),
        }
    }

    pub fn state(&self) -> UpdateState {
        self.state.lock().unwrap().clone()
    }

    fn publish(&self, new_state: UpdateState) {
        let mut lock = self.state.lock().unwrap();
        if *lock != new_state {
            *lock = new_state.clone();
            drop(lock);
            (self.on_state)(&new_state);
        }
    }

    pub fn can_check(&self) -> bool {
        !self.download_busy.load(Ordering::SeqCst)
            && !matches!(
                *self.state.lock().unwrap(),
                UpdateState::ReadyToRestart { .. }
            )
    }

    pub async fn check_for_updates(&self)
    where
        S::Error: Display,
    {
        if self.download_busy.load(Ordering::SeqCst) {
            return;
        }

        let request = self.generation.fetch_add(1, Ordering::SeqCst) + 1;

        let (previous, should_notify) = {
            let mut lock = self.state.lock().unwrap();
            if matches!(*lock, UpdateState::ReadyToRestart { .. }) {
                return;
            }
            let prev_raw = std::mem::replace(&mut *lock, UpdateState::Checking);
            let should_notify = prev_raw != UpdateState::Checking;
            let prev = match prev_raw {
                UpdateState::Checking => UpdateState::Idle,
                other => other,
            };
            (prev, should_notify)
        };

        let mut guard = CancelGuard {
            coord: self,
            request,
            previous: Some(previous),
        };

        if should_notify {
            (self.on_state)(&UpdateState::Checking);
        }

        match self.source.check().await {
            Ok(info) => {
                guard.previous = None;
                if self.generation.load(Ordering::SeqCst) == request
                    && !self.download_busy.load(Ordering::SeqCst)
                {
                    self.publish(info.map_or(UpdateState::Idle, UpdateState::Available));
                }
            }
            Err(err) => {
                guard.previous = None;
                if self.generation.load(Ordering::SeqCst) == request
                    && !self.download_busy.load(Ordering::SeqCst)
                {
                    self.publish(UpdateState::CheckFailed(err.to_string()));
                }
            }
        }
    }

    pub async fn download(&self, info: &UpdateInfo)
    where
        S::Error: Display,
        F::Error: Display,
        I::Error: Display,
    {
        if self
            .download_busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        if matches!(
            *self.state.lock().unwrap(),
            UpdateState::ReadyToRestart { .. }
        ) {
            self.download_busy.store(false, Ordering::SeqCst);
            return;
        }

        let mut guard = DownloadGuard {
            coord: self,
            destination: None,
            keep_download: false,
            fallback_info: info.clone(),
        };

        self.generation.fetch_add(1, Ordering::SeqCst);
        self.publish(UpdateState::Downloading {
            progress: 0.0,
            info: info.clone(),
        });

        let res = async {
            let dest = self
                .files
                .prepare_destination(info)
                .await
                .map_err(|e| DownloadFailure::DestinationUnavailable(e.to_string()))?;
            guard.destination = Some(dest.clone());

            let mut last_progress = 0.0f32;
            self.source
                .download(info, &dest, |p| {
                    if (p - last_progress).abs() >= 0.01 || p >= 1.0 {
                        last_progress = p;
                        self.publish(UpdateState::Downloading {
                            progress: p,
                            info: info.clone(),
                        });
                    }
                })
                .await
                .map_err(|e| DownloadFailure::Transfer(e.to_string()))?;

            if info.asset_size > 0 {
                let received_bytes = self
                    .files
                    .size(&dest)
                    .map_err(|e| DownloadFailure::SizeUnreadable(e.to_string()))?;
                let expected_bytes = info.asset_size as u64;
                if received_bytes != expected_bytes {
                    return Err(DownloadFailure::SizeMismatch {
                        expected_bytes,
                        received_bytes,
                    });
                }
            }
            let is_steam = self.installer.is_steam_game_mode().unwrap_or(false);
            Ok((dest, is_steam))
        }
        .await;

        match res {
            Ok((dest, is_steam)) => {
                guard.keep_download = true;
                self.publish(UpdateState::ReadyToRestart {
                    downloaded_path: dest,
                    is_steam_game_mode: is_steam,
                });
            }
            Err(failure) => self.publish(UpdateState::DownloadFailed(failure)),
        }
    }

    pub fn restart(&self, path: &str, operation_in_progress: bool) -> bool {
        if operation_in_progress || self.download_busy.load(Ordering::SeqCst) {
            return false;
        }
        match self.installer.restart(path) {
            Ok(true) => true,
            _ => {
                self.publish(UpdateState::RestartFailed {
                    downloaded_path: path.to_owned(),
                });
                false
            }
        }
    }
}
