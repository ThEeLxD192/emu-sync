use emusync_data_local::ConfigManager;
use emusync_desktop_app::{AppEvent, DesktopCoordinator, scan_entry_games};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore},
    library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType},
};
use emusync_presentation::{
    EmuSyncApp,
    actions::UiAction,
    model::{AppStatus, GameItem},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::channel,
};
use tempfile::tempdir;

fn test_emulator_entry(name: &str) -> GameEntry {
    GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: "/usr/bin/retroarch".to_string(),
            arguments: vec!["{ROM}".to_string()],
            save_paths: vec!["/saves".to_string()],
            save_targets: vec![SaveTarget {
                path: "/saves".to_string(),
                cloud_path: Some("saves".to_string()),
                kind: SaveTargetType::Directory,
            }],
            ..Default::default()
        },
        roms_directory: "/roms".to_string(),
        extensions: vec!["sfc".to_string(), "smc".to_string()],
        ..Default::default()
    })
}

fn test_native_entry(name: &str) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: "/bin/sh".to_string(),
            arguments: vec!["-c".to_string(), "exit 0".to_string()],
            save_paths: vec!["/save.dat".to_string()],
            save_targets: vec![SaveTarget {
                path: "/save.dat".to_string(),
                cloud_path: Some("save.dat".to_string()),
                kind: SaveTargetType::File,
            }],
            ..Default::default()
        },
        ..Default::default()
    })
}

#[tokio::test]
async fn coordinator_initializes_and_scans_library() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();

    let entry = test_emulator_entry("SNES");
    manager
        .update(|_| AppConfig {
            credentials: None,
            entries: vec![entry.clone()],
        })
        .unwrap();

    let (tx, rx) = channel();
    let redraw_requested = Arc::new(AtomicBool::new(false));
    let redraw_clone = redraw_requested.clone();
    let redraw = Arc::new(move || {
        redraw_clone.store(true, Ordering::SeqCst);
    });

    let coordinator =
        DesktopCoordinator::new(manager, tx, redraw, tokio::runtime::Handle::current());
    coordinator.initialize();

    assert!(redraw_requested.load(Ordering::SeqCst));

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }

    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEvent::ConfigReloaded(_)))
    );
    assert!(events.iter().any(
        |e| matches!(e, AppEvent::EntryGamesScanned { entry_name, .. } if entry_name == "SNES")
    ));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEvent::SteamRegistrationUpdated { entry_name, .. } if entry_name == "SNES"))
    );
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

#[tokio::test]
async fn coordinator_edit_game_override() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();

    let native = test_native_entry("MyGame");
    manager
        .update(|_| AppConfig {
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
async fn coordinator_launch_game_session() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();

    let native = test_native_entry("QuickRunner");
    manager
        .update(|_| AppConfig {
            credentials: None,
            entries: vec![native.clone()],
        })
        .unwrap();

    let (tx, rx) = channel();
    let redraw = Arc::new(|| {});
    let coordinator =
        DesktopCoordinator::new(manager, tx, redraw, tokio::runtime::Handle::current());

    let item = GameItem::new("QuickRunner", native, None);
    coordinator.handle_action(UiAction::LaunchGame(item));

    // Await background execution
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let mut seen_playing = false;
    let mut seen_idle = false;
    while let Ok(event) = rx.try_recv() {
        match event {
            AppEvent::StatusChanged(AppStatus::Playing) => seen_playing = true,
            AppEvent::StatusChanged(AppStatus::Idle) => seen_idle = true,
            _ => {}
        }
    }
    assert!(seen_playing);
    assert!(seen_idle);
}

#[test]
fn app_processes_events_and_renders_cleanly() {
    let mut app = EmuSyncApp::new();
    let entry = test_native_entry("Hollow Knight");
    let games = scan_entry_games(&entry);

    app.state.config = Some(AppConfig {
        credentials: None,
        entries: vec![entry.clone()],
    });
    app.state.selected_entry = Some(entry);
    app.state.game_items = games;

    let context = egui::Context::default();
    let output = context.run_ui(egui::RawInput::default(), |ui| {
        app.update(ui);
    });
    output.drop_without_applying_deltas();

    assert_eq!(app.state.game_items.len(), 1);
    assert_eq!(app.state.selected_entry_name(), Some("Hollow Knight"));
}
