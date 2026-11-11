//! Immutable single source of truth for the entire application UI state.

use crate::model::{AppStatus, CloudSyncStatus, GameItem};
use emusync_application::UpdateState;
use emusync_domain::{config::AppConfig, library::GameEntry};
use std::collections::{HashMap, HashSet};

/// UI Single Source of Truth state struct.
#[derive(Clone, Debug, PartialEq)]
pub struct AppUiState {
    pub config: Option<AppConfig>,
    pub selected_entry: Option<GameEntry>,
    pub steam_available: bool,
    pub steam_registered_entries: HashSet<String>,
    pub game_items: Vec<GameItem>,
    pub status: AppStatus,
    pub is_loading: bool,
    pub is_operation_in_progress: bool,
    pub entry_sync_status: HashMap<String, CloudSyncStatus>,
    pub update_state: UpdateState,
    pub show_update_dialog: bool,
    pub toast_message: Option<String>,
    pub current_version: String,
    pub config_path: String,
}

impl Default for AppUiState {
    fn default() -> Self {
        Self {
            config: None,
            selected_entry: None,
            steam_available: false,
            steam_registered_entries: HashSet::new(),
            game_items: Vec::new(),
            status: AppStatus::Idle,
            is_loading: false,
            is_operation_in_progress: false,
            entry_sync_status: HashMap::new(),
            update_state: UpdateState::Idle,
            show_update_dialog: false,
            toast_message: None,
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            config_path: String::new(),
        }
    }
}

impl AppUiState {
    pub fn is_drive_connected(&self) -> bool {
        self.config
            .as_ref()
            .and_then(|c| c.credentials.as_ref())
            .and_then(|creds| creds.refresh_token.as_deref())
            .is_some_and(|token| !token.trim().is_empty())
    }

    pub fn selected_entry_name(&self) -> Option<&str> {
        self.selected_entry
            .as_ref()
            .map(|e| e.common().name.as_str())
    }

    pub fn sync_status_for_selected(&self) -> CloudSyncStatus {
        self.selected_entry_name()
            .and_then(|name| self.entry_sync_status.get(name).copied())
            .unwrap_or(CloudSyncStatus::Idle)
    }

    pub fn is_steam_registered(&self, entry: &GameEntry) -> bool {
        entry
            .common()
            .steam_app_id
            .is_some_and(|id| self.steam_registered_entries.contains(&id.to_string()))
            || self.steam_registered_entries.contains(&entry.common().name)
    }
}
