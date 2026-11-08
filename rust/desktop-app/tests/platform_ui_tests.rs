use emusync_data_local::ConfigManager;
use emusync_desktop_app::{AppEvent, DesktopCoordinator, scan_entry_games};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore},
    library::{Emulator, EntryCommon, GameEntry, NativeGame},
};
use emusync_presentation::{actions::UiAction, model::GameItem};
use std::sync::{Arc, mpsc::channel};
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

#[tokio::test]
async fn deleting_last_category_clears_library_cleanly() {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry = test_native("OnlyCategory");
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

    coordinator.handle_action(UiAction::DeleteEntry(entry));

    let mut saw_config_reloaded = false;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::ConfigReloaded(cfg) = event {
            assert!(cfg.entries.is_empty(), "Config should have 0 entries");
            saw_config_reloaded = true;
        }
    }
    assert!(saw_config_reloaded);
}

#[test]
fn stale_scan_event_with_lower_revision_is_ignored() {
    use emusync_desktop_app::apply_entry_games_scanned;

    let mut current_revision = 0u64;
    let mut displayed_games = vec![];

    // Event 1 with revision 2 arrives first for "Cat"
    let event1_games = vec![GameItem::new("GameV2", test_native("Cat"), None)];
    let applied1 = apply_entry_games_scanned(
        &mut current_revision,
        &mut displayed_games,
        Some("Cat"),
        "Cat",
        event1_games,
        2,
    );
    assert!(applied1);
    assert_eq!(current_revision, 2);
    assert_eq!(displayed_games[0].name, "GameV2");

    // Stale Event with revision 1 arrives later for "Cat"
    let event0_games = vec![GameItem::new("GameV1_Stale", test_native("Cat"), None)];
    let applied0 = apply_entry_games_scanned(
        &mut current_revision,
        &mut displayed_games,
        Some("Cat"),
        "Cat",
        event0_games,
        1,
    );
    assert!(!applied0);
    assert_eq!(current_revision, 2);
    assert_eq!(displayed_games[0].name, "GameV2");

    // Event for a different category arrives
    let other_games = vec![GameItem::new("OtherGame", test_native("OtherCat"), None)];
    let applied_other = apply_entry_games_scanned(
        &mut current_revision,
        &mut displayed_games,
        Some("Cat"),
        "OtherCat",
        other_games,
        3,
    );
    assert!(!applied_other);
    assert_eq!(current_revision, 2);
    assert_eq!(displayed_games[0].name, "GameV2");
}

#[tokio::test]
async fn scan_entry_games_handles_missing_roms_dir_and_filters_extensions_with_monotonic_revisions()
{
    let dir = tempdir().unwrap();
    let roms_dir = dir.path().join("roms");
    std::fs::create_dir(&roms_dir).unwrap();
    std::fs::write(roms_dir.join("Mario.sfc"), b"mario").unwrap();
    std::fs::write(roms_dir.join("Zelda.smc"), b"zelda").unwrap();
    std::fs::write(roms_dir.join("Readme.txt"), b"readme").unwrap();

    let missing_emu = Emulator {
        common: EntryCommon {
            name: "MissingEmu".into(),
            ..Default::default()
        },
        roms_directory: "/nonexistent/roms/dir".into(),
        extensions: vec!["sfc".into(), "smc".into()],
        ..Default::default()
    };
    let entry_missing = GameEntry::Emulator(missing_emu);

    let valid_emu = Emulator {
        common: EntryCommon {
            name: "ValidEmu".into(),
            ..Default::default()
        },
        roms_directory: roms_dir.to_str().unwrap().to_string(),
        extensions: vec!["sfc".into(), "smc".into()],
        ..Default::default()
    };
    let entry_valid = GameEntry::Emulator(valid_emu);

    assert!(scan_entry_games(&entry_missing).is_empty());
    let scanned_games = scan_entry_games(&entry_valid);
    assert_eq!(scanned_games.len(), 2);
    assert!(scanned_games.iter().any(|g| g.name == "Mario"));
    assert!(scanned_games.iter().any(|g| g.name == "Zelda"));
    assert!(!scanned_games.iter().any(|g| g.name == "Readme"));

    let (tx, rx) = std::sync::mpsc::channel();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    let (rev1, h1) = coordinator.platform.scan_entry_games_async(entry_missing);
    let (rev2, h2) = coordinator.platform.scan_entry_games_async(entry_valid);

    let _ = h1.await;
    let _ = h2.await;

    assert_eq!(rev1, 1);
    assert_eq!(rev2, 2);

    let events: Vec<_> = rx.try_iter().collect();
    let ev1 = events
        .iter()
        .find(|e| matches!(e, AppEvent::EntryGamesScanned { revision: 1, .. }))
        .unwrap();
    let ev2 = events
        .iter()
        .find(|e| matches!(e, AppEvent::EntryGamesScanned { revision: 2, .. }))
        .unwrap();

    match ev1 {
        AppEvent::EntryGamesScanned { games, .. } => assert!(games.is_empty()),
        _ => unreachable!(),
    }
    match ev2 {
        AppEvent::EntryGamesScanned { games, .. } => {
            assert_eq!(games.len(), 2);
            assert!(games.iter().any(|g| g.name == "Mario"));
            assert!(games.iter().any(|g| g.name == "Zelda"));
        }
        _ => unreachable!(),
    }
}
