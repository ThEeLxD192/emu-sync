use emusync_data_local::ConfigManager;
use emusync_desktop_app::{AppEvent, DesktopCoordinator, DriveEndpoints, scan_entry_games};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, Credentials},
    library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType},
};
use emusync_presentation::{
    EmuSyncApp,
    actions::UiAction,
    model::{AppStatus, GameItem},
};
use std::{
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::channel,
    },
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

#[test]
fn displayed_version_matches_application_package() {
    let app = EmuSyncApp::new();
    assert_eq!(app.state.current_version, env!("CARGO_PKG_VERSION"));
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
async fn invalid_drive_connect_is_rejected_and_preserves_credentials() {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry = test_native_entry("Game");
    let original = AppConfig {
        credentials: Some(Credentials {
            client_id: "existing-id".into(),
            client_secret: "existing-secret".into(),
            refresh_token: Some("existing-token".into()),
        }),
        entries: vec![entry.clone()],
    };
    manager.update(|_| original.clone()).unwrap();
    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    coordinator.handle_action(UiAction::ConnectDrive {
        client_id: "".into(),
        client_secret: "".into(),
    });
    coordinator.handle_action(UiAction::CheckForUpdates { manual: true });

    assert_eq!(manager.load().unwrap(), original);
    let events: Vec<_> = rx.try_iter().collect();
    assert!(events.iter().any(
        |event| matches!(event, AppEvent::Toast(message) if message.contains("cannot be empty"))
    ));
    assert!(events.iter().any(|event| matches!(event, AppEvent::Toast(message) if message.contains("Updates are not yet available"))));
}

fn mock_loopback_drive(respond_token_error: bool) -> (DriveEndpoints, std::sync::mpsc::Sender<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop_tx, stop_rx) = channel();

    std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while stop_rx.try_recv().is_err() {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                if respond_token_error && req.contains("/token") {
                    let _ = stream.write_all(
                        b"HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                    );
                } else if req.contains("/token") {
                    let body =
                        r#"{"access_token":"mock_token","token_type":"Bearer","expires_in":3600}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                } else if req.starts_with("POST /files") {
                    let body = r#"{"id":"mock_folder_id","name":"mock_name"}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                } else if req.contains("orderBy=modifiedTime") {
                    let body = r#"{"files":[]}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                } else if req.contains("/files") {
                    let body = r#"{"files":[{"id":"mock_folder_id","name":"EmuSync"}]}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                } else {
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                    );
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });

    let url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}")).unwrap();
    let endpoints = DriveEndpoints {
        files_v3: url.join("/files").unwrap(),
        upload_v3: url.join("/upload").unwrap(),
        files_v2: url.join("/v2/files").unwrap(),
        upload_v2: url.join("/v2/upload").unwrap(),
        token_endpoint: url.join("/token").unwrap(),
        authorization_endpoint: url.join("/auth").unwrap(),
        oauth_address: format!("127.0.0.1:{port}").parse().unwrap(),
        redirect_uri: format!("http://127.0.0.1:{port}/callback"),
    };
    (endpoints, stop_tx)
}

#[tokio::test]
async fn connected_launch_runs_under_sync_lifecycle() {
    let (endpoints, _stop) = mock_loopback_drive(false);
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry = test_native_entry("Game");
    manager
        .update(|_| AppConfig {
            credentials: Some(Credentials {
                client_id: "id".into(),
                client_secret: "secret".into(),
                refresh_token: Some("token".into()),
            }),
            entries: vec![entry.clone()],
        })
        .unwrap();
    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    coordinator.handle_action(UiAction::LaunchGame(GameItem::new("Game", entry, None)));

    // Verify the full lifecycle: Syncing -> Idle/Error without hanging
    let mut saw_syncing = false;
    let mut completed = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        while let Ok(event) = rx.try_recv() {
            if matches!(event, AppEvent::StatusChanged(AppStatus::Syncing { .. })) {
                saw_syncing = true;
            }
            if matches!(event, AppEvent::StatusChanged(AppStatus::Idle)) {
                completed = true;
                break;
            }
        }
        if completed {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        saw_syncing,
        "Expected sync lifecycle to enter Syncing status"
    );
    assert!(completed, "Expected sync lifecycle to complete to Idle");
}

#[tokio::test]
async fn failed_game_process_emits_error_and_does_not_send_idle() {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let mut entry = test_native_entry("Game");
    if let GameEntry::Native(ref mut n) = entry {
        n.common.arguments = vec!["-c".to_string(), "exit 1".to_string()];
    }
    manager
        .update(|_| AppConfig {
            credentials: None,
            entries: vec![entry.clone()],
        })
        .unwrap();
    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    coordinator.handle_action(UiAction::LaunchGame(GameItem::new("Game", entry, None)));

    let mut saw_error = false;
    let mut saw_idle_after_error = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        while let Ok(event) = rx.try_recv() {
            if matches!(event, AppEvent::StatusChanged(AppStatus::Error { .. })) {
                saw_error = true;
            } else if saw_error && matches!(event, AppEvent::StatusChanged(AppStatus::Idle)) {
                saw_idle_after_error = true;
            }
        }
        if saw_error {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            while let Ok(event) = rx.try_recv() {
                if matches!(event, AppEvent::StatusChanged(AppStatus::Idle)) {
                    saw_idle_after_error = true;
                }
            }
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert!(
        saw_error,
        "Expected sync lifecycle to emit Error on game failure"
    );
    assert!(
        !saw_idle_after_error,
        "AppStatus::Error must not be trampled by AppStatus::Idle"
    );
}

#[tokio::test]
async fn connected_launch_with_game_exit_error_preserves_error_and_does_not_send_idle() {
    let (endpoints, _stop) = mock_loopback_drive(false);
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let mut entry = test_native_entry("Game");
    if let GameEntry::Native(ref mut n) = entry {
        n.common.arguments = vec!["-c".to_string(), "exit 1".to_string()];
    }
    manager
        .update(|_| AppConfig {
            credentials: Some(Credentials {
                client_id: "id".into(),
                client_secret: "secret".into(),
                refresh_token: Some("token".into()),
            }),
            entries: vec![entry.clone()],
        })
        .unwrap();
    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    coordinator.handle_action(UiAction::LaunchGame(GameItem::new("Game", entry, None)));

    let mut saw_error = false;
    let mut saw_idle_after_error = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        while let Ok(event) = rx.try_recv() {
            if matches!(event, AppEvent::StatusChanged(AppStatus::Error { .. })) {
                saw_error = true;
            } else if saw_error && matches!(event, AppEvent::StatusChanged(AppStatus::Idle)) {
                saw_idle_after_error = true;
            }
        }
        if saw_error {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            while let Ok(event) = rx.try_recv() {
                if matches!(event, AppEvent::StatusChanged(AppStatus::Idle)) {
                    saw_idle_after_error = true;
                }
            }
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert!(
        saw_error,
        "Expected sync lifecycle to emit Error on game failure"
    );
    assert!(
        !saw_idle_after_error,
        "AppStatus::Error must not be trampled by AppStatus::Idle"
    );
}

#[tokio::test]
async fn connected_edits_do_not_change_paths_without_cloud_migration() {
    let (endpoints, _stop) = mock_loopback_drive(true);
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry = test_native_entry("Game");
    let original = AppConfig {
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![entry.clone()],
    };
    manager.update(|_| original.clone()).unwrap();
    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    coordinator.handle_action(UiAction::EditEntry {
        old: Box::new(entry.clone()),
        new: Box::new(test_native_entry("Renamed")),
    });

    // Wait for the migration task to report failure
    let mut saw_migrating = false;
    let mut saw_failure = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        while let Ok(event) = rx.try_recv() {
            if matches!(event, AppEvent::StatusChanged(AppStatus::Syncing { ref message }) if message.contains("Migrating cloud folder"))
            {
                saw_migrating = true;
            }
            if matches!(event, AppEvent::StatusChanged(AppStatus::Error { .. })) {
                saw_failure = true;
                break;
            }
        }
        if saw_failure {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert!(
        saw_migrating,
        "Expected to see Migrating cloud folder status"
    );
    assert!(
        saw_failure,
        "Expected migration to fail against mock error server"
    );
    // Configuration must remain untouched after failed cloud migration
    assert_eq!(manager.load().unwrap(), original);
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
