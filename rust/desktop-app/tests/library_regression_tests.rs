use emusync_data_local::ConfigManager;
use emusync_desktop_app::{AppEvent, DesktopCoordinator};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore},
    library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType},
};
use emusync_presentation::{actions::UiAction, model::GameItem};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc::channel},
};
use tempfile::tempdir;

fn test_native(name: &str) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: "/bin/true".to_string(),
            arguments: vec![],
            save_paths: vec!["/tmp/saves".to_string()],
            save_targets: vec![],
            group: None,
            cloud_folder: None,
            drive_file_id: None,
            steam_app_id: None,
        },
        cover_path: None,
        wait_for_process: None,
    })
}

fn test_emulator(name: &str) -> GameEntry {
    GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: "/bin/emulator".to_string(),
            arguments: vec!["{ROM}".to_string()],
            save_paths: vec!["/tmp/saves".to_string()],
            save_targets: vec![],
            group: None,
            cloud_folder: None,
            drive_file_id: None,
            steam_app_id: None,
        },
        roms_directory: "/tmp/roms".to_string(),
        extensions: vec!["sfc".to_string()],
        save_paths_by_rom: vec![],
        save_targets_by_rom: vec![],
        cover_path_by_rom: vec![],
        title_by_rom: vec![],
        steam_process_name: None,
        fullscreen_args: None,
        covers_directory: None,
    })
}

#[tokio::test]
async fn renaming_b_to_existing_a_is_rejected_with_toast() {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry_a = test_native("CategoryA");
    let entry_b = test_native("CategoryB");
    manager
        .update(|_| AppConfig {
            credentials: None,
            entries: vec![entry_a.clone(), entry_b.clone()],
        })
        .unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    // Try renaming B as A
    coordinator.handle_action(UiAction::EditEntry {
        old: Box::new(entry_b.clone()),
        new: Box::new(test_native("CategoryA")),
    });

    let events: Vec<_> = rx.try_iter().collect();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEvent::Toast(msg) if msg.contains("already in use"))),
        "Expected toast rejecting duplicate name"
    );

    // Config entries must retain original names
    let loaded = manager.load().unwrap();
    assert_eq!(loaded.entries.len(), 2);
    assert!(
        loaded
            .entries
            .iter()
            .any(|e| e.common().name == "CategoryA")
    );
    assert!(
        loaded
            .entries
            .iter()
            .any(|e| e.common().name == "CategoryB")
    );
}

#[tokio::test]
async fn old_game_item_does_not_wipe_other_rom_overrides() {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();

    let mut emu = match test_emulator("SNES") {
        GameEntry::Emulator(e) => e,
        _ => unreachable!(),
    };

    // Pre-install an override for RomTwo
    emu.title_by_rom
        .push(("RomTwo.sfc".to_string(), "Rom Two Preserved".to_string()));
    emu.save_targets_by_rom.push((
        "RomTwo.sfc".to_string(),
        vec![SaveTarget {
            path: "/saves/rom2.srm".to_string(),
            kind: SaveTargetType::File,
            cloud_path: Some("rom2.srm".to_string()),
        }],
    ));
    let entry = GameEntry::Emulator(emu);

    manager
        .update(|_| AppConfig {
            credentials: None,
            entries: vec![entry.clone()],
        })
        .unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    // Stale GameItem from before RomTwo had an override (empty game_overrides)
    let stale_entry = test_emulator("SNES");
    let stale_item = GameItem::new(
        "RomOne",
        stale_entry,
        Some(PathBuf::from("/tmp/roms/RomOne.sfc")),
    );

    // Edit RomOne's override
    coordinator.handle_action(UiAction::EditGameOverride {
        item: stale_item,
        title: Some("Rom One Custom".into()),
        targets: vec![SaveTarget {
            path: "/saves/rom1.srm".into(),
            kind: SaveTargetType::File,
            cloud_path: Some("rom1.srm".into()),
        }],
        cover_path: None,
    });

    let mut saved = false;
    while let Ok(event) = rx.try_recv() {
        if matches!(event, AppEvent::ConfigReloaded(_)) {
            saved = true;
            break;
        }
    }
    assert!(saved, "Expected ConfigReloaded after saving override");

    // Check that RomTwo's override was NOT wiped
    let loaded = manager.load().unwrap();
    let loaded_emu = match &loaded.entries[0] {
        GameEntry::Emulator(e) => e,
        _ => panic!("Expected emulator entry"),
    };

    assert!(
        loaded_emu
            .title_by_rom
            .iter()
            .any(|(r, t)| r == "RomTwo.sfc" && t == "Rom Two Preserved"),
        "RomTwo override must be preserved"
    );
}

#[tokio::test]
async fn deleted_entry_produces_no_save_success() {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    manager.update(|_| AppConfig::default()).unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    let ghost_entry = test_native("GhostCategory");
    let ghost_item = GameItem::new("GhostGame", ghost_entry, None);

    coordinator.handle_action(UiAction::EditGameOverride {
        item: ghost_item,
        title: Some("Ghost Remastered".into()),
        targets: vec![],
        cover_path: None,
    });

    let events: Vec<_> = rx.try_iter().collect();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEvent::Toast(msg) if msg.contains("no longer exists"))),
        "Expected toast for non-existent entry"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AppEvent::ConfigReloaded(_))),
        "Must not emit ConfigReloaded for ghost entry"
    );
}

#[tokio::test]
async fn renaming_native_game_override_to_existing_entry_is_rejected_with_toast() {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry_a = test_native("GameA");
    let entry_b = test_native("GameB");
    manager
        .update(|_| AppConfig {
            credentials: None,
            entries: vec![entry_a.clone(), entry_b.clone()],
        })
        .unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    let item_b = GameItem::new("GameB", entry_b, None);

    coordinator.handle_action(UiAction::EditGameOverride {
        item: item_b,
        title: Some("GameA".into()),
        targets: vec![],
        cover_path: None,
    });

    let events: Vec<_> = rx.try_iter().collect();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEvent::Toast(msg) if msg.contains("already in use"))),
        "Expected toast for conflicting native game name"
    );

    let loaded = manager.load().unwrap();
    assert_eq!(loaded.entries.len(), 2);
    assert_eq!(loaded.entries[0].common().name, "GameA");
    assert_eq!(loaded.entries[1].common().name, "GameB");
}
