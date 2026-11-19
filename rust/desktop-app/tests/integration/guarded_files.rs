use emusync_data_local::{
    ConfigManager, RecoveryCredentials, RecoveryStore, SyncHistoryStore, TrackedError,
    TrackedFileSync,
};
use emusync_desktop_app::{GuardedFiles, RetainedFiles, RetainedFilesError};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, CredentialStore, Credentials},
    sync::{CloudSaveTransfer, LocalSaveFiles, RemoteState, Timestamp},
};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct UncertainTransfer(Arc<AtomicUsize>);
impl CloudSaveTransfer for UncertainTransfer {
    type Error = io::Error;
    async fn download(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: Option<Timestamp>,
        _: &str,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }
    async fn upload(&self, _: &str, _: &str, _: &str, _: &str) -> Result<RemoteState, Self::Error> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::other("uncertain write"))
    }
    async fn replace(
        &self,
        _: &str,
        _: &RemoteState,
        _: &str,
        _: &str,
    ) -> Result<RemoteState, Self::Error> {
        unreachable!()
    }
}

#[tokio::test]
async fn guarded_upload_retains_uncertain_snapshot_and_persistent_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let store = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    store.update(|_| config.clone()).unwrap();
    let guard = RecoveryCredentials::new(store.clone(), store.snapshot(&config).unwrap(), config);
    let recovery = Arc::new(RecoveryStore::new(&config_path).unwrap());
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let tracked = Arc::new(
        TrackedFileSync::new(UncertainTransfer(calls.clone()), history)
            .with_recovery(recovery.clone()),
    );
    let files = RetainedFiles::new(GuardedFiles::new(tracked, guard, |_| false));
    let path = dir.path().join("save.dat");
    std::fs::write(&path, b"original save").unwrap();
    assert_eq!(
        files
            .upload("access", path.to_str().unwrap(), None, "folder", "save.dat")
            .await,
        Err(RetainedFilesError::TransferFailed)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(recovery.list().unwrap().len(), 1);
    let failure = match files.finish() {
        Ok(Some(TrackedError::Upload {
            snapshot, context, ..
        })) => {
            assert!(context.recovery_id.is_some());
            snapshot
        }
        _ => panic!("missing retained upload"),
    };
    assert_eq!(std::fs::read(failure.path()).unwrap(), b"original save");
}

#[tokio::test]
async fn changed_connection_prevents_upload_before_remote_write() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let store = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("old-token".into()),
        }),
        entries: vec![],
    };
    store.update(|_| config.clone()).unwrap();
    let guard = RecoveryCredentials::new(store.clone(), store.snapshot(&config).unwrap(), config);
    store
        .update_credentials(|_| {
            Some(Credentials {
                client_id: "id".into(),
                client_secret: "secret".into(),
                refresh_token: Some("new-token".into()),
            })
        })
        .unwrap();
    let recovery = Arc::new(RecoveryStore::new(&config_path).unwrap());
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let tracked = Arc::new(
        TrackedFileSync::new(UncertainTransfer(calls.clone()), history)
            .with_recovery(recovery.clone()),
    );
    let files = RetainedFiles::new(GuardedFiles::new(tracked, guard, |_| false));
    let path = dir.path().join("save.dat");
    std::fs::write(&path, b"original save").unwrap();
    assert_eq!(
        files
            .upload("access", path.to_str().unwrap(), None, "folder", "save.dat")
            .await,
        Err(RetainedFilesError::TransferFailed)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(recovery.list().unwrap().is_empty());
    assert!(matches!(files.finish(), Ok(Some(TrackedError::Local(_)))));
}

struct PausedDownload {
    ready: Arc<tokio::sync::Notify>,
    resume: Arc<tokio::sync::Notify>,
}
impl CloudSaveTransfer for PausedDownload {
    type Error = io::Error;
    async fn download(
        &self,
        _: &str,
        _: &str,
        destination: &str,
        _: Option<Timestamp>,
        _: &str,
    ) -> Result<(), Self::Error> {
        std::fs::write(destination, b"remote save")?;
        self.ready.notify_one();
        self.resume.notified().await;
        Ok(())
    }
    async fn upload(&self, _: &str, _: &str, _: &str, _: &str) -> Result<RemoteState, Self::Error> {
        unreachable!()
    }
    async fn replace(
        &self,
        _: &str,
        _: &RemoteState,
        _: &str,
        _: &str,
    ) -> Result<RemoteState, Self::Error> {
        unreachable!()
    }
}

#[tokio::test]
async fn changed_connection_during_download_preserves_save_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let store = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    store.update(|_| config.clone()).unwrap();
    let guard = RecoveryCredentials::new(store.clone(), store.snapshot(&config).unwrap(), config);
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let ready = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    let transfer = PausedDownload {
        ready: ready.clone(),
        resume: resume.clone(),
    };
    let tracked = Arc::new(TrackedFileSync::new(transfer, history.clone()));
    let files = RetainedFiles::new(GuardedFiles::new(tracked, guard, |_| false));
    let path = dir.path().join("save.dat");
    std::fs::write(&path, b"local save").unwrap();
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: None,
        version: Some("1".into()),
    };
    let task_files = files.clone();
    let task_path = path.clone();
    let task = tokio::spawn(async move {
        task_files
            .download(
                "access",
                &remote,
                task_path.to_str().unwrap(),
                "folder",
                "save.dat",
            )
            .await
    });
    ready.notified().await;
    store.update_credentials(|_| None).unwrap();
    resume.notify_one();
    assert_eq!(task.await.unwrap(), Err(RetainedFilesError::TransferFailed));
    assert_eq!(std::fs::read(&path).unwrap(), b"local save");
    let key = history.key(&path, "folder", "save.dat").unwrap();
    assert!(history.get(&key).unwrap().is_none());
    assert!(
        matches!(files.finish(), Ok(Some(TrackedError::Local(error))) if error.kind() == io::ErrorKind::PermissionDenied)
    );
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".restore")
    }));
}

#[tokio::test]
async fn changed_connection_cannot_record_in_sync_history() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let store = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    store.update(|_| config.clone()).unwrap();
    let guard = RecoveryCredentials::new(store.clone(), store.snapshot(&config).unwrap(), config);
    store.update_credentials(|_| None).unwrap();
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let transfer = UncertainTransfer(Arc::new(AtomicUsize::new(0)));
    let tracked = Arc::new(TrackedFileSync::new(transfer, history.clone()));
    let files = RetainedFiles::new(GuardedFiles::new(tracked, guard, |_| false));
    let path = dir.path().join("save.dat");
    std::fs::write(&path, b"local save").unwrap();
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: None,
        version: Some("1".into()),
    };
    assert_eq!(
        files
            .remember(path.to_str().unwrap(), &remote, "folder", "save.dat")
            .await,
        Err(RetainedFilesError::TransferFailed)
    );
    let key = history.key(&path, "folder", "save.dat").unwrap();
    assert!(history.get(&key).unwrap().is_none());
    assert!(
        matches!(files.finish(), Ok(Some(TrackedError::Local(error))) if error.kind() == io::ErrorKind::PermissionDenied)
    );
}

#[tokio::test]
async fn current_connection_commits_download_and_records_history() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let store = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    store.update(|_| config.clone()).unwrap();
    let guard = RecoveryCredentials::new(store.clone(), store.snapshot(&config).unwrap(), config);
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let ready = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    let tracked = Arc::new(TrackedFileSync::new(
        PausedDownload {
            ready: ready.clone(),
            resume: resume.clone(),
        },
        history.clone(),
    ));
    let files = RetainedFiles::new(GuardedFiles::new(tracked, guard, |_| false));
    let path = dir.path().join("save.dat");
    std::fs::write(&path, b"local save").unwrap();
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: None,
        version: Some("1".into()),
    };
    let task_files = files.clone();
    let task_path = path.clone();
    let task = tokio::spawn(async move {
        task_files
            .download(
                "access",
                &remote,
                task_path.to_str().unwrap(),
                "folder",
                "save.dat",
            )
            .await
    });
    ready.notified().await;
    resume.notify_one();
    assert_eq!(task.await.unwrap(), Ok(()));
    assert_eq!(std::fs::read(&path).unwrap(), b"remote save");
    let key = history.key(&path, "folder", "save.dat").unwrap();
    assert_eq!(history.get(&key).unwrap().unwrap().remote_id, "remote");
    assert!(matches!(files.finish(), Ok(None)));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn directory_download_rejects_parent_replacement_and_preserves_both_saves() {
    use emusync_domain::sync::DirectoryDownload;
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let store = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    store.update(|_| config.clone()).unwrap();
    let guard = RecoveryCredentials::new(store.clone(), store.snapshot(&config).unwrap(), config);
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let ready = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    let transfer = PausedDownload {
        ready: ready.clone(),
        resume: resume.clone(),
    };
    let tracked = Arc::new(TrackedFileSync::new(transfer, history.clone()));
    let files = RetainedFiles::new(GuardedFiles::new(tracked, guard, |_| false));
    let root = dir.path().join("selected");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(root.join("sub/save.dat"), b"original save").unwrap();
    std::fs::write(outside.join("save.dat"), b"outside save").unwrap();
    let remote = RemoteState {
        id: "remote".into(),
        modified: 1_700_000_000_000_000_000,
        sha256: None,
        version: Some("1".into()),
    };
    let task_files = files.clone();
    let task_root = root.clone();
    let task = tokio::spawn(async move {
        let local = task_root.join("sub/save.dat");
        task_files
            .download_in_directory(DirectoryDownload {
                token: "access",
                remote: &remote,
                local: local.to_str().unwrap(),
                root: task_root.to_str().unwrap(),
                relative: "sub/save.dat",
                folder: "folder",
                remote_name: "prefix/sub/save.dat",
            })
            .await
    });
    ready.notified().await;
    std::fs::rename(root.join("sub"), root.join("moved")).unwrap();
    symlink(&outside, root.join("sub")).unwrap();
    resume.notify_one();
    assert_eq!(task.await.unwrap(), Err(RetainedFilesError::TransferFailed));
    assert_eq!(
        std::fs::read(root.join("moved/save.dat")).unwrap(),
        b"original save"
    );
    assert_eq!(
        std::fs::read(outside.join("save.dat")).unwrap(),
        b"outside save"
    );
    let key = history
        .key(&root.join("sub/save.dat"), "folder", "prefix/sub/save.dat")
        .unwrap();
    assert!(history.get(&key).unwrap().is_none());
    assert!(matches!(files.finish(), Ok(Some(TrackedError::Local(_)))));
}

struct DefinitiveTransfer;
impl CloudSaveTransfer for DefinitiveTransfer {
    type Error = io::Error;
    async fn download(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: Option<Timestamp>,
        _: &str,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }
    async fn upload(&self, _: &str, _: &str, _: &str, _: &str) -> Result<RemoteState, Self::Error> {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "definitive upload failure",
        ))
    }
    async fn replace(
        &self,
        _: &str,
        _: &RemoteState,
        _: &str,
        _: &str,
    ) -> Result<RemoteState, Self::Error> {
        unreachable!()
    }
}

#[tokio::test]
async fn definitive_failure_classifier_removes_recovery_record_while_uncertain_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let store = Arc::new(ConfigManager::new(&config_path).unwrap());
    let config = AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![],
    };
    store.update(|_| config.clone()).unwrap();
    let guard = RecoveryCredentials::new(store.clone(), store.snapshot(&config).unwrap(), config);
    let recovery = Arc::new(RecoveryStore::new(&config_path).unwrap());
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let tracked =
        Arc::new(TrackedFileSync::new(DefinitiveTransfer, history).with_recovery(recovery.clone()));
    let files = RetainedFiles::new(GuardedFiles::new(
        tracked.clone(),
        guard.clone(),
        |e: &io::Error| e.kind() == io::ErrorKind::InvalidData,
    ));
    let path = dir.path().join("save.dat");
    std::fs::write(&path, b"data").unwrap();

    let res = files
        .upload("token", path.to_str().unwrap(), None, "folder", "save.dat")
        .await;
    assert_eq!(res, Err(RetainedFilesError::TransferFailed));

    // Recovery list is empty (0 records) because the failure was definitive
    assert_eq!(recovery.list().unwrap().len(), 0);

    // A second upload with a fresh GuardedFiles is not blocked with ResourceBusy
    let files2 = RetainedFiles::new(GuardedFiles::new(tracked, guard, |e: &io::Error| {
        e.kind() == io::ErrorKind::InvalidData
    }));
    let res2 = files2
        .upload("token", path.to_str().unwrap(), None, "folder", "save.dat")
        .await;
    assert_eq!(res2, Err(RetainedFilesError::TransferFailed));
}

#[tokio::test]
async fn non_linux_directory_download_rejection_contract() {
    use emusync_domain::sync::DirectoryDownload;

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = Arc::new(ConfigManager::new(&config_path).unwrap());
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let store = Arc::new(RecoveryStore::new(&config_path).unwrap());
    let tracked = Arc::new(
        TrackedFileSync::new(UncertainTransfer(Arc::new(AtomicUsize::new(0))), history)
            .with_recovery(store),
    );
    let config = AppConfig::default();
    let snapshot = manager.snapshot(&config).unwrap();
    let credentials = RecoveryCredentials::new(manager.clone(), snapshot, config);
    let guarded = GuardedFiles::new(tracked, credentials, |_| false);

    let remote = RemoteState {
        id: "dummy_id".into(),
        modified: 1_700_000_000,
        sha256: None,
        version: None,
    };
    let local_file = dir.path().join("rel");
    let req = DirectoryDownload {
        token: "token",
        remote: &remote,
        local: local_file.to_str().unwrap(),
        root: dir.path().to_str().unwrap(),
        relative: "rel",
        folder: "folder",
        remote_name: "remote_name",
    };

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        use emusync_domain::sync::LocalSaveFiles;
        let res = guarded.download_in_directory(req).await;
        assert!(
            matches!(res, Err(emusync_data_local::TrackedError::Local(e)) if e.kind() == std::io::ErrorKind::Unsupported)
        );
    }
    #[cfg(any(target_os = "linux", windows))]
    {
        let _ = guarded;
        let _ = req;
    }
}
