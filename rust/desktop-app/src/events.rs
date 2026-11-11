//! Application events sent from background operations to the presentation layer.

use emusync_application::UpdateState;
use emusync_domain::{config::AppConfig, library::SaveTargetType};
use emusync_presentation::model::{
    AppStatus, CloudSyncStatus, GameItem, PathTarget, PickMode, SaveSetupRequest,
};
use std::collections::HashSet;

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
    /// Whether Steam shortcuts can be managed and which entries are registered,
    /// as Kotlin's `refreshSteamState`.
    SteamStateRefreshed {
        available: bool,
        registered: HashSet<String>,
        revision: u64,
    },
    StatusChanged(AppStatus),
    Toast(String),
    UpdateStateChanged(UpdateState),
    PathPicked {
        target: PathTarget,
        mode: PickMode,
        path: String,
    },
    SaveTargetTypeDetected {
        index: usize,
        path: String,
        kind: SaveTargetType,
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

/// Applies a `SteamStateRefreshed` event unless a newer refresh was already applied.
pub fn apply_steam_state_refreshed(
    current_revision: &mut u64,
    steam_available: &mut bool,
    registered_entries: &mut HashSet<String>,
    available: bool,
    registered: HashSet<String>,
    revision: u64,
) -> bool {
    if revision < *current_revision {
        return false;
    }
    *current_revision = revision;
    *steam_available = available;
    *registered_entries = registered;
    true
}
