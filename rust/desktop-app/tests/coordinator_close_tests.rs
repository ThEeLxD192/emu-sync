use emusync_data_local::{ConfigManager, RecoveryCredentials, TrackedError};
use emusync_desktop_app::{
    AuthorizedTransferFailure, CloseState, DesktopCoordinator, OperationFailure,
    OperationSupervisor,
};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, CredentialStore, Credentials},
    library::{EntryCommon, GameEntry, NativeGame},
};
use emusync_presentation::{actions::UiAction, model::GameItem};
use std::{io, sync::Arc, sync::mpsc::channel};
use tempfile::tempdir;

#[tokio::test]
async fn coordinator_close_with_in_flight_operation_and_recovery_flow() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "NativeGame".into(),
            executable_path: "/bin/true".into(),
            arguments: vec![],
            save_paths: vec!["/tmp/saves".into()],
            save_targets: vec![],
            group: None,
            cloud_folder: None,
            drive_file_id: None,
            steam_app_id: None,
        },
        cover_path: None,
        wait_for_process: None,
    });
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![entry.clone()],
    };
    manager.update(|_| config.clone()).unwrap();

    let supervisor = Arc::new(tokio::sync::Mutex::new(OperationSupervisor::new()));
    let (tx, _rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_supervisor(supervisor.clone());

    // 1. Initially when no tasks or errors exist, poll_close is Ready
    assert_eq!(coordinator.poll_close(), CloseState::Ready);

    // 2. Acquire admission manually to simulate in-flight transfer
    assert!(coordinator.try_acquire_admission());
    // While in-flight, begin_close is signaled
    coordinator.begin_close();

    // begin_close marks closing: new operations must be rejected
    assert!(!coordinator.try_acquire_admission());

    // poll_close should be Pending while supervisor is marked closing with in-flight worker
    // or while coordinator holds admission
    assert_eq!(coordinator.poll_close(), CloseState::Pending);

    // 3. Simulate uncertain failure retained in supervisor
    let snapshot = manager.snapshot(&config).unwrap();
    let rec_creds = RecoveryCredentials::new(Arc::new(manager.clone()), snapshot, config);
    let failure = OperationFailure::Operation(AuthorizedTransferFailure::new(
        TrackedError::Local(io::Error::other("uncertain I/O failure")),
        rec_creds,
    ));
    supervisor.lock().await.retain_failure(failure);

    // Release temporary admission permit
    coordinator.release_admission();

    // 4. While an unrecovered failure exists, poll_close must return RecoveryRequired
    assert_eq!(coordinator.poll_close(), CloseState::RecoveryRequired);

    // New mutations are rejected when RecoveryRequired
    coordinator.handle_action(UiAction::LaunchGame(GameItem::new(
        "NativeGame",
        entry,
        None,
    )));
    // Exclusion is still held by RecoveryRequired
    assert!(!coordinator.try_acquire_admission());

    // 5. Once recovery completes and clears supervisor failure, poll_close returns Ready
    let resolved = supervisor.lock().await.resolve();
    assert!(resolved);
    assert_eq!(coordinator.poll_close(), CloseState::Ready);
    assert!(
        coordinator.try_acquire_admission(),
        "Admission should be acquirable again after recovery is resolved"
    );
    coordinator.release_admission();
}

#[tokio::test]
async fn begin_close_blocks_new_operations_and_waits_for_barrier_cleanup() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();
    manager.update(|_| AppConfig::default()).unwrap();

    let (tx, _rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let barrier_clone = barrier.clone();

    assert!(coordinator.try_acquire_admission());
    let coord_clone = coordinator.clone();
    let task = tokio::spawn(async move {
        // Wait at barrier to hold cleanup
        barrier_clone.wait().await;
        coord_clone.release_admission();
    });
    coordinator.track_task(task);

    // Close requested while task is held at barrier
    coordinator.begin_close();

    // New operation admission is strictly rejected
    assert!(!coordinator.try_acquire_admission());

    // poll_close remains Pending while the task is blocked at the barrier
    assert_eq!(coordinator.poll_close(), CloseState::Pending);

    // Release the barrier to let cleanup finish
    barrier.wait().await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    // After cleanup has finished and admission released, poll_close returns Ready
    assert_eq!(coordinator.poll_close(), CloseState::Ready);
}
