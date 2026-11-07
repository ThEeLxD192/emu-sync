//! Update coordinator managing update checks, downloads, and restarts.

use emusync_domain::update::{UpdateFiles, UpdateInfo, UpdateInstaller, UpdateSource};
use std::fmt::Display;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateState {
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
    Error(String),
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
        if !self.can_check() {
            return;
        }
        let request = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let previous = match self.state() {
            UpdateState::Checking => UpdateState::Idle,
            other => other,
        };
        self.publish(UpdateState::Checking);

        struct CancelGuard<
            'a,
            S: UpdateSource + Sync,
            F: UpdateFiles + Sync,
            I: UpdateInstaller + Sync,
        > {
            coord: &'a UpdateCoordinator<S, F, I>,
            request: u64,
            previous: UpdateState,
            completed: bool,
        }
        impl<'a, S: UpdateSource + Sync, F: UpdateFiles + Sync, I: UpdateInstaller + Sync> Drop
            for CancelGuard<'a, S, F, I>
        {
            fn drop(&mut self) {
                if !self.completed
                    && self.coord.generation.load(Ordering::SeqCst) == self.request
                    && !self.coord.download_busy.load(Ordering::SeqCst)
                {
                    self.coord.publish(self.previous.clone());
                }
            }
        }

        let mut guard = CancelGuard {
            coord: self,
            request,
            previous,
            completed: false,
        };

        match self.source.check().await {
            Ok(info) => {
                guard.completed = true;
                if self.generation.load(Ordering::SeqCst) == request
                    && !self.download_busy.load(Ordering::SeqCst)
                {
                    self.publish(info.map_or(UpdateState::Idle, UpdateState::Available));
                }
            }
            Err(err) => {
                guard.completed = true;
                if self.generation.load(Ordering::SeqCst) == request
                    && !self.download_busy.load(Ordering::SeqCst)
                {
                    self.publish(UpdateState::Error(format!("Update check failed: {err}")));
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
        if matches!(
            *self.state.lock().unwrap(),
            UpdateState::ReadyToRestart { .. }
        ) {
            return;
        }
        if self
            .download_busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.publish(UpdateState::Downloading {
            progress: 0.0,
            info: info.clone(),
        });

        let mut destination: Option<String> = None;
        let mut keep_download = false;

        let res = async {
            let dest = self
                .files
                .prepare_destination(info)
                .await
                .map_err(|e| e.to_string())?;
            destination = Some(dest.clone());

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
                .map_err(|e| e.to_string())?;

            if info.asset_size > 0 {
                let actual_size = self
                    .files
                    .size(&dest)
                    .map_err(|e| format!("Could not read update size: {e}"))?;
                if actual_size as i64 != info.asset_size {
                    return Err(format!(
                        "The update size does not match the release: expected {} bytes, received {actual_size}",
                        info.asset_size
                    ));
                }
            }
            let is_steam = self.installer.is_steam_game_mode().unwrap_or(false);
            Ok((dest, is_steam))
        }
        .await;

        match res {
            Ok((dest, is_steam)) => {
                keep_download = true;
                self.publish(UpdateState::ReadyToRestart {
                    downloaded_path: dest,
                    is_steam_game_mode: is_steam,
                });
            }
            Err(err) => {
                self.publish(UpdateState::Error(format!(
                    "Failed to download update: {err}"
                )));
            }
        }

        if !keep_download && let Some(dest) = destination {
            let _ = self.files.delete(&dest);
        }
        self.download_busy.store(false, Ordering::Release);
    }

    pub fn restart(&self, path: &str, operation_in_progress: bool) -> bool {
        if operation_in_progress || self.download_busy.load(Ordering::SeqCst) {
            return false;
        }
        match self.installer.restart(path) {
            Ok(true) => true,
            _ => {
                self.publish(UpdateState::Error(format!(
                    "Could not restart automatically. The update file is saved at: {path}"
                )));
                false
            }
        }
    }
}
