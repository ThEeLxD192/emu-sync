mod common;

use common::{TestHarness, test_emulator_entry, test_native_entry, wait_for_active_tasks};
use emusync_data_drive::{ConditionalError, TransferError};
use emusync_data_local::{
    ConfigManager, RecoveryRecord, RecoveryStore, TrackedError, UploadContext, UploadSnapshot,
};
use emusync_desktop_app::{
    AppEvent, CloseState, DesktopCoordinator, RecoveryStatus,
    coordinator::interaction::{AsyncSaveSetupBridge, RetryRunner},
    scan_entry_games,
};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, CredentialStore, Credentials},
    library::{SaveTarget, SaveTargetType},
    sync::{PostSyncFailure, RemoteState, RequestId, SaveSetup, SaveSetupRequest},
};
use emusync_presentation::{EmuSyncApp, actions::UiAction, model::AppStatus};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::channel,
};
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

#[test]
fn displayed_version_matches_application_package() {
    let app = EmuSyncApp::new();
    assert_eq!(app.state.current_version, env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn invalid_drive_connect_is_rejected_and_preserves_credentials() {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry = test_native_entry("Game");
    let original = AppConfig {
        language: Default::default(),
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
    // Development builds without a controlled release server never contact GitHub.
    assert!(events.iter().any(|event| matches!(event, AppEvent::Toast(message) if message.contains("disabled in development builds"))));
}

#[tokio::test]
async fn coordinator_initializes_and_scans_library() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();

    let entry = test_emulator_entry("SNES");
    manager
        .update(|_| AppConfig {
            language: Default::default(),
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

    // Configuration and Steam events are sent before `initialize` returns; the game scan
    // reports from a blocking worker, so wait for it instead of racing it.
    let scanned = |e: &AppEvent| matches!(e, AppEvent::EntryGamesScanned { entry_name, .. } if entry_name == "SNES");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut events = Vec::new();
    while !events.iter().any(scanned) {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(event) => events.push(event),
            Err(error) => panic!("no scan event for SNES ({error}); got {events:?}"),
        }
    }
    events.extend(rx.try_iter());

    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEvent::ConfigReloaded(_)))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEvent::SteamRegistrationUpdated { entry_name, .. } if entry_name == "SNES"))
    );
}

#[test]
fn app_processes_events_and_renders_cleanly() {
    let mut app = EmuSyncApp::new();
    let entry = test_native_entry("Hollow Knight");
    let games = scan_entry_games(&entry);

    app.state.config = Some(AppConfig {
        language: Default::default(),
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

#[tokio::test]
async fn clear_status_does_not_emit_idle_while_operation_admitted_or_pending_recovery() {
    let harness = TestHarness::new();
    let cancel = CancellationToken::new();
    harness
        .coordinator
        .interaction
        .set_cancel_token(cancel.clone());

    // Scenario (a): is_admitted = true -> token is cancelled, but NO AppStatus::Idle is emitted
    harness
        .coordinator
        .is_admitted
        .store(true, Ordering::SeqCst);
    harness.coordinator.handle_action(UiAction::ClearStatus);

    assert!(cancel.is_cancelled());
    let events_a: Vec<_> = harness.event_rx.try_iter().collect();
    assert!(
        !events_a
            .iter()
            .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Idle)))
    );

    // Scenario (b): pending_recovery is Some -> NO AppStatus::Idle is emitted
    harness.coordinator.release_admission();
    harness
        .coordinator
        .record_pending_recovery("unreconciled failure".into());
    harness.coordinator.handle_action(UiAction::ClearStatus);

    let events_b: Vec<_> = harness.event_rx.try_iter().collect();
    assert!(
        !events_b
            .iter()
            .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Idle)))
    );

    // Scenario (c): Idle state (!is_admitted and pending_recovery is None) -> AppStatus::Idle is emitted
    harness.coordinator.clear_pending_recovery();
    harness.coordinator.handle_action(UiAction::ClearStatus);

    let events_c: Vec<_> = harness.event_rx.try_iter().collect();
    assert!(
        events_c
            .iter()
            .any(|e| matches!(e, AppEvent::StatusChanged(AppStatus::Idle)))
    );
}

#[tokio::test]
async fn retry_post_sync_retains_transfer_error_and_restores_runner_when_busy() {
    let harness = TestHarness::new();

    let executed = Arc::new(AtomicBool::new(false));
    let executed_clone = executed.clone();
    let failure = PostSyncFailure {
        request_id: RequestId(1),
        game_name: "Metroid".into(),
        reason: "network glitch".into(),
        completed_save_count: 1,
        pending_save_count: 1,
    };
    let runner: RetryRunner = Arc::new(move |_| {
        let flag = executed_clone.clone();
        Box::pin(async move {
            flag.store(true, Ordering::SeqCst);
        })
    });

    harness
        .coordinator
        .interaction
        .set_retry(failure.clone(), runner);
    assert!(harness.coordinator.interaction.has_retry());

    // 1. When coordinator is busy (admission held)
    assert!(harness.coordinator.try_acquire_admission());
    harness.coordinator.handle_action(UiAction::RetryPostSync);

    // Toast emitted and runner was reinstated
    let events: Vec<_> = harness.event_rx.try_iter().collect();
    assert!(events.iter().any(|e| matches!(
        e,
        AppEvent::Toast(msg) if msg.contains("operation in progress or recovery required")
    )));
    assert!(!executed.load(Ordering::SeqCst));
    assert!(harness.coordinator.interaction.has_retry());

    // 2. Release admission and retry again
    harness.coordinator.release_admission();
    harness.coordinator.handle_action(UiAction::RetryPostSync);

    wait_for_active_tasks(&harness.coordinator).await;

    // Runner executed and retry was consumed
    assert!(executed.load(Ordering::SeqCst));
    assert!(!harness.coordinator.interaction.has_retry());
    assert!(harness.coordinator.try_acquire_admission());
}

#[tokio::test]
async fn reconcile_active_and_reconcile_persisted_recover_uploads_and_clear_pending_recovery() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();

    let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let hash_clone = hash.to_string();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                res = listener.accept() => {
                    let Ok((mut stream, _)) = res else { break; };
                    let h = hash_clone.clone();
                    tokio::spawn(async move {
                        if let Ok((req, _)) = common::read_http_request_async(&mut stream).await {
                            if req.contains("/token") {
                                let body = r#"{"access_token":"mock_token","token_type":"Bearer","expires_in":3600}"#;
                                let _ = common::write_http_json_async(&mut stream, 200, "OK", body).await;
                            } else if req.contains("/files/old?") || req.contains("/files/persisted_id?") {
                                let id = if req.contains("old") { "old" } else { "persisted_id" };
                                let name = if req.contains("old") { "active.dat" } else { "persisted.dat" };
                                let body = format!(
                                    r#"{{"id":"{id}","name":"{name}","parents":["folder"],"modifiedTime":"2026-01-01T00:00:00Z","sha256Checksum":"{h}","version":"1"}}"#
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
    let harness = TestHarness::new().with_endpoints(endpoints);

    let active_save = harness.dir.path().join("active.dat");
    std::fs::write(&active_save, b"abc").unwrap();
    let active_snap = UploadSnapshot::prepare(&active_save).unwrap();
    let active_ctx = UploadContext {
        local: active_save.clone(),
        folder: "folder".into(),
        remote_name: "active.dat".into(),
        observed: Some(RemoteState {
            id: "old".into(),
            modified: 100,
            sha256: None,
            version: None,
        }),
        recovery_id: None,
    };

    harness
        .manager
        .update(|_| AppConfig {
            language: Default::default(),
            credentials: Some(Credentials {
                client_id: "id".into(),
                client_secret: "secret".into(),
                refresh_token: Some("token".into()),
            }),
            entries: vec![],
        })
        .unwrap();

    // Part 1: reconcile_active
    harness
        .coordinator
        .record_pending_recovery("uncertain upload".into());
    harness
        .coordinator
        .record_transfer_error(TrackedError::Upload {
            source: TransferError::Conditional(ConditionalError::HeadersTimedOut),
            snapshot: active_snap,
            context: Box::new(active_ctx),
        });

    assert_eq!(
        harness.coordinator.poll_close(),
        CloseState::RecoveryRequired
    );

    let rec_status = harness.coordinator.reconcile_active().await.unwrap();
    assert_eq!(rec_status, RecoveryStatus::Recovered);

    // pending_recovery cleared and close state is Ready
    assert!(
        harness
            .coordinator
            .pending_recovery
            .lock()
            .unwrap()
            .is_none()
    );
    assert_eq!(harness.coordinator.poll_close(), CloseState::Ready);

    // Part 2: reconcile_persisted via initialize()
    let persisted_save = harness.dir.path().join("persisted.dat");
    std::fs::write(&persisted_save, b"abc").unwrap();
    let persisted_snap = UploadSnapshot::prepare(&persisted_save).unwrap();

    let creds = harness.manager.load().unwrap();
    let snapshot = harness.manager.snapshot(&creds).unwrap();
    let rec_creds = emusync_data_local::RecoveryCredentials::new(
        Arc::new(harness.manager.clone()),
        snapshot,
        creds,
    );

    let recovery_store = RecoveryStore::new(harness.manager.path()).unwrap();
    let rec_record = RecoveryRecord {
        remote_id: Some("persisted_id".into()),
        local: persisted_save.to_str().unwrap().into(),
        folder: "folder".into(),
        remote_name: "persisted.dat".into(),
        digest: persisted_snap.digest().into(),
        connection: rec_creds.fingerprint(),
        observed: None,
    };
    recovery_store.stage(&persisted_snap, rec_record).unwrap();
    assert!(!recovery_store.list().unwrap().is_empty());

    // initialize() detects pending recovery, triggers background reconcile_persisted and clears it
    harness.coordinator.initialize();
    wait_for_active_tasks(&harness.coordinator).await;

    assert!(recovery_store.list().unwrap().is_empty());
    assert!(
        harness
            .coordinator
            .pending_recovery
            .lock()
            .unwrap()
            .is_none()
    );
    assert_eq!(harness.coordinator.poll_close(), CloseState::Ready);

    let _ = stop_tx.send(());
}

#[tokio::test]
async fn complete_save_setup_cancels_bridge_when_category_deleted_or_persists_and_unblocks_when_valid()
 {
    let harness = TestHarness::new();
    let entry = test_native_entry("Zelda");
    harness
        .manager
        .update(|_| AppConfig {
            language: Default::default(),
            credentials: None,
            entries: vec![entry.clone()],
        })
        .unwrap();

    // Case 1: Category deleted before complete_save_setup
    let game_deleted = emusync_domain::sync::SyncGame {
        name: "Zelda".into(),
        entry: test_native_entry("NonExistentCategory"),
        rom_path: None,
        effective_title: "Zelda".into(),
        save_targets: vec![],
    };
    let controls1 = harness.coordinator.interaction.clone();
    let bridge_task_1 = tokio::spawn(async move {
        let b = AsyncSaveSetupBridge {
            controls: controls1,
        };
        b.request(SaveSetupRequest {
            game: game_deleted.clone(),
            cloud_file_names: vec!["save.srm".into()],
        })
        .await
    });

    // Wait until bridge event is sent
    let req_event = harness
        .recv_event_matching(|e| matches!(e, AppEvent::SaveSetupRequested { .. }))
        .await;
    let AppEvent::SaveSetupRequested { request: req1 } = req_event else {
        unreachable!()
    };

    // Complete setup for non-existent category
    harness
        .coordinator
        .handle_action(UiAction::CompleteSaveSetup {
            request: req1,
            targets: vec![],
            title: None,
            cover_path: None,
            restore: None,
        });

    // Bridge task 1 terminates with error
    let res1 = bridge_task_1.await.unwrap();
    assert!(res1.is_err(), "Expected save setup to be cancelled");

    let events: Vec<_> = harness.event_rx.try_iter().collect();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEvent::Toast(msg) if msg.contains("no longer exists")))
    );

    // Case 2: Category exists, valid setup persists target and unblocks bridge
    let game_valid = emusync_domain::sync::SyncGame {
        name: "Zelda".into(),
        entry: entry.clone(),
        rom_path: None,
        effective_title: "Zelda".into(),
        save_targets: vec![],
    };
    let controls2 = harness.coordinator.interaction.clone();
    let bridge_task_2 = tokio::spawn(async move {
        let b = AsyncSaveSetupBridge {
            controls: controls2,
        };
        b.request(SaveSetupRequest {
            game: game_valid,
            cloud_file_names: vec!["zelda.sav".into()],
        })
        .await
    });

    let req_event2 = harness
        .recv_event_matching(|e| matches!(e, AppEvent::SaveSetupRequested { .. }))
        .await;
    let AppEvent::SaveSetupRequested { request: req2 } = req_event2 else {
        unreachable!()
    };

    let new_target = SaveTarget {
        path: "/saves/zelda.sav".into(),
        cloud_path: Some("zelda.sav".into()),
        kind: SaveTargetType::File,
        excluded_folders: Vec::new(),
    };
    harness
        .coordinator
        .handle_action(UiAction::CompleteSaveSetup {
            request: req2,
            targets: vec![new_target.clone()],
            title: Some("Zelda DX".into()),
            cover_path: None,
            restore: None,
        });

    // Bridge task 2 resolves with Ok(result)
    let res2 = bridge_task_2
        .await
        .unwrap()
        .expect("Save setup must succeed");
    assert_eq!(res2.targets.len(), 1);
    assert_eq!(res2.targets[0].path, "/saves/zelda.sav");

    // Config reloaded and contains the new target under renamed title
    let loaded = harness.manager.load().unwrap();
    let saved_entry = loaded
        .entries
        .iter()
        .find(|e| e.common().name == "Zelda DX")
        .unwrap();
    assert_eq!(saved_entry.common().save_targets.len(), 1);
    assert_eq!(
        saved_entry.common().save_targets[0].path,
        "/saves/zelda.sav"
    );
}

#[tokio::test]
async fn complete_save_setup_without_a_pending_request_writes_nothing() {
    let harness = TestHarness::new();
    let entry = test_native_entry("Zelda");
    harness
        .manager
        .update(|_| AppConfig {
            language: Default::default(),
            credentials: None,
            entries: vec![entry.clone()],
        })
        .unwrap();
    let before = harness.manager.load().unwrap();

    harness
        .coordinator
        .handle_action(UiAction::CompleteSaveSetup {
            request: Box::new(emusync_presentation::model::SaveSetupRequest {
                id: 7,
                item: emusync_presentation::model::GameItem::new("Zelda", entry, None),
                notice: None,
                cloud_file_names: vec![],
                targets: vec![],
            }),
            targets: vec![SaveTarget {
                path: "/saves/orphan.sav".into(),
                cloud_path: None,
                kind: SaveTargetType::File,
                excluded_folders: Vec::new(),
            }],
            title: Some("Orphan".into()),
            cover_path: None,
            restore: None,
        });

    assert_eq!(harness.manager.load().unwrap(), before);
    let events: Vec<_> = harness.event_rx.try_iter().collect();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AppEvent::Toast(_) | AppEvent::ConfigReloaded(_))),
        "{events:?}"
    );
}
