//! Application events sent from background operations to the presentation layer.

use emusync_application::UpdateState;
use emusync_domain::{
    config::AppConfig,
    library::{SaveTarget, SaveTargetType},
};
use emusync_presentation::model::{
    AppStatus, CloudSyncStatus, GameItem, PathTarget, PickMode, SaveSetupRequest,
};
use std::{
    collections::HashSet,
    sync::{Arc, mpsc::Sender},
};

/// Sends events to the UI and wakes it. The UI draws only when something asks it to, so
/// an event sent without waking it would wait for unrelated input.
#[derive(Clone)]
pub struct EventSender {
    tx: Sender<AppEvent>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl EventSender {
    pub fn new(tx: Sender<AppEvent>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self { tx, wake }
    }

    /// Queues `event`, then wakes the UI so the next frame receives it.
    pub fn send(&self, event: AppEvent) -> Result<(), UiClosed> {
        self.tx.send(event).map_err(|_| UiClosed)?;
        (self.wake)();
        Ok(())
    }
}

/// The UI no longer receives events: the application is closing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiClosed;

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
    /// An installed update asks the application to close normally.
    ExitRequested,
    PathPicked {
        /// The id of the `PickPath` request this answers.
        request_id: u64,
        target: PathTarget,
        mode: PickMode,
        path: String,
    },
    /// The game's own save targets were read; the game dialog can open.
    GameEditReady {
        item: Box<GameItem>,
        targets: Vec<SaveTarget>,
    },
    /// Where each cloud file would be restored inside `destination`.
    RestorePathsPreviewed {
        /// The id of the `PreviewRestorePaths` request this answers.
        request_id: u64,
        destination: String,
        paths: Vec<String>,
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
    /// The coordinator resolved or dropped request `request_id`; its dialog must close
    /// without answering.
    SaveSetupClosed {
        request_id: u64,
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
