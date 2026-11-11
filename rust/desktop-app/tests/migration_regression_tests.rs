mod common;

use common::{
    read_http_request_async, test_native_entry, wait_for_active_tasks, write_http_json_async,
};
use emusync_data_local::ConfigManager;
#[cfg(unix)]
use emusync_desktop_app::CloseState;
use emusync_desktop_app::{AppEvent, DesktopCoordinator, DriveEndpoints};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, Credentials},
    library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType},
};
use emusync_presentation::{
    actions::UiAction,
    model::{AppStatus, GameItem},
};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc::channel},
};
use tempfile::tempdir;

async fn spawn_failing_token_server() -> (DriveEndpoints, tokio::sync::oneshot::Sender<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                res = listener.accept() => {
                    let Ok((mut stream, _)) = res else { break; };
                    tokio::spawn(async move {
                        if let Ok((_req, _)) = read_http_request_async(&mut stream).await {
                            let _ = write_http_json_async(&mut stream, 500, "Internal Server Error", "{}").await;
                        }
                    });
                }
            }
        }
    });

    (common::make_drive_endpoints(port), stop_tx)
}

fn test_native(name: &str, group: Option<&str>, cloud_folder: Option<&str>) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: "/bin/true".to_string(),
            arguments: vec![],
            save_paths: vec!["/tmp/saves".to_string()],
            save_targets: vec![],
            group: group.map(str::to_string),
            cloud_folder: cloud_folder.map(str::to_string),
            drive_file_id: None,
            steam_app_id: None,
        },
        cover_path: None,
        wait_for_process: None,
    })
}

#[tokio::test]
async fn token_server_failure_leaves_config_intact_and_emits_error() {
    let (endpoints, _stop) = spawn_failing_token_server().await;
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry = test_native("OriginalGame", None, None);
    let initial_config = AppConfig {
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![entry.clone()],
    };
    manager.update(|_| initial_config).unwrap();
    let original_config = manager.load().unwrap();

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
        new: Box::new(test_native("RenamedGame", None, None)),
    });

    wait_for_active_tasks(&coordinator).await;

    let events: Vec<_> = rx.try_iter().collect();
    let saw_syncing = events
        .iter()
        .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Syncing { .. })));
    let saw_error = events
        .iter()
        .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Error { .. })));

    assert!(saw_syncing, "Expected syncing event for cloud migration");
    assert!(saw_error, "Expected migration to fail with Error status");

    // Config must remain identical to original
    assert_eq!(manager.load().unwrap(), original_config);

    // No success toast was emitted
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AppEvent::Toast(msg) if msg.contains("updated")))
    );
}

#[tokio::test]
async fn cloud_folder_change_without_name_change_triggers_migration() {
    let (endpoints, _stop) = spawn_failing_token_server().await;
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry_old = test_native("SameName", None, None);
    let entry_new = test_native("SameName", None, Some("custom_folder"));

    let original_config = AppConfig {
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![entry_old.clone()],
    };
    manager.update(|_| original_config.clone()).unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    coordinator.handle_action(UiAction::EditEntry {
        old: Box::new(entry_old),
        new: Box::new(entry_new),
    });

    wait_for_active_tasks(&coordinator).await;

    let events: Vec<_> = rx.try_iter().collect();
    let saw_syncing = events
        .iter()
        .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Syncing { .. })));
    assert!(
        saw_syncing,
        "Expected migration to be triggered by cloud folder change"
    );
}

#[tokio::test]
async fn renaming_grouped_native_game_triggers_migration() {
    let (endpoints, _stop) = spawn_failing_token_server().await;
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry_old = test_native("OldGame", Some("GroupG"), None);
    let entry_new = test_native("NewGame", Some("GroupG"), None);

    let original_config = AppConfig {
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![entry_old.clone()],
    };
    manager.update(|_| original_config.clone()).unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    coordinator.handle_action(UiAction::EditEntry {
        old: Box::new(entry_old),
        new: Box::new(entry_new),
    });

    wait_for_active_tasks(&coordinator).await;

    let events: Vec<_> = rx.try_iter().collect();
    let saw_syncing = events
        .iter()
        .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Syncing { .. })));
    assert!(
        saw_syncing,
        "Expected cloud migration to be triggered by renaming grouped native game"
    );
}

#[tokio::test]
async fn connected_edits_do_not_change_paths_without_cloud_migration() {
    let (endpoints, _stop) = spawn_failing_token_server().await;
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

    wait_for_active_tasks(&coordinator).await;

    let events: Vec<_> = rx.try_iter().collect();
    let saw_migrating = events.iter().any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Syncing { message }) if message.contains("Moving cloud saves") || message.contains("Migrating cloud folder")));
    let saw_failure = events
        .iter()
        .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Error { .. })));

    assert!(saw_migrating, "Expected to see cloud migration status");
    assert!(
        saw_failure,
        "Expected migration to fail against mock error server"
    );
    assert_eq!(manager.load().unwrap(), original);
}

// The local save failure is injected with a read-only Unix directory.
#[cfg(unix)]
#[tokio::test]
async fn cloud_migration_entry_records_pending_recovery_when_local_config_save_fails_after_remote_rename()
 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();

    let dir = tempdir().unwrap();
    let dir_path = dir.path().to_path_buf();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                res = listener.accept() => {
                    let Ok((mut stream, _)) = res else { break; };
                    let dir_for_perm = dir_path.clone();
                    tokio::spawn(async move {
                        if let Ok((req, _)) = read_http_request_async(&mut stream).await {
                            if req.contains("/token") {
                                let body = r#"{"access_token":"mock_token","token_type":"Bearer","expires_in":3600}"#;
                                let _ = write_http_json_async(&mut stream, 200, "OK", body).await;
                            } else {
                                // Folder search or rename request:
                                // Make the directory read-only before or upon sending response
                                #[cfg(unix)]
                                {
                                    use std::os::unix::fs::PermissionsExt;
                                    let _ = std::fs::set_permissions(&dir_for_perm, std::fs::Permissions::from_mode(0o555));
                                }
                                let body = r#"{"id":"folder_id","name":"NewCat"}"#;
                                let _ = write_http_json_async(&mut stream, 200, "OK", body).await;
                            }
                        }
                    });
                }
            }
        }
    });

    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();
    let old_entry = test_native("OldCat", None, None);
    manager
        .update(|_| AppConfig {
            credentials: Some(Credentials {
                client_id: "id".into(),
                client_secret: "secret".into(),
                refresh_token: Some("token".into()),
            }),
            entries: vec![old_entry.clone()],
        })
        .unwrap();

    let (tx, rx) = channel();
    let endpoints = common::make_drive_endpoints(port);
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    coordinator.handle_action(UiAction::EditEntry {
        old: Box::new(old_entry),
        new: Box::new(test_native("NewCat", None, None)),
    });

    wait_for_active_tasks(&coordinator).await;

    // Restore permissions for cleanup
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755));
    }

    let events: Vec<_> = rx.try_iter().collect();
    let has_recovery_error = events.iter().any(|e| {
        matches!(
            e,
            AppEvent::StatusChanged(AppStatus::Error { message })
                if message.contains("Cloud folder was renamed, but local configuration save failed")
                    && message.contains("Recovery is required.")
        )
    });
    assert!(
        has_recovery_error,
        "Expected Error status indicating recovery is required after local config save failure"
    );

    // Pending recovery is recorded
    assert!(coordinator.pending_recovery.lock().unwrap().is_some());

    // Application close blocked
    assert_eq!(coordinator.poll_close(), CloseState::RecoveryRequired);

    // New actions rejected
    assert!(!coordinator.try_acquire_admission());

    let _ = stop_tx.send(());
}

#[tokio::test]
async fn cloud_migration_game_override_retains_transfer_error_and_leaves_config_untouched() {
    let (endpoints, _stop) = spawn_failing_token_server().await;
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();

    let emu = Emulator {
        common: EntryCommon {
            name: "SNES".into(),
            executable_path: "/bin/true".into(),
            save_paths: vec!["/tmp/saves".into()],
            ..Default::default()
        },
        roms_directory: "/tmp/roms".into(),
        extensions: vec!["sfc".into()],
        ..Default::default()
    };
    let entry = GameEntry::Emulator(emu);
    let initial_config = AppConfig {
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![entry.clone()],
    };
    manager.update(|_| initial_config.clone()).unwrap();
    let initial_config = manager.load().unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    let item = GameItem::new("Chrono", entry, Some(PathBuf::from("/tmp/roms/Chrono.sfc")));

    coordinator.handle_action(UiAction::EditGameOverride {
        item,
        title: Some("Chrono Trigger New".into()),
        targets: vec![SaveTarget {
            path: "/saves/chrono.srm".into(),
            cloud_path: Some("chrono.srm".into()),
            kind: SaveTargetType::File,
        }],
        cover_path: None,
    });

    wait_for_active_tasks(&coordinator).await;

    let events: Vec<_> = rx.try_iter().collect();
    let error_emitted = events.iter().any(|e| matches!(
        e,
        AppEvent::StatusChanged(AppStatus::Error { message }) if message.contains("not saved") || message.contains("failed")
    ));
    assert!(
        error_emitted,
        "Expected Error status indicating game overrides were not saved"
    );

    // Config must remain completely untouched
    assert_eq!(manager.load().unwrap(), initial_config);
}
