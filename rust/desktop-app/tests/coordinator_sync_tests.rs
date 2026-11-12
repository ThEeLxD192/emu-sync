mod common;

use common::{TestHarness, test_native_entry, wait_for_active_tasks};
use emusync_data_local::{ConfigManager, RecoveryStore};
use emusync_desktop_app::{AppEvent, CloseState, DesktopCoordinator, DriveEndpoints};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, Credentials},
    library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType},
    sync::{RequestId, SaveConflict},
};
use emusync_presentation::{
    actions::UiAction,
    model::{AppStatus, CloudSyncStatus, GameItem},
};
use std::sync::{Arc, mpsc::channel};
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

async fn spawn_mock_drive(
    respond_token_error: bool,
) -> (DriveEndpoints, tokio::sync::oneshot::Sender<()>) {
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
                        if let Ok((req, _)) = common::read_http_request_async(&mut stream).await {
                            if respond_token_error && req.contains("/token") {
                                let _ = common::write_http_json_async(&mut stream, 500, "Internal Server Error", "{}").await;
                            } else if req.contains("/token") {
                                let body = r#"{"access_token":"mock_token","token_type":"Bearer","expires_in":3600}"#;
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", body).await;
                            } else if req.starts_with("POST /files") {
                                let body = r#"{"id":"mock_folder_id","name":"mock_name"}"#;
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", body).await;
                            } else if req.contains("orderBy=modifiedTime") {
                                let body = r#"{"files":[]}"#;
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", body).await;
                            } else if req.contains("/files") {
                                let body = r#"{"files":[{"id":"mock_folder_id","name":"EmuSync"}]}"#;
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", body).await;
                            } else {
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", "{}").await;
                            }
                        }
                    });
                }
            }
        }
    });

    (common::make_drive_endpoints(port), stop_tx)
}

#[tokio::test]
async fn connected_launch_runs_under_sync_lifecycle() {
    let (endpoints, _stop) = spawn_mock_drive(false).await;
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

    // Deterministically wait for launch and sync tasks to finish
    wait_for_active_tasks(&coordinator).await;

    let events: Vec<_> = rx.try_iter().collect();
    let saw_syncing = events
        .iter()
        .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Syncing { .. })));
    let completed = events
        .iter()
        .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Idle)));

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
        (n.common.executable_path, n.common.arguments) = common::shell_exit(1);
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

    wait_for_active_tasks(&coordinator).await;

    let events: Vec<_> = rx.try_iter().collect();
    let error_idx = events
        .iter()
        .position(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Error { .. })));
    assert!(
        error_idx.is_some(),
        "Expected sync lifecycle to emit Error on game failure"
    );

    let idle_after_error = events[error_idx.unwrap()..]
        .iter()
        .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Idle)));
    assert!(
        !idle_after_error,
        "AppStatus::Error must not be trampled by AppStatus::Idle"
    );
}

#[tokio::test]
async fn connected_launch_with_game_exit_error_preserves_error_and_does_not_send_idle() {
    let (endpoints, _stop) = spawn_mock_drive(false).await;
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let mut entry = test_native_entry("Game");
    if let GameEntry::Native(ref mut n) = entry {
        (n.common.executable_path, n.common.arguments) = common::shell_exit(1);
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

    wait_for_active_tasks(&coordinator).await;

    let events: Vec<_> = rx.try_iter().collect();
    let error_idx = events
        .iter()
        .position(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Error { .. })));
    assert!(
        error_idx.is_some(),
        "Expected sync lifecycle to emit Error on game failure"
    );

    let idle_after_error = events[error_idx.unwrap()..]
        .iter()
        .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Idle)));
    assert!(
        !idle_after_error,
        "AppStatus::Error must not be trampled by AppStatus::Idle"
    );
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
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    let item = GameItem::new("QuickRunner", native, None);
    coordinator.handle_action(UiAction::LaunchGame(item));

    wait_for_active_tasks(&coordinator).await;

    let events: Vec<_> = rx.try_iter().collect();
    let seen_playing = events
        .iter()
        .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Playing)));
    let seen_idle = events
        .iter()
        .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Idle)));

    assert!(seen_playing);
    assert!(seen_idle);
}

#[tokio::test]
async fn execute_sync_entry_retains_uncertain_upload_error_and_blocks_close() {
    let dir = tempdir().unwrap();
    let save_path = dir.path().join("save.dat");
    std::fs::write(&save_path, b"save_data").unwrap();

    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "UncertainGame".to_string(),
            executable_path: "/bin/sh".to_string(),
            arguments: vec!["-c".to_string(), "exit 0".to_string()],
            save_paths: vec![save_path.to_string_lossy().to_string()],
            save_targets: vec![SaveTarget {
                path: save_path.to_string_lossy().to_string(),
                cloud_path: Some("save.dat".to_string()),
                kind: SaveTargetType::File,
                excluded_folders: Vec::new(),
            }],
            ..Default::default()
        },
        ..Default::default()
    });

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
                        if let Ok((req, _)) = common::read_http_request_async(&mut stream).await {
                            if req.contains("/token") {
                                let body = r#"{"access_token":"mock_token","token_type":"Bearer","expires_in":3600}"#;
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", body).await;
                            } else if req.starts_with("POST /upload") {
                                // Initiate resumable upload session
                                let session_url = format!("http://127.0.0.1:{port}/session");
                                let resp = format!(
                                    "HTTP/1.1 200 OK\r\nLocation: {session_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                                );
                                use tokio::io::AsyncWriteExt;
                                let _ = stream.write_all(resp.as_bytes()).await;
                            } else if req.starts_with("PUT /session") {
                                // Abruptly close the socket after receiving bytes -> OutcomeUnknown
                                drop(stream);
                            } else if req.contains("orderBy=modifiedTime") {
                                let body = r#"{"files":[]}"#;
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", body).await;
                            } else if req.contains("/files") {
                                let body = r#"{"files":[{"id":"mock_folder_id","name":"EmuSync"}]}"#;
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", body).await;
                            } else {
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", "{}").await;
                            }
                        }
                    });
                }
            }
        }
    });

    let endpoints = common::make_drive_endpoints(port);
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
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
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    coordinator.handle_action(UiAction::SyncEntry(entry.clone()));
    wait_for_active_tasks(&coordinator).await;

    let events: Vec<_> = rx.try_iter().collect();
    assert!(events.iter().any(|e| matches!(
        e,
        AppEvent::SyncStatusUpdated { status: CloudSyncStatus::Error, entry_name } if entry_name == "UncertainGame"
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        AppEvent::StatusChanged(AppStatus::Error { message }) if message.contains("Recovery is required")
    )));

    // Supervisor holds the transfer failure
    assert!(coordinator.supervisor.lock().await.failure().is_some());

    // Application close blocked
    assert_eq!(coordinator.poll_close(), CloseState::RecoveryRequired);

    // New actions rejected with toast
    coordinator.handle_action(UiAction::SyncEntry(entry.clone()));
    let new_events: Vec<_> = rx.try_iter().collect();
    assert!(new_events.iter().any(|e| matches!(
        e,
        AppEvent::Toast(msg) if msg.contains("in progress") || msg.contains("requires recovery")
    )));

    // Recovery record in .sync-recovery
    let recovery_store = RecoveryStore::new(manager.path()).unwrap();
    assert!(!recovery_store.list().unwrap().is_empty());

    // Original file intact
    assert_eq!(std::fs::read(&save_path).unwrap(), b"save_data");

    let _ = stop_tx.send(());
}

#[tokio::test]
async fn execute_launch_game_aborts_when_category_was_deleted_from_config() {
    let harness = TestHarness::new();
    let witness_path = harness.dir.path().join("witness.txt");

    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "DeletedCat".to_string(),
            executable_path: "/bin/sh".to_string(),
            arguments: vec![
                "-c".to_string(),
                format!("touch {}", witness_path.display()),
            ],
            ..Default::default()
        },
        ..Default::default()
    });
    let item = GameItem::new("DeletedCat", entry, None);

    harness
        .coordinator
        .handle_action(UiAction::LaunchGame(item));
    harness.wait_for_active_tasks().await;

    // Witness file must NOT have been created (executable was not run)
    assert!(!witness_path.exists());

    let events: Vec<_> = harness.event_rx.try_iter().collect();
    assert!(events.iter().any(|e| matches!(
        e,
        AppEvent::StatusChanged(AppStatus::Error { message }) if message.contains("no longer exists")
    )));

    // Admission released
    assert!(harness.coordinator.try_acquire_admission());
}

#[tokio::test]
async fn execute_open_emulator_rejects_blank_executable_and_releases_admission() {
    let harness = TestHarness::new();

    let blank_emu = Emulator {
        common: EntryCommon {
            name: "BlankEmu".into(),
            executable_path: "   ".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    harness
        .coordinator
        .handle_action(UiAction::OpenEmulator(blank_emu));
    harness.wait_for_active_tasks().await;

    let events: Vec<_> = harness.event_rx.try_iter().collect();
    assert!(events.iter().any(|e| matches!(
        e,
        AppEvent::Toast(msg) if msg.contains("No emulator executable configured")
    )));
    assert!(harness.coordinator.try_acquire_admission());
    harness.coordinator.release_admission();

    let missing_emu = Emulator {
        common: EntryCommon {
            name: "MissingEmu".into(),
            executable_path: "/nonexistent/path/to/emu_binary".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    harness
        .coordinator
        .handle_action(UiAction::OpenEmulator(missing_emu));
    harness.wait_for_active_tasks().await;

    let events2: Vec<_> = harness.event_rx.try_iter().collect();
    assert!(events2.iter().any(|e| matches!(
        e,
        AppEvent::Toast(msg) if msg.contains("Failed to launch emulator")
    )));
    assert!(harness.coordinator.try_acquire_admission());
}

#[tokio::test]
async fn execute_check_sync_entry_reports_not_configured_offline_and_does_not_clobber_active_conflict()
 {
    let (endpoints, _stop) = spawn_mock_drive(false).await;
    let harness = TestHarness::new().with_endpoints(endpoints);
    let entry = test_native_entry("Game");

    // Install an active conflict in interaction controls
    let conflict = SaveConflict {
        request_id: RequestId(1),
        local_modified_ms: 100,
        cloud_modified_ms: 200,
        game_name: "Game".into(),
    };
    harness
        .coordinator
        .interaction
        .on_conflict(conflict.clone());
    let cancel = CancellationToken::new();
    harness
        .coordinator
        .interaction
        .set_cancel_token(cancel.clone());

    // 1. Without credentials: CheckSyncEntry reports NotConfigured
    harness
        .coordinator
        .handle_action(UiAction::CheckSyncEntry(entry.clone()));
    harness.wait_for_active_tasks().await;

    let events1: Vec<_> = harness.event_rx.try_iter().collect();
    assert!(events1.iter().any(|e| matches!(
        e,
        AppEvent::SyncStatusUpdated {
            status: CloudSyncStatus::NotConfigured,
            ..
        }
    )));
    assert_eq!(
        harness
            .coordinator
            .interaction
            .current_conflict
            .lock()
            .unwrap()
            .clone(),
        Some(conflict.clone())
    );
    assert!(!cancel.is_cancelled());

    // 2. With credentials: CheckSyncEntry queries via check orchestrator without overwriting controls
    harness
        .manager
        .update(|_| AppConfig {
            credentials: Some(Credentials {
                client_id: "id".into(),
                client_secret: "secret".into(),
                refresh_token: Some("token".into()),
            }),
            entries: vec![entry.clone()],
        })
        .unwrap();

    harness
        .coordinator
        .handle_action(UiAction::CheckSyncEntry(entry));
    harness.wait_for_active_tasks().await;

    let events2: Vec<_> = harness.event_rx.try_iter().collect();
    assert!(events2.iter().any(|e| matches!(
        e,
        AppEvent::SyncStatusUpdated {
            status: CloudSyncStatus::Checking,
            ..
        }
    )));
    // Interactive conflict remains intact!
    assert_eq!(
        harness
            .coordinator
            .interaction
            .current_conflict
            .lock()
            .unwrap()
            .clone(),
        Some(conflict)
    );
    assert!(!cancel.is_cancelled());
}

/// A download that fails changes nothing uncertain: the app must say what failed, accept new
/// work and be closable. Only an upload with an unknown outcome may hold the app.
#[tokio::test]
async fn execute_sync_entry_reports_a_failed_download_without_blocking_close() {
    let dir = tempdir().unwrap();
    let save_path = dir.path().join("save.dat");
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "DownloadGame".to_string(),
            executable_path: "/bin/sh".to_string(),
            arguments: vec!["-c".to_string(), "exit 0".to_string()],
            save_paths: vec![save_path.to_string_lossy().to_string()],
            save_targets: vec![SaveTarget {
                path: save_path.to_string_lossy().to_string(),
                cloud_path: Some("save.dat".to_string()),
                kind: SaveTargetType::File,
                excluded_folders: Vec::new(),
            }],
            ..Default::default()
        },
        ..Default::default()
    });

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
                        if let Ok((req, _)) = common::read_http_request_async(&mut stream).await {
                            if req.contains("/token") {
                                let body = r#"{"access_token":"mock_token","token_type":"Bearer","expires_in":3600}"#;
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", body).await;
                            } else if req.contains("alt=media") {
                                let _ = common::write_http_json_async(&mut stream, 500, "Internal Server Error", "{}").await;
                            } else if req.contains("google-apps.folder") {
                                let body = r#"{"files":[{"id":"mock_folder_id","name":"EmuSync"}]}"#;
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", body).await;
                            } else if req.contains("parents") {
                                let body = format!(
                                    r#"{{"files":[{{"id":"remote1","name":"save.dat","modifiedTime":"2026-01-01T00:00:00.000Z","mimeType":"application/octet-stream","sha256Checksum":"{}","version":"1"}}]}}"#,
                                    "ab".repeat(32)
                                );
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", &body).await;
                            } else {
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", "{}").await;
                            }
                        }
                    });
                }
            }
        }
    });

    let endpoints = common::make_drive_endpoints(port);
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
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
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    coordinator.handle_action(UiAction::SyncEntry(entry.clone()));
    wait_for_active_tasks(&coordinator).await;

    let events: Vec<_> = rx.try_iter().collect();
    assert!(events.iter().any(|e| matches!(
        e,
        AppEvent::SyncStatusUpdated { status: CloudSyncStatus::Error, entry_name } if entry_name == "DownloadGame"
    )), "{events:?}");
    assert!(
        events.iter().any(|e| matches!(
            e,
            AppEvent::StatusChanged(AppStatus::Error { message })
                if message.starts_with("Sync failed:") && !message.contains("Recovery is required")
        )),
        "{events:?}"
    );

    // Nothing is retained, nothing is left to recover, and the app can close.
    assert!(coordinator.supervisor.lock().await.failure().is_none());
    assert_eq!(coordinator.poll_close(), CloseState::Ready);
    assert!(
        RecoveryStore::new(manager.path())
            .unwrap()
            .list()
            .unwrap()
            .is_empty()
    );
    assert!(!save_path.exists());

    let _ = stop_tx.send(());
}
