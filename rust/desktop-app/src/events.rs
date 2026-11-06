//! Application events sent from background operations to the presentation layer.

use emusync_application::UpdateState;
use emusync_domain::config::AppConfig;
use emusync_presentation::model::{AppStatus, CloudSyncStatus, GameItem, PathTarget};

/// Typed events dispatched from background tasks to update the UI single source of truth.
#[derive(Clone, Debug, PartialEq)]
pub enum AppEvent {
    ConfigReloaded(AppConfig),
    EntryGamesScanned {
        entry_name: String,
        games: Vec<GameItem>,
    },
    SyncStatusUpdated {
        entry_name: String,
        status: CloudSyncStatus,
    },
    SteamRegistrationUpdated {
        entry_name: String,
        registered: bool,
    },
    StatusChanged(AppStatus),
    Toast(String),
    UpdateStateChanged(UpdateState),
    PathPicked {
        target: PathTarget,
        path: String,
    },
    CloseNotice(String),
}
