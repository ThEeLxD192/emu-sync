use emusync_data_local::{
    ConfigManager, RecoveryCredentials, RecoveryRecord, RecoveryStore, SyncHistoryStore,
    TrackedError, TrackedFileSync, UploadRequest, UploadSnapshot, file_sha256,
};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, CredentialSnapshot, CredentialStore, Credentials},
    sync::{CloudSaveTransfer, RemoteState},
};
use std::{
    fs, io,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

type UploadHandler =
    Box<dyn Fn(&str, &str, &str, &str) -> io::Result<RemoteState> + Send + Sync + 'static>;
type ReplaceHandler =
    Box<dyn Fn(&str, &RemoteState, &str, &str) -> io::Result<RemoteState> + Send + Sync + 'static>;

struct MockTransfer {
    upload_calls: Arc<AtomicUsize>,
    replace_calls: Arc<AtomicUsize>,
    upload_handler: Arc<Mutex<Option<UploadHandler>>>,
    replace_handler: Arc<Mutex<Option<ReplaceHandler>>>,
}

impl MockTransfer {
    fn new() -> Self {
        Self {
            upload_calls: Arc::new(AtomicUsize::new(0)),
            replace_calls: Arc::new(AtomicUsize::new(0)),
            upload_handler: Arc::new(Mutex::new(None)),
            replace_handler: Arc::new(Mutex::new(None)),
        }
    }

    fn on_upload(
        &self,
        handler: impl Fn(&str, &str, &str, &str) -> io::Result<RemoteState> + Send + Sync + 'static,
    ) {
        *self.upload_handler.lock().unwrap() = Some(Box::new(handler));
    }

    fn on_replace(
        &self,
        handler: impl Fn(&str, &RemoteState, &str, &str) -> io::Result<RemoteState>
        + Send
        + Sync
        + 'static,
    ) {
        *self.replace_handler.lock().unwrap() = Some(Box::new(handler));
    }
}

impl CloudSaveTransfer for MockTransfer {
    type Error = io::Error;

    async fn download(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: Option<i128>,
        _: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn upload(
        &self,
        token: &str,
        source: &str,
        name: &str,
        folder: &str,
    ) -> Result<RemoteState, Self::Error> {
        self.upload_calls.fetch_add(1, Ordering::SeqCst);
        let handler = self.upload_handler.lock().unwrap();
        if let Some(h) = handler.as_ref() {
            h(token, source, name, folder)
        } else {
            Ok(RemoteState {
                id: "mock-upload-id".into(),
                modified: 1_700_000_000_000_000_000,
                sha256: Some(file_sha256(Path::new(source))?),
                version: Some("1".into()),
            })
        }
    }

    async fn replace(
        &self,
        token: &str,
        remote: &RemoteState,
        source: &str,
        name: &str,
    ) -> Result<RemoteState, Self::Error> {
        self.replace_calls.fetch_add(1, Ordering::SeqCst);
        let handler = self.replace_handler.lock().unwrap();
        if let Some(h) = handler.as_ref() {
            h(token, remote, source, name)
        } else {
            Ok(RemoteState {
                id: remote.id.clone(),
                modified: 1_700_000_000_000_000_000,
                sha256: Some(file_sha256(Path::new(source))?),
                version: Some("2".into()),
            })
        }
    }
}

fn create_store_with_creds(path: &Path) -> Arc<ConfigManager> {
    fs::write(
        path,
        r#"{"googleDrive":{"clientId":"id","clientSecret":"secret","refreshToken":"refresh"}}"#,
    )
    .unwrap();
    Arc::new(ConfigManager::new(path).unwrap())
}

#[test]
fn upload_guarded_rejects_stale_credentials_before_staging_or_network_transfer() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let store = create_store_with_creds(&config_path);
        let config = store.load().unwrap();
        let expected = store.snapshot(&config).unwrap();

        // Stale guard: captured before rotating/clearing credentials
        let stale_guard = RecoveryCredentials::new(store.clone(), expected, config);

        // Rotate/clear credentials
        store.update_credentials(|_| None).unwrap();

        let save = dir.path().join("save.dat");
        fs::write(&save, b"save content").unwrap();
        let save_str = save.to_str().unwrap();

        let recovery = Arc::new(RecoveryStore::new(&config_path).unwrap());
        let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
        let transfer = MockTransfer::new();
        let upload_calls = transfer.upload_calls.clone();

        let sync = TrackedFileSync::new(transfer, history.clone()).with_recovery(recovery.clone());

        let result = sync
            .upload_guarded("token", save_str, None, "folder", "save.dat", stale_guard)
            .await;

        let Err(TrackedError::Local(err)) = result else {
            panic!("Expected TrackedError::Local");
        };
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(upload_calls.load(Ordering::SeqCst), 0);
        assert!(recovery.list().unwrap().is_empty());

        let key = history.key(&save, "folder", "save.dat").unwrap();
        assert!(history.get(&key).unwrap().is_none());
    });
}

#[test]
fn upload_guarded_blocks_with_resource_busy_when_unresolved_recovery_entry_exists() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let store = create_store_with_creds(&config_path);
        let config = store.load().unwrap();
        let expected = store.snapshot(&config).unwrap();
        let guard = RecoveryCredentials::new(store.clone(), expected, config);

        let save = dir.path().join("save.dat");
        fs::write(&save, b"v1").unwrap();
        let save_str = save.to_str().unwrap();

        let recovery = Arc::new(RecoveryStore::new(&config_path).unwrap());
        let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());

        // Stage an unresolved recovery entry manually
        let snapshot = UploadSnapshot::prepare(&save).unwrap();
        let record = RecoveryRecord {
            remote_id: None,
            local: save_str.into(),
            folder: "folder".into(),
            remote_name: "save.dat".into(),
            observed: None,
            digest: snapshot.digest().to_owned(),
            connection: guard.fingerprint(),
        };
        recovery.stage(&snapshot, record).unwrap();
        assert_eq!(recovery.list().unwrap().len(), 1);

        let transfer = MockTransfer::new();
        let upload_calls = transfer.upload_calls.clone();

        let sync = TrackedFileSync::new(transfer, history).with_recovery(recovery.clone());

        let result = sync
            .upload_guarded("token", save_str, None, "folder", "save.dat", guard)
            .await;

        let Err(TrackedError::Local(err)) = result else {
            panic!("Expected TrackedError::Local, got: {result:?}");
        };
        assert_eq!(err.kind(), io::ErrorKind::ResourceBusy);
        assert_eq!(upload_calls.load(Ordering::SeqCst), 0);
        assert_eq!(recovery.list().unwrap().len(), 1);
    });
}

#[test]
fn upload_guarded_fails_closed_when_recovery_store_staging_fails_before_network() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let store = create_store_with_creds(&config_path);
        let config = store.load().unwrap();
        let expected = store.snapshot(&config).unwrap();
        let guard = RecoveryCredentials::new(store.clone(), expected, config);

        let save = dir.path().join("save.dat");
        fs::write(&save, b"save data").unwrap();
        let save_str = save.to_str().unwrap();

        // Sabotage recovery path by creating a regular file
        fs::write(
            config_path.with_file_name("config.json.sync-recovery"),
            b"regular file blocking directory creation",
        )
        .unwrap();

        let recovery = Arc::new(RecoveryStore::new(&config_path).unwrap());
        let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
        let transfer = MockTransfer::new();
        let upload_calls = transfer.upload_calls.clone();
        let replace_calls = transfer.replace_calls.clone();

        let sync = TrackedFileSync::new(transfer, history.clone()).with_recovery(recovery);

        let result = sync
            .upload_guarded("token", save_str, None, "folder", "save.dat", guard)
            .await;

        assert!(matches!(result, Err(TrackedError::Local(_))));
        assert_eq!(upload_calls.load(Ordering::SeqCst), 0);
        assert_eq!(replace_calls.load(Ordering::SeqCst), 0);

        let key = history.key(&save, "folder", "save.dat").unwrap();
        assert!(history.get(&key).unwrap().is_none());
    });
}

#[test]
fn upload_guarded_classified_retains_recovery_on_uncertain_error_and_removes_it_on_definitive_failure()
 {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let store = create_store_with_creds(&config_path);
        let config = store.load().unwrap();
        let expected = store.snapshot(&config).unwrap();
        let guard = RecoveryCredentials::new(store.clone(), expected, config);

        let save = dir.path().join("save.dat");
        fs::write(&save, b"save for classified").unwrap();
        let save_str = save.to_str().unwrap();

        let recovery = Arc::new(RecoveryStore::new(&config_path).unwrap());
        let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
        let transfer = MockTransfer::new();

        // Step 1: Transfer fails with "412 Precondition Failed" (definitive)
        transfer.on_upload(|_, _, _, _| Err(io::Error::other("412 Precondition Failed")));
        let sync = TrackedFileSync::new(transfer, history).with_recovery(recovery.clone());

        let request = UploadRequest {
            token: "token",
            local: save_str,
            remote: None,
            folder: "folder",
            remote_name: "save.dat",
        };
        let predicate = |e: &io::Error| e.to_string().contains("412");

        let err1 = sync
            .upload_guarded_classified(request, guard.clone(), predicate)
            .await
            .unwrap_err();

        let TrackedError::Upload {
            snapshot, context, ..
        } = err1
        else {
            panic!("Expected TrackedError::Upload");
        };
        assert!(
            context.recovery_id.is_none(),
            "Recovery entry must be cleared on definitive failure"
        );
        assert!(
            snapshot.path().exists(),
            "Snapshot must remain alive while error lives"
        );
        assert!(recovery.list().unwrap().is_empty());
        drop(snapshot);

        // Step 2: Transfer fails with uncertain error ("socket closed after body")
        let transfer2 = MockTransfer::new();
        transfer2.on_upload(|_, _, _, _| Err(io::Error::other("socket closed after body")));
        let sync2 = TrackedFileSync::new(
            transfer2,
            Arc::new(SyncHistoryStore::new(&config_path).unwrap()),
        )
        .with_recovery(recovery.clone());

        let err2 = sync2
            .upload_guarded_classified(request, guard.clone(), predicate)
            .await
            .unwrap_err();

        let TrackedError::Upload {
            snapshot: snapshot2,
            context: context2,
            ..
        } = err2
        else {
            panic!("Expected TrackedError::Upload");
        };
        assert!(
            context2.recovery_id.is_some(),
            "Recovery entry must be retained on uncertain failure"
        );
        assert!(
            recovery
                .has_unresolved(&save, "folder", "save.dat")
                .unwrap(),
            "has_unresolved must be true"
        );
        drop(snapshot2);

        // Step 3: A third upload attempt must block with ResourceBusy
        let transfer3 = MockTransfer::new();
        let sync3 = TrackedFileSync::new(
            transfer3,
            Arc::new(SyncHistoryStore::new(&config_path).unwrap()),
        )
        .with_recovery(recovery.clone());
        let result3 = sync3
            .upload_guarded("token", save_str, None, "folder", "save.dat", guard)
            .await;
        let Err(TrackedError::Local(err)) = result3 else {
            panic!("Expected TrackedError::Local(ResourceBusy)");
        };
        assert_eq!(err.kind(), io::ErrorKind::ResourceBusy);
    });
}

#[test]
fn upload_guarded_returns_after_upload_and_keeps_recovery_when_credentials_rotate_or_remote_hash_mismatches()
 {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        // Scenario A: Credential rotation mid-flight
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let store = create_store_with_creds(&config_path);
        let config = store.load().unwrap();
        let expected = store.snapshot(&config).unwrap();
        let guard = RecoveryCredentials::new(store.clone(), expected, config);

        let save = dir.path().join("save.dat");
        fs::write(&save, b"save scenario A").unwrap();
        let save_str = save.to_str().unwrap();
        let digest = file_sha256(&save).unwrap();

        let recovery = Arc::new(RecoveryStore::new(&config_path).unwrap());
        let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
        let transfer = MockTransfer::new();

        let store_for_rotation = store.clone();
        let digest_clone = digest.clone();
        transfer.on_replace(move |_, remote, _, _| {
            // Rotate credentials before returning Ok(remote)
            store_for_rotation.update_credentials(|_| None).unwrap();
            Ok(RemoteState {
                id: remote.id.clone(),
                modified: 1_700_000_000_000_000_000,
                sha256: Some(digest_clone.clone()),
                version: Some("2".into()),
            })
        });

        let observed = RemoteState {
            id: "observed-id".into(),
            modified: 1_600_000_000_000_000_000,
            sha256: Some(digest),
            version: Some("1".into()),
        };

        let sync = TrackedFileSync::new(transfer, history.clone()).with_recovery(recovery.clone());
        let result_a = sync
            .upload_guarded(
                "token",
                save_str,
                Some(&observed),
                "folder",
                "save.dat",
                guard,
            )
            .await;

        let Err(TrackedError::AfterUpload {
            source, context, ..
        }) = result_a
        else {
            panic!("Expected TrackedError::AfterUpload for Scenario A");
        };
        assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
        assert!(context.recovery_id.is_some());
        assert!(
            recovery
                .has_unresolved(&save, "folder", "save.dat")
                .unwrap()
        );
        let key = history.key(&save, "folder", "save.dat").unwrap();
        assert!(history.get(&key).unwrap().is_none());

        // Scenario B: Remote hash divergence
        let dir_b = tempfile::tempdir().unwrap();
        let config_path_b = dir_b.path().join("config.json");
        let store_b = create_store_with_creds(&config_path_b);
        let config_b = store_b.load().unwrap();
        let expected_b = store_b.snapshot(&config_b).unwrap();
        let guard_b = RecoveryCredentials::new(store_b.clone(), expected_b, config_b);

        let save_b = dir_b.path().join("save.dat");
        fs::write(&save_b, b"save scenario B").unwrap();
        let save_str_b = save_b.to_str().unwrap();

        let recovery_b = Arc::new(RecoveryStore::new(&config_path_b).unwrap());
        let history_b = Arc::new(SyncHistoryStore::new(&config_path_b).unwrap());
        let transfer_b = MockTransfer::new();

        transfer_b.on_upload(|_, _, _, _| {
            Ok(RemoteState {
                id: "rem-divergent".into(),
                modified: 1_700_000_000_000_000_000,
                sha256: Some("0".repeat(64)), // Mismatched SHA-256
                version: Some("1".into()),
            })
        });

        let sync_b =
            TrackedFileSync::new(transfer_b, history_b.clone()).with_recovery(recovery_b.clone());
        let result_b = sync_b
            .upload_guarded("token", save_str_b, None, "folder", "save.dat", guard_b)
            .await;

        let Err(TrackedError::AfterUpload {
            source: source_b,
            context: context_b,
            ..
        }) = result_b
        else {
            panic!("Expected TrackedError::AfterUpload for Scenario B");
        };
        assert!(
            source_b
                .to_string()
                .contains("reconciled upload content could not be verified"),
            "Expected 'reconciled upload content could not be verified', got: {source_b}"
        );
        assert!(context_b.recovery_id.is_some());
        let key_b = history_b.key(&save_b, "folder", "save.dat").unwrap();
        assert!(history_b.get(&key_b).unwrap().is_none());
    });
}

#[test]
fn upload_guarded_success_records_checkpoint_updates_mtime_and_clears_recovery_entry() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let store = create_store_with_creds(&config_path);
        let config = store.load().unwrap();
        let expected = store.snapshot(&config).unwrap();
        let guard = RecoveryCredentials::new(store.clone(), expected, config);

        let save = dir.path().join("save.dat");
        fs::write(&save, b"save for success").unwrap();
        let save_str = save.to_str().unwrap();
        let digest = file_sha256(&save).unwrap();

        // Set old mtime: 1_000_000_000 seconds
        filetime::set_file_mtime(&save, filetime::FileTime::from_unix_time(1_000_000_000, 0))
            .unwrap();

        let recovery = Arc::new(RecoveryStore::new(&config_path).unwrap());
        let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
        let transfer = MockTransfer::new();

        let expected_observed_id = "observed-old-id";
        let target_modified_nanos: i128 = 1_780_000_123_000_000_000;
        let digest_for_replace = digest.clone();

        transfer.on_replace(move |_, remote, _, _| {
            assert_eq!(remote.id, expected_observed_id);
            Ok(RemoteState {
                id: "new-remote-id".into(),
                modified: target_modified_nanos,
                sha256: Some(digest_for_replace.clone()),
                version: Some("2".into()),
            })
        });

        let observed = RemoteState {
            id: expected_observed_id.into(),
            modified: 1_000_000_000_000_000_000,
            sha256: Some(digest),
            version: Some("1".into()),
        };

        let sync = TrackedFileSync::new(transfer, history.clone()).with_recovery(recovery.clone());

        sync.upload_guarded(
            "token",
            save_str,
            Some(&observed),
            "folder",
            "save.dat",
            guard,
        )
        .await
        .unwrap();

        // 1. Recovery store is empty
        assert!(recovery.list().unwrap().is_empty());

        // 2. History checkpoint is recorded with new remote_id
        let key = history.key(&save, "folder", "save.dat").unwrap();
        let cp = history.get(&key).unwrap().unwrap();
        assert_eq!(cp.remote_id, "new-remote-id");

        // 3. mtime on disk updated to 1_780_000_123 seconds
        let disk_mtime =
            filetime::FileTime::from_last_modification_time(&fs::metadata(&save).unwrap());
        assert_eq!(disk_mtime.unix_seconds(), 1_780_000_123);
    });
}

#[test]
fn recovery_credentials_fingerprint_is_length_prefixed_and_collision_resistant() {
    let fallback = AppConfig::default();

    let cred1 = RecoveryCredentials::new(
        Arc::new(()),
        CredentialSnapshot {
            revision: 1,
            credentials: Some(Credentials {
                client_id: "ab".into(),
                client_secret: "c".into(),
                refresh_token: Some("d".into()),
            }),
        },
        fallback.clone(),
    );

    let cred2 = RecoveryCredentials::new(
        Arc::new(()),
        CredentialSnapshot {
            revision: 1,
            credentials: Some(Credentials {
                client_id: "a".into(),
                client_secret: "bc".into(),
                refresh_token: Some("d".into()),
            }),
        },
        fallback.clone(),
    );

    let cred3 = RecoveryCredentials::new(
        Arc::new(()),
        CredentialSnapshot {
            revision: 1,
            credentials: None,
        },
        fallback,
    );

    let fp1 = cred1.fingerprint();
    let fp2 = cred2.fingerprint();
    let fp3 = cred3.fingerprint();

    assert_eq!(fp1.len(), 64);
    assert_eq!(fp2.len(), 64);
    assert_eq!(fp3.len(), 64);

    assert_ne!(
        fp1, fp2,
        "Length prefixing must prevent shifting boundaries"
    );
    assert_ne!(fp1, fp3);
    assert_ne!(fp2, fp3);
}
