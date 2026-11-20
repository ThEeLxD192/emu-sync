//! Application updates: release checks, AppImage download and replacement on restart.

use super::proxy::ProxyPolicy;
use crate::events::{AppEvent, EventSender};
use emusync_application::{UpdateCoordinator, UpdateState};
use emusync_data_updates::{
    GitHubUpdateSource, ReleaseKey, ReleaseTransport, release_client_builder,
};
use emusync_domain::update::UpdateInfo;
use emusync_platform_desktop::{DesktopUpdateInstaller, UpdateStagingFiles};
use emusync_presentation::i18n::fill;
use std::sync::{Arc, Mutex};
use tokio::{runtime::Handle, task::JoinHandle};

const REPOSITORY_OWNER: &str = "ThEeLxD192";
const REPOSITORY_NAME: &str = "emu-sync";
/// Development builds only contact a controlled release server given here.
const DEVELOPMENT_SOURCE_VARIABLE: &str = "EMUSYNC_UPDATE_BASE_URL";
/// Development builds may trust another key for that server's signed manifests.
const DEVELOPMENT_KEY_VARIABLE: &str = "EMUSYNC_UPDATE_PUBLIC_KEY";

/// The key a controlled release server's manifests must be signed with: in development
/// builds the one in `EMUSYNC_UPDATE_PUBLIC_KEY` when it is a valid key, otherwise the
/// EmuSync key. GitHub releases are always checked against the EmuSync key.
pub(crate) fn controlled_release_key_for_this_build() -> ReleaseKey {
    let Some(key) = cfg!(debug_assertions)
        .then(|| std::env::var(DEVELOPMENT_KEY_VARIABLE).ok())
        .flatten()
    else {
        return ReleaseKey::emusync();
    };
    ReleaseKey::from_base64(&key).unwrap_or_else(|error| {
        eprintln!("{DEVELOPMENT_KEY_VARIABLE} is ignored ({error}); using the EmuSync key");
        ReleaseKey::emusync()
    })
}

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

/// The error followed by each of its causes, which `reqwest` leaves out of its message.
fn with_causes(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(current) = cause {
        message.push_str(": ");
        message.push_str(&current.to_string());
        cause = current.source();
    }
    message
}

type DesktopUpdater =
    UpdateCoordinator<GitHubUpdateSource, UpdateStagingFiles, DesktopUpdateInstaller>;

/// Why a build has no updater.
#[derive(Clone, Debug)]
enum UpdatesUnavailable {
    /// Development build without a controlled release server.
    Development,
    /// A portable copy, replaced by hand.
    Portable,
    /// The HTTP client could not be built; holds the reason.
    ClientFailed(String),
}

/// The update coordinator plus the background checks and downloads it runs.
#[derive(Clone)]
pub struct UpdateServices {
    updater: Result<Arc<DesktopUpdater>, UpdatesUnavailable>,
    tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
    tokio_handle: Handle,
}

impl UpdateServices {
    /// `controlled_release_key` checks a controlled server's releases; GitHub's are
    /// always checked against the EmuSync key.
    pub fn new(
        source: UpdateSourceConfig,
        proxy_policy: ProxyPolicy,
        controlled_release_key: ReleaseKey,
        event_tx: EventSender,
        tokio_handle: Handle,
    ) -> Self {
        let (base_url, transport, release_key) = match source {
            UpdateSourceConfig::GitHub => {
                (None, ReleaseTransport::HttpsOnly, ReleaseKey::emusync())
            }
            UpdateSourceConfig::Controlled(url) => (
                Some(url),
                ReleaseTransport::AllowPlainHttp,
                controlled_release_key,
            ),
            UpdateSourceConfig::Disabled => {
                return Self::without_updater(UpdatesUnavailable::Development, tokio_handle);
            }
            UpdateSourceConfig::Portable => {
                return Self::without_updater(UpdatesUnavailable::Portable, tokio_handle);
            }
        };
        let client = match proxy_policy
            .apply(release_client_builder(transport))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                let reason = UpdatesUnavailable::ClientFailed(with_causes(&error));
                return Self::without_updater(reason, tokio_handle);
            }
        };
        let mut source = GitHubUpdateSource::new(
            client,
            REPOSITORY_OWNER,
            REPOSITORY_NAME,
            env!("CARGO_PKG_VERSION"),
            cfg!(windows),
        )
        .with_release_key(release_key);
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
            updater: Ok(Arc::new(updater)),
            tasks: Arc::default(),
            tokio_handle,
        }
    }

    fn without_updater(reason: UpdatesUnavailable, tokio_handle: Handle) -> Self {
        Self {
            updater: Err(reason),
            tasks: Arc::default(),
            tokio_handle,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.updater.is_ok()
    }

    /// Why updates are unavailable, for the user who asked for one.
    pub fn disabled_message(&self, t: &emusync_presentation::i18n::Texts) -> String {
        match &self.updater {
            Err(UpdatesUnavailable::Portable) => t.msg_updates_portable.to_owned(),
            Err(UpdatesUnavailable::ClientFailed(error)) => {
                fill(t.msg_http_config_failed, &[("error", error)])
            }
            Ok(_) | Err(UpdatesUnavailable::Development) => t.msg_updates_disabled.to_owned(),
        }
    }

    /// Checks for a newer release; `on_done` gets the resulting state.
    pub fn check(&self, on_done: impl FnOnce(UpdateState) + Send + 'static) {
        let Ok(updater) = self.updater.clone() else {
            return;
        };
        self.spawn(async move {
            updater.check_for_updates().await;
            on_done(updater.state());
        });
    }

    pub fn download(&self, info: UpdateInfo) {
        let Ok(updater) = self.updater.clone() else {
            return;
        };
        self.spawn(async move { updater.download(&info).await });
    }

    /// Replaces the running AppImage with the download and closes the application.
    /// Returns whether the update was applied.
    pub async fn restart(&self, path: String) -> bool {
        let Ok(updater) = self.updater.clone() else {
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
