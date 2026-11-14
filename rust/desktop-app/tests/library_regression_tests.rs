mod common;

use common::{test_emulator_entry, test_native_entry};
use emusync_data_local::ConfigManager;
use emusync_desktop_app::{AppEvent, DesktopCoordinator};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, Language},
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
            language: Default::default(),
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

    emu.title_by_rom
        .push(("RomTwo.sfc".to_string(), "Rom Two Preserved".to_string()));
    emu.save_targets_by_rom.push((
        "RomTwo.sfc".to_string(),
        vec![SaveTarget {
            path: "/saves/rom2.srm".to_string(),
            kind: SaveTargetType::File,
            cloud_path: Some("rom2.srm".to_string()),
            excluded_folders: Vec::new(),
        }],
    ));
    let entry = GameEntry::Emulator(emu);

    manager
        .update(|_| AppConfig {
            language: Default::default(),
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

    let stale_entry = test_emulator("SNES");
    let stale_item = GameItem::new(
        "RomOne",
        stale_entry,
        Some(PathBuf::from("/tmp/roms/RomOne.sfc")),
    );

    coordinator.handle_action(UiAction::EditGameOverride {
        item: stale_item,
        title: Some("Rom One Custom".into()),
        targets: vec![SaveTarget {
            path: "/saves/rom1.srm".into(),
            kind: SaveTargetType::File,
            cloud_path: Some("rom1.srm".into()),
            excluded_folders: Vec::new(),
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
            language: Default::default(),
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

#[tokio::test]
async fn coordinator_add_and_delete_entry() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();

    let (tx, rx) = channel();
    let redraw = Arc::new(|| {});
    let coordinator =
        DesktopCoordinator::new(manager, tx, redraw, tokio::runtime::Handle::current());

    let new_entry = test_native_entry("Celeste");
    coordinator.handle_action(UiAction::AddEntry(new_entry.clone()));

    let mut added = false;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::ConfigReloaded(cfg) = &event
            && cfg.entries.iter().any(|e| e.common().name == "Celeste")
        {
            added = true;
        }
    }
    assert!(added);

    coordinator.handle_action(UiAction::DeleteEntry(new_entry));
    let mut deleted = false;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::ConfigReloaded(cfg) = &event
            && !cfg.entries.iter().any(|e| e.common().name == "Celeste")
        {
            deleted = true;
        }
    }
    assert!(deleted);
}

#[tokio::test]
async fn coordinator_reorder_entries() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();

    let e1 = test_emulator_entry("GBA");
    let e2 = test_emulator_entry("N64");
    manager
        .update(|_| AppConfig {
            language: Default::default(),
            credentials: None,
            entries: vec![e1.clone(), e2.clone()],
        })
        .unwrap();

    let (tx, rx) = channel();
    let redraw = Arc::new(|| {});
    let coordinator =
        DesktopCoordinator::new(manager, tx, redraw, tokio::runtime::Handle::current());

    coordinator.handle_action(UiAction::ReorderEntries(vec![e2.clone(), e1.clone()]));

    let mut reordered = false;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::ConfigReloaded(cfg) = &event
            && cfg.entries.len() == 2
            && cfg.entries[0].common().name == "N64"
            && cfg.entries[1].common().name == "GBA"
        {
            reordered = true;
        }
    }
    assert!(reordered);
}

/// Names in the order of the `ConfigReloaded` event at the head of the queue, if any.
fn next_reloaded_names(rx: &std::sync::mpsc::Receiver<AppEvent>) -> Option<Vec<String>> {
    match rx.try_recv() {
        Ok(AppEvent::ConfigReloaded(cfg)) => Some(
            cfg.entries
                .iter()
                .map(|entry| entry.common().name.clone())
                .collect(),
        ),
        _ => None,
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| name.to_string()).collect()
}

/// The UI computes each drag step from the entries it holds, so the reorder must be
/// reloaded before `handle_action` returns (the next frame drains the queue first).
#[tokio::test]
async fn reorder_is_reloaded_before_handle_action_returns() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();
    let (a, b, c) = (
        test_native_entry("A"),
        test_native_entry("B"),
        test_native_entry("C"),
    );
    manager
        .update(|_| AppConfig {
            language: Default::default(),
            credentials: None,
            entries: vec![a.clone(), b.clone(), c.clone()],
        })
        .unwrap();
    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );
    let on_disk = || -> Vec<String> {
        let text = std::fs::read_to_string(&config_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap().to_string())
            .collect()
    };

    coordinator.handle_action(UiAction::ReorderEntries(vec![
        b.clone(),
        a.clone(),
        c.clone(),
    ]));
    assert_eq!(next_reloaded_names(&rx), Some(names(&["B", "A", "C"])));
    assert_eq!(on_disk(), names(&["B", "A", "C"]));

    coordinator.handle_action(UiAction::ReorderEntries(vec![b, c, a]));
    assert_eq!(next_reloaded_names(&rx), Some(names(&["B", "C", "A"])));
    assert_eq!(on_disk(), names(&["B", "C", "A"]));
    assert!(
        rx.try_recv().is_err(),
        "no toast such as `Cannot reorder` is queued"
    );
}

#[tokio::test]
async fn coordinator_edit_game_override() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();

    let native = test_native_entry("MyGame");
    manager
        .update(|_| AppConfig {
            language: Default::default(),
            credentials: None,
            entries: vec![native.clone()],
        })
        .unwrap();

    let (tx, rx) = channel();
    let redraw = Arc::new(|| {});
    let coordinator =
        DesktopCoordinator::new(manager, tx, redraw, tokio::runtime::Handle::current());

    let item = GameItem::new("MyGame", native, None);
    coordinator.handle_action(UiAction::EditGameOverride {
        item,
        title: Some("My Game Remastered".into()),
        targets: vec![SaveTarget {
            path: "/new_save.dat".into(),
            cloud_path: Some("new_save.dat".into()),
            kind: SaveTargetType::File,
            excluded_folders: Vec::new(),
        }],
        cover_path: Some("/custom_cover.png".into()),
    });

    let mut override_saved = false;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::ConfigReloaded(cfg) = &event
            && let Some(GameEntry::Native(nat)) = cfg
                .entries
                .iter()
                .find(|e| e.common().name == "My Game Remastered")
            && nat.cover_path.as_deref() == Some("/custom_cover.png")
            && nat.common.save_targets.len() == 1
        {
            override_saved = true;
        }
    }
    assert!(override_saved);
}

#[tokio::test]
async fn reorder_entries_deduplicates_payload_and_appends_concurrently_added_categories() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();

    let ea = test_native("A");
    let eb = test_native("B");
    let ec = test_native("C");
    manager
        .update(|_| AppConfig {
            language: Default::default(),
            credentials: None,
            entries: vec![ea.clone(), eb.clone(), ec.clone()],
        })
        .unwrap();

    let (tx, rx) = channel();
    let redraw = Arc::new(|| {});
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        redraw,
        tokio::runtime::Handle::current(),
    );

    let ghost = test_native("Ghost");
    // Payload with duplicate "B", non-existent "Ghost", and missing "C"
    coordinator.handle_action(UiAction::ReorderEntries(vec![
        eb.clone(),
        eb.clone(),
        ghost,
        ea.clone(),
    ]));

    let mut reordered = false;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::ConfigReloaded(cfg) = &event {
            let names: Vec<_> = cfg
                .entries
                .iter()
                .map(|e| e.common().name.as_str())
                .collect();
            if names == ["B", "A", "C"] {
                reordered = true;
            }
        }
    }
    assert!(
        reordered,
        "Expected reorder to deduplicate and preserve missing entries at the end"
    );

    let loaded = manager.load().unwrap();
    let loaded_names: Vec<_> = loaded
        .entries
        .iter()
        .map(|e| e.common().name.as_str())
        .collect();
    assert_eq!(loaded_names, vec!["B", "A", "C"]);
}

#[tokio::test]
async fn choosing_a_language_stores_only_the_language() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    let manager = ConfigManager::new(&path).unwrap();
    manager
        .save(&AppConfig {
            language: Default::default(),
            credentials: None,
            entries: vec![test_native("Game")],
        })
        .unwrap();
    let before = manager.load().unwrap();
    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    coordinator.handle_action(UiAction::SetLanguage(Language::Spanish));

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while manager.load().unwrap().language != Language::Spanish {
        assert!(
            std::time::Instant::now() < deadline,
            "language never stored"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let stored = manager.load().unwrap();
    assert_eq!(
        stored,
        AppConfig {
            language: Language::Spanish,
            ..before
        },
        "only the language changed"
    );
    assert!(
        rx.try_iter()
            .all(|event| !matches!(event, AppEvent::ConfigReloaded(_))),
        "choosing a language reloads nothing"
    );
}

#[tokio::test]
async fn messages_follow_the_configured_and_the_chosen_language() {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    manager
        .save(&AppConfig {
            language: Language::Spanish,
            credentials: None,
            entries: Vec::new(),
        })
        .unwrap();
    let (tx, _rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );
    assert_eq!(coordinator.language.get(), Language::English);

    coordinator.initialize();
    assert_eq!(coordinator.language.get(), Language::Spanish);

    coordinator.handle_action(UiAction::SetLanguage(Language::English));
    assert_eq!(
        coordinator.language.get(),
        Language::English,
        "the next message is already in the chosen language"
    );
}
