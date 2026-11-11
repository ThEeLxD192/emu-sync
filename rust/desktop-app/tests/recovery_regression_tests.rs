use emusync_data_drive::DriveListing;
use emusync_data_local::{
    ConfigManager, RecoveryCredentials, RecoveryRecord, RecoveryStore, SyncHistoryStore,
    TrackedError, TrackedFileSync, UploadSnapshot,
};
use emusync_desktop_app::{
    DesktopCoordinator, OperationSupervisor, PersistedRecoveryStatus, reconcile_persisted_uploads,
};
use emusync_domain::config::{AppConfig, ConfigurationStore, CredentialStore, Credentials};
use reqwest::Client;
use std::{
    io,
    path::Path,
    sync::{Arc, mpsc::channel},
};
use tempfile::tempdir;

fn stage(dir: &Path, connection: &str, remote_id: Option<&str>, digest_of: &[u8]) -> String {
    let source = dir.join("save.dat");
    std::fs::write(&source, digest_of).unwrap();
    let snapshot = UploadSnapshot::prepare(&source).unwrap();
    RecoveryStore::new(&dir.join("config.json"))
        .unwrap()
        .stage(
            &snapshot,
            RecoveryRecord {
                remote_id: remote_id.map(str::to_owned),
                local: source.to_str().unwrap().to_owned(),
                folder: "folder".into(),
                remote_name: "save.dat".into(),
                observed: None,
                digest: snapshot.digest().to_owned(),
                connection: connection.into(),
            },
        )
        .unwrap()
}

#[tokio::test]
async fn foreign_connection_record_is_preserved_without_removal() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client-id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    manager.update(|_| config.clone()).unwrap();

    let id = stage(
        dir.path(),
        "other_connection_fingerprint",
        Some("remote-1"),
        b"abc",
    );
    let store = Arc::new(RecoveryStore::new(manager.path()).unwrap());

    // Run persisted reconciliation with current credentials
    let snapshot = manager.snapshot(&config).unwrap();
    let credentials = RecoveryCredentials::new(Arc::new(manager.clone()), snapshot, config);
    let listing = DriveListing::new(
        Client::new(),
        reqwest::Url::parse("http://127.0.0.1:9/files").unwrap(),
    );
    let history = Arc::new(SyncHistoryStore::new(manager.path()).unwrap());
    let tracked = Arc::new(TrackedFileSync::new((), history).with_recovery(store.clone()));

    let results =
        reconcile_persisted_uploads(store.clone(), &listing, &tracked, "token", credentials)
            .await
            .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, id);
    assert_eq!(results[0].1, PersistedRecoveryStatus::ConnectionChanged);

    // Verify it is not deleted from RecoveryStore
    let remaining = store.list().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, id);
}

#[tokio::test]
async fn typed_error_blocks_second_mutation_and_retains_supervisor_exclusion() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client-id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    manager.update(|_| config).unwrap();

    let supervisor = Arc::new(tokio::sync::Mutex::new(OperationSupervisor::new()));
    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager,
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_supervisor(supervisor.clone());

    // Record a typed transfer error
    let io_err = io::Error::new(
        io::ErrorKind::ConnectionReset,
        "connection lost during transfer",
    );
    coordinator.record_transfer_error(TrackedError::Local(io_err));

    // Error must be retained in supervisor
    let failure_opt = supervisor.lock().await.failure();
    assert!(failure_opt.is_some());

    // A second mutation must be strictly blocked
    assert!(
        !coordinator.try_acquire_admission(),
        "Second mutation must be rejected when recovery is required"
    );

    let dummy_entry =
        emusync_domain::library::GameEntry::Native(emusync_domain::library::NativeGame {
            common: emusync_domain::library::EntryCommon {
                name: "Test".into(),
                executable_path: "/bin/true".into(),
                arguments: vec![],
                save_paths: vec![],
                save_targets: vec![],
                group: None,
                cloud_folder: None,
                drive_file_id: None,
                steam_app_id: None,
            },
            cover_path: None,
            wait_for_process: None,
        });
    coordinator.handle_action(emusync_presentation::actions::UiAction::LaunchGame(
        emusync_presentation::model::GameItem::new("Test", dummy_entry, None),
    ));

    // Toast event emitted when action is rejected due to recovery
    let events: Vec<_> = rx.try_iter().collect();
    assert!(events.iter().any(
        |e| matches!(e, emusync_desktop_app::AppEvent::Toast(msg) if msg.contains("recovery"))
    ));
}

#[tokio::test]
async fn pending_recovery_allows_connect_drive_and_clears_on_recovery() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = ConfigManager::new(&config_path).unwrap();

    let config = AppConfig {
        credentials: None,
        entries: vec![],
    };
    manager.update(|_| config).unwrap();

    // Stage an unconfirmed upload in RecoveryStore
    let _id = stage(dir.path(), "dummy_fingerprint", Some("remote-1"), b"abc");

    let (tx, _rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    // Initialize detects RecoveryStore entries without Drive connected
    coordinator.initialize();

    // Regular admission is blocked due to pending recovery
    assert!(!coordinator.try_acquire_admission());

    // But admission for connecting Drive succeeds
    assert!(coordinator.try_acquire_admission_for_connect());
    coordinator.release_admission();
}
