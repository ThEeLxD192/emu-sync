//! Tests for UI state queries.

mod common;

use common::*;
use emusync_domain::{
    config::{AppConfig, Credentials},
    library::{EntryCommon, GameEntry, NativeGame},
};
use emusync_presentation::{AppUiState, CloudSyncStatus};

#[test]
fn app_ui_state_drive_connection_and_sync_queries() {
    let mut state = AppUiState::default();
    assert!(!state.is_drive_connected());

    state.config = Some(AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "client-id".to_string(),
            client_secret: "secret".to_string(),
            refresh_token: Some("1//valid-token".to_string()),
        }),
        entries: Vec::new(),
    });
    assert!(state.is_drive_connected());

    state
        .config
        .as_mut()
        .unwrap()
        .credentials
        .as_mut()
        .unwrap()
        .refresh_token = Some("".to_string());
    assert!(!state.is_drive_connected());
}

#[test]
fn app_ui_state_steam_registration_queries() {
    let mut state = AppUiState {
        steam_available: true,
        ..Default::default()
    };
    state.steam_registered_entries.insert("12345".to_string());
    state
        .steam_registered_entries
        .insert("Named Game".to_string());

    let entry_with_id = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "AppId Game".to_string(),
            steam_app_id: Some(12345),
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });
    assert!(state.is_steam_registered(&entry_with_id));

    let entry_with_name = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Named Game".to_string(),
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });
    assert!(state.is_steam_registered(&entry_with_name));

    let unregistered = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Other Game".to_string(),
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });
    assert!(!state.is_steam_registered(&unregistered));
}

#[test]
fn app_ui_state_sync_status_lookup() {
    let mut state = AppUiState::default();
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Portal 2".to_string(),
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });

    state.selected_entry = Some(entry);
    assert_eq!(state.sync_status_for_selected(), CloudSyncStatus::Idle);

    state
        .entry_sync_status
        .insert("Portal 2".to_string(), CloudSyncStatus::InSync);
    assert_eq!(state.sync_status_for_selected(), CloudSyncStatus::InSync);
}

#[test]
fn whitespace_or_missing_refresh_token_is_not_connected() {
    let with_token = |token: Option<&str>| AppUiState {
        config: Some(config_with(
            Vec::new(),
            Some(Credentials {
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
                refresh_token: token.map(str::to_string),
            }),
        )),
        ..AppUiState::default()
    };

    assert!(!with_token(Some("  \t")).is_drive_connected());
    assert!(!with_token(None).is_drive_connected());
    let no_credentials = AppUiState {
        config: Some(config_with(Vec::new(), None)),
        ..AppUiState::default()
    };
    assert!(!no_credentials.is_drive_connected());
}

#[test]
fn no_selection_reports_idle_even_with_known_statuses() {
    let mut state = AppUiState::default();
    state
        .entry_sync_status
        .insert("A".to_string(), CloudSyncStatus::Conflict);
    assert_eq!(state.selected_entry, None);

    assert_eq!(state.sync_status_for_selected(), CloudSyncStatus::Idle);
}

#[test]
fn app_ui_state_debug_never_shows_credentials() {
    let state = AppUiState {
        config: Some(config_with(
            Vec::new(),
            Some(Credentials {
                client_id: "id".to_string(),
                client_secret: "GOCSPX-state-secret".to_string(),
                refresh_token: Some("1//state-refresh-token".to_string()),
            }),
        )),
        ..AppUiState::default()
    };

    let rendered = format!("{state:?}");

    assert!(!rendered.contains("GOCSPX-state-secret"), "{rendered}");
    assert!(!rendered.contains("1//state-refresh-token"), "{rendered}");
    assert!(rendered.contains("client_id: \"id\""), "{rendered}");
}
