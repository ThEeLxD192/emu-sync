//! Application updates: release checks, AppImage download and replacement on restart.

use super::{UPDATES_DISABLED, UPDATES_PORTABLE};
use crate::events::{AppEvent, EventSender};
use emusync_application::{UpdateCoordinator, UpdateState};
use emusync_data_updates::GitHubUpdateSource;
use emusync_domain::update::UpdateInfo;
use emusync_platform_desktop::{DesktopUpdateInstaller, UpdateStagingFiles};
use std::sync::{Arc, Mutex};
use tokio::{runtime::Handle, task::JoinHandle};

const REPOSITORY_OWNER: &str = "ThEeLxD192";
const REPOSITORY_NAME: &str = "emu-sync";
/// Development builds only contact a controlled release server given here.
const DEVELOPMENT_SOURCE_VARIABLE: &str = "EMUSYNC_UPDATE_BASE_URL";

/// Where release information comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateSourceConfig {
    /// The project's GitHub releases.
    GitHub,
    /// A controlled server with GitHub's release API, for development and tests.
    Controlled(String),
    /// Development build without a controlled server: never contacts GitHub.
    Disabled,
    /// A portable copy: the installer-based updater would install the machine-wide package
    /// beside it, so the copy is replaced by hand instead.
    Portable,
}

impl UpdateSourceConfig {
    /// Release builds use GitHub; development builds use `EMUSYNC_UPDATE_BASE_URL` or nothing.
    pub fn for_this_build() -> Self {
        if emusync_data_local::portable_install_directory().is_some() {
            return Self::Portable;
        }
        Self::select(
            std::env::var(DEVELOPMENT_SOURCE_VARIABLE).ok(),
            cfg!(debug_assertions),
        )
    }

    /// The controlled server is honoured only in development builds, so an environment
    /// variable can never redirect where a release build downloads itself from.
    pub fn select(controlled: Option<String>, development: bool) -> Self {
        let controlled = controlled.filter(|url| !url.trim().is_empty());
        match (development, controlled) {
            (false, _) => Self::GitHub,
            (true, Some(url)) => Self::Controlled(url.trim().to_string()),
            (true, None) => Self::Disabled,
        }
    }
}

type DesktopUpdater =
    UpdateCoordinator<GitHubUpdateSource, UpdateStagingFiles, DesktopUpdateInstaller>;

/// The update coordinator plus the background checks and downloads it runs.
#[derive(Clone)]
pub struct UpdateServices {
    updater: Option<Arc<DesktopUpdater>>,
    portable: bool,
    tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
    tokio_handle: Handle,
}

impl UpdateServices {
    pub fn new(source: UpdateSourceConfig, event_tx: EventSender, tokio_handle: Handle) -> Self {
        let portable = source == UpdateSourceConfig::Portable;
        let base_url = match source {
            UpdateSourceConfig::GitHub => None,
            UpdateSourceConfig::Controlled(url) => Some(url),
            UpdateSourceConfig::Disabled | UpdateSourceConfig::Portable => {
                return Self {
                    updater: None,
                    portable,
                    tasks: Arc::default(),
                    tokio_handle,
                };
            }
        };
        let mut source = GitHubUpdateSource::new(
            reqwest::Client::new(),
            REPOSITORY_OWNER,
            REPOSITORY_NAME,
            env!("CARGO_PKG_VERSION"),
            cfg!(windows),
        );
        if let Some(url) = base_url {
            source = source.with_base_url(url);
        }
        // Closing goes through the normal shutdown, which waits for running operations.
        let exit_tx = event_tx.clone();
        let installer = DesktopUpdateInstaller::new().with_exit_application(move |_| {
            let _ = exit_tx.send(AppEvent::ExitRequested);
        });
        let updater = UpdateCoordinator::new(
            source,
            UpdateStagingFiles::default_for_platform(),
            installer,
            move |state: &UpdateState| {
                let _ = event_tx.send(AppEvent::UpdateStateChanged(state.clone()));
            },
        );
        Self {
            updater: Some(Arc::new(updater)),
            portable: false,
            tasks: Arc::default(),
            tokio_handle,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.updater.is_some()
    }

    /// Why updates are unavailable, for the user who asked for one.
    pub fn disabled_message(&self) -> &'static str {
        if self.portable {
            UPDATES_PORTABLE
        } else {
            UPDATES_DISABLED
        }
    }

    /// Checks for a newer release; `on_done` gets the resulting state.
    pub fn check(&self, on_done: impl FnOnce(UpdateState) + Send + 'static) {
        let Some(updater) = self.updater.clone() else {
            return;
        };
        self.spawn(async move {
            updater.check_for_updates().await;
            on_done(updater.state());
        });
    }

    pub fn download(&self, info: UpdateInfo) {
        let Some(updater) = self.updater.clone() else {
            return;
        };
        self.spawn(async move { updater.download(&info).await });
    }

    /// Replaces the running AppImage with the download and closes the application.
    /// Returns whether the update was applied.
    pub async fn restart(&self, path: String) -> bool {
        let Some(updater) = self.updater.clone() else {
            return false;
        };
        tokio::task::spawn_blocking(move || updater.restart(&path, false))
            .await
            .unwrap_or(false)
    }

    /// Stops checks and downloads; an interrupted download removes its partial file.
    pub fn abort(&self) {
        for task in self.tasks.lock().unwrap().drain(..) {
            task.abort();
        }
    }

    fn spawn(&self, task: impl std::future::Future<Output = ()> + Send + 'static) {
        let handle = self.tokio_handle.spawn(task);
        let mut tasks = self.tasks.lock().unwrap();
        tasks.retain(|t| !t.is_finished());
        tasks.push(handle);
    }
}
