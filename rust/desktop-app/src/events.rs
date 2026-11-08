//! Application events sent from background operations to the presentation layer.

use emusync_application::UpdateState;
use emusync_domain::config::AppConfig;
use emusync_presentation::model::{
    AppStatus, CloudSyncStatus, GameItem, PathTarget, SaveSetupRequest,
};

/// Typed events dispatched from background tasks to update the UI single source of truth.
#[derive(Clone, Debug, PartialEq)]
pub enum AppEvent {
    ConfigReloaded(AppConfig),
    EntryGamesScanned {
        entry_name: String,
        games: Vec<GameItem>,
        revision: u64,
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
    SaveSetupRequested {
        request: Box<SaveSetupRequest>,
    },
}

/// Applies an `EntryGamesScanned` event to presentation state if the revision is newer or equal
/// and the event matches the currently selected category.
pub fn apply_entry_games_scanned(
    current_scan_revision: &mut u64,
    game_items: &mut Vec<GameItem>,
    selected_entry_name: Option<&str>,
    event_entry_name: &str,
    games: Vec<GameItem>,
    revision: u64,
) -> bool {
    if selected_entry_name == Some(event_entry_name) && revision >= *current_scan_revision {
        *current_scan_revision = revision;
        *game_items = games;
        true
    } else {
        false
    }
}
