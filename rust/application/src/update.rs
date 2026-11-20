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

/// Highest progress a `Downloading` state reports; the completed download is
/// `ReadyToRestart`.
const DOWNLOADING_PROGRESS_LIMIT: f32 = 0.99;

/// The order of the states a coordinator announces: each new state has a higher
/// revision. Announcements run outside the state lock, so two of them can arrive in either
/// order; whoever shows the state keeps the one with the highest revision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UpdateRevision(u64);

/// The current state and the revision it was announced with.
struct Published {
    state: UpdateState,
    revision: UpdateRevision,
}

/// Revisions come from one counter for the whole process, so a coordinator created to
/// replace another keeps announcing revisions above the ones already shown.
static LAST_REVISION: AtomicU64 = AtomicU64::new(0);

impl Published {
    /// Replaces the state and returns the revision to announce it with. It runs under the
    /// state lock, so a coordinator's revisions follow the order of its states.
    fn set(&mut self, state: UpdateState) -> UpdateRevision {
        self.state = state;
        self.revision = UpdateRevision(LAST_REVISION.fetch_add(1, Ordering::SeqCst) + 1);
        self.revision
    }
}

type StateListener = Box<dyn Fn(&UpdateState, UpdateRevision) + Send + Sync + 'static>;

pub struct UpdateCoordinator<S, F, I> {
    source: S,
    files: F,
    installer: I,
    state: Mutex<Published>,
    download_busy: AtomicBool,
    generation: AtomicU64,
    on_state: StateListener,
}

/// Ends a download, finished or interrupted: removes a file that is not kept, frees the
/// updater and only then announces the outcome, so whoever reacts to it can already
/// retry or restart.
struct DownloadGuard<'a, S: UpdateSource + Sync, F: UpdateFiles + Sync, I: UpdateInstaller + Sync> {
    coord: &'a UpdateCoordinator<S, F, I>,
    destination: Option<String>,
    /// `ReadyToRestart` or `DownloadFailed`; `None` while running, so an interrupted
    /// download offers the release again.
    outcome: Option<UpdateState>,
    fallback_info: UpdateInfo,
}

impl<'a, S: UpdateSource + Sync, F: UpdateFiles + Sync, I: UpdateInstaller + Sync> Drop
    for DownloadGuard<'a, S, F, I>
{
    fn drop(&mut self) {
        let keep_download = matches!(self.outcome, Some(UpdateState::ReadyToRestart { .. }));
        if !keep_download && let Some(ref dest) = self.destination {
            let _ = self.coord.files.delete(dest);
        }
        let pending_notify = {
            let mut published = self.coord.state.lock().unwrap();
            // A finished download announces its outcome whatever replaced `Downloading`
            // meanwhile; an interrupted one only undoes its own `Downloading`.
            let next = match self.outcome.take() {
                Some(outcome) => Some(outcome),
                None if matches!(published.state, UpdateState::Downloading { .. }) => {
                    Some(UpdateState::Available(self.fallback_info.clone()))
                }
                None => None,
            };
            next.filter(|next| published.state != *next)
                .map(|next| (published.set(next.clone()), next))
        };
        self.coord.download_busy.store(false, Ordering::SeqCst);
        if let Some((revision, new_state)) = pending_notify {
            (self.coord.on_state)(&new_state, revision);
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
        on_state: impl Fn(&UpdateState, UpdateRevision) + Send + Sync + 'static,
    ) -> Self {
        Self {
            source,
            files,
            installer,
            state: Mutex::new(Published {
                state: UpdateState::Idle,
                revision: UpdateRevision::default(),
            }),
            download_busy: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            on_state: Box::new(on_state),
        }
    }

    pub fn state(&self) -> UpdateState {
        self.state.lock().unwrap().state.clone()
    }

    fn publish(&self, new_state: UpdateState) {
        let mut published = self.state.lock().unwrap();
        if published.state != new_state {
            let revision = published.set(new_state.clone());
            drop(published);
            (self.on_state)(&new_state, revision);
        }
    }

    pub fn can_check(&self) -> bool {
        !self.download_busy.load(Ordering::SeqCst)
            && !matches!(
                self.state.lock().unwrap().state,
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

        let (previous, checking_revision) = {
            let mut published = self.state.lock().unwrap();
            // Read again under the lock: a download or a newer check that started after
            // the first read publishes under this lock, and this check, which would never
            // publish its result, must not replace their state with `Checking`.
            if self.download_busy.load(Ordering::SeqCst)
                || self.generation.load(Ordering::SeqCst) != request
                || matches!(published.state, UpdateState::ReadyToRestart { .. })
            {
                return;
            }
            let prev_raw = published.state.clone();
            let checking_revision =
                (prev_raw != UpdateState::Checking).then(|| published.set(UpdateState::Checking));
            let prev = match prev_raw {
                UpdateState::Checking => UpdateState::Idle,
                other => other,
            };
            (prev, checking_revision)
        };

        let mut guard = CancelGuard {
            coord: self,
            request,
            previous: Some(previous),
        };

        if let Some(revision) = checking_revision {
            (self.on_state)(&UpdateState::Checking, revision);
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
            self.state.lock().unwrap().state,
            UpdateState::ReadyToRestart { .. }
        ) {
            self.download_busy.store(false, Ordering::SeqCst);
            return;
        }

        let mut guard = DownloadGuard {
            coord: self,
            destination: None,
            outcome: None,
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
                    // 100 % is `ReadyToRestart`: the size check below can still fail.
                    let p = p.min(DOWNLOADING_PROGRESS_LIMIT);
                    let reaches_limit = p >= DOWNLOADING_PROGRESS_LIMIT
                        && last_progress < DOWNLOADING_PROGRESS_LIMIT;
                    if (p - last_progress).abs() >= 0.01 || reaches_limit {
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

        guard.outcome = Some(match res {
            Ok((dest, is_steam)) => UpdateState::ReadyToRestart {
                downloaded_path: dest,
                is_steam_game_mode: is_steam,
            },
            Err(failure) => UpdateState::DownloadFailed(failure),
        });
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
