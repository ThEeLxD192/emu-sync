use emusync_data_local::ConfigManager;
use emusync_domain::config::{
    AppConfig, ConfigurationStore, CredentialStore, CredentialTransaction, Credentials,
};
use std::{
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

#[test]
fn generic_config_writes_invalidate_credential_aba_but_library_edits_do_not() {
    use emusync_domain::library::{GameEntry, NativeGame};
    let dir = tempfile::tempdir().unwrap();
    let store = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "synthetic".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    store.save(&config).unwrap();
    let initial = store.snapshot(&config).unwrap();
    store
        .update(|mut config| {
            config
                .entries
                .push(GameEntry::Native(NativeGame::default()));
            config
        })
        .unwrap();
    assert_eq!(store.snapshot(&config).unwrap(), initial);
    store
        .update(|mut config| {
            config.credentials = None;
            config
        })
        .unwrap();
    store
        .update(|mut current| {
            current.credentials = config.credentials.clone();
            current
        })
        .unwrap();
    assert!(!store.compare_and_set(&initial, None, &config).unwrap());
    let after_update = store.snapshot(&config).unwrap();
    let mut disconnected = store.load().unwrap();
    disconnected.credentials = None;
    store.save(&disconnected).unwrap();
    disconnected.credentials = config.credentials.clone();
    store.save(&disconnected).unwrap();
    assert!(!store.compare_and_set(&after_update, None, &config).unwrap());
}
#[test]
fn connection_change_waits_for_guarded_action_and_stale_revision_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ConfigManager::new(&dir.path().join("config.json")).unwrap());
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "synthetic".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    store.save(&config).unwrap();
    let expected = store.snapshot(&config).unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker_store = store.clone();
    let worker_config = config.clone();
    let worker_expected = expected.clone();
    let worker = thread::spawn(move || {
        worker_store
            .while_current(&worker_expected, &worker_config, || {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                "recorded"
            })
            .unwrap()
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let changer = store.clone();
    let change = thread::spawn(move || {
        attempt_tx.send(()).unwrap();
        changer.update_credentials(|_| None).unwrap();
        done_tx.send(()).unwrap();
    });
    attempt_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(done_rx.try_recv().is_err());
    release_tx.send(()).unwrap();
    assert_eq!(worker.join().unwrap(), Some("recorded"));
    done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    change.join().unwrap();
    assert_eq!(
        store
            .while_current(&expected, &config, || panic!("stale action executed"))
            .unwrap(),
        None::<()>
    );
}

#[tokio::test]
async fn guarded_reconciliation_rejects_connection_aba_without_writing_history() {
    use emusync_data_local::{
        RecoveryCredentials, SyncHistoryStore, TrackedFileSync, UploadSnapshot,
    };
    use emusync_domain::sync::RemoteState;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let store = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "synthetic".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    store.save(&config).unwrap();
    let expected = store.snapshot(&config).unwrap();
    let save = dir.path().join("save");
    std::fs::write(&save, b"abc").unwrap();
    let snapshot = UploadSnapshot::prepare(&save).unwrap();
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let tracked = TrackedFileSync::new((), history.clone());
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: Some(snapshot.digest().into()),
        version: Some("2".into()),
    };
    store.update_credentials(|_| None).unwrap();
    store
        .update_credentials(|_| config.credentials.clone())
        .unwrap();
    let guard = RecoveryCredentials::new(store.clone(), expected, config.clone());
    let result = tracked
        .record_reconciled_upload_guarded(
            snapshot.evidence(),
            remote.clone(),
            save.to_str().unwrap(),
            "folder",
            "save",
            guard,
        )
        .await;
    assert_eq!(
        result.unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    let key = history.key(&save, "folder", "save").unwrap();
    assert!(history.get(&key).unwrap().is_none());
    assert_eq!(std::fs::read(&save).unwrap(), b"abc");
    let fresh = RecoveryCredentials::new(store.clone(), store.snapshot(&config).unwrap(), config);
    tracked
        .record_reconciled_upload_guarded(
            snapshot.evidence(),
            remote,
            save.to_str().unwrap(),
            "folder",
            "save",
            fresh,
        )
        .await
        .unwrap();
    assert_eq!(history.get(&key).unwrap().unwrap().remote_id, "remote");
}
