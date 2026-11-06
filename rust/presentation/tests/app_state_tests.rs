//! Tests for UI state, models, and GameItem resolution.

use emusync_domain::{
    config::{AppConfig, Credentials},
    library::{Emulator, EntryCommon, GameEntry, NativeGame},
};
use emusync_presentation::{AppUiState, CloudSyncStatus, GameItem};
use std::path::PathBuf;

#[test]
fn game_item_resolves_effective_title_and_cover_path() {
    let emulator = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "GBA".to_string(),
            ..EntryCommon::default()
        },
        title_by_rom: vec![("zelda.gba".to_string(), "The Minish Cap".to_string())],
        cover_path_by_rom: vec![("zelda.gba".to_string(), "/covers/zelda.png".to_string())],
        ..Emulator::default()
    });

    let overridden_item = GameItem::new(
        "zelda.gba",
        emulator.clone(),
        Some(PathBuf::from("/roms/gba/zelda.gba")),
    );
    assert_eq!(overridden_item.effective_title(), "The Minish Cap");
    assert_eq!(
        overridden_item.effective_cover_path(),
        Some("/covers/zelda.png".to_string())
    );

    let default_item = GameItem::new(
        "pokemon.gba",
        emulator,
        Some(PathBuf::from("/roms/gba/pokemon.gba")),
    );
    assert_eq!(default_item.effective_title(), "pokemon");
    assert_eq!(default_item.effective_cover_path(), None);

    let native = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Hades".to_string(),
            ..EntryCommon::default()
        },
        cover_path: Some("/covers/hades.jpg".to_string()),
        ..NativeGame::default()
    });

    let native_item = GameItem::new("Hades", native, None);
    assert_eq!(native_item.effective_title(), "Hades");
    assert_eq!(
        native_item.effective_cover_path(),
        Some("/covers/hades.jpg".to_string())
    );
}

#[test]
fn app_ui_state_drive_connection_and_sync_queries() {
    let mut state = AppUiState::default();
    assert!(!state.is_drive_connected());

    state.config = Some(AppConfig {
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
