use emusync_data_local::{
    ConfigManager, RecoveryCredentials, SyncHistoryStore, TrackedError, TrackedFileSync,
    file_sha256,
};
use emusync_domain::{
    config::{ConfigurationStore, CredentialStore, Credentials},
    sync::{CloudSaveTransfer, Decision, LocalSaveFiles, RemoteState},
};
use std::{fs, io, path::Path, sync::Arc};

struct Transfer {
    remote: RemoteState,
    source_to_change: Option<std::path::PathBuf>,
}

impl CloudSaveTransfer for Transfer {
    type Error = std::io::Error;
    async fn download(
        &self,
        _: &str,
        _: &str,
        destination: &str,
        _: Option<i128>,
        _: &str,
    ) -> Result<(), Self::Error> {
        fs::write(destination, b"remote")
    }
    async fn upload(
        &self,
        _: &str,
        source: &str,
        _: &str,
        _: &str,
    ) -> Result<RemoteState, Self::Error> {
        assert_eq!(fs::read(source)?, b"local");
        if let Some(path) = &self.source_to_change {
            fs::write(path, b"newer")?;
        }
        Ok(self.remote.clone())
    }
    async fn replace(
        &self,
        token: &str,
        _: &RemoteState,
        source: &str,
        name: &str,
    ) -> Result<RemoteState, Self::Error> {
        self.upload(token, source, name, "").await
    }
}

#[test]
fn tracked_transfers_verify_content_record_history_and_keep_local_changes() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save.dat");
        fs::write(&path, b"local").unwrap();
        let hash = file_sha256(&path).unwrap();
        let remote = RemoteState {
            id: "id".into(),
            modified: 1_780_000_000_000_000_000,
            sha256: Some(hash),
            version: Some("1".into()),
        };
        let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
        let sync = TrackedFileSync::new(
            Transfer {
                remote: remote.clone(),
                source_to_change: Some(path.clone()),
            },
            history.clone(),
        );
        let name = path.to_str().unwrap();
        assert!(
            sync.upload("token", name, None, "folder", "save.dat")
                .await
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"newer");
        let key = history.key(Path::new(name), "folder", "save.dat").unwrap();
        assert!(history.get(&key).unwrap().is_some());
        assert_eq!(
            sync.plan(name, Some(&remote), "folder", "save.dat")
                .await
                .unwrap(),
            Decision::UploadLocal
        );
        assert!(
            sync.download("token", &remote, name, "folder", "save.dat")
                .await
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"newer");
        let remote_file = dir.path().join("remote");
        fs::write(&remote_file, b"remote").unwrap();
        let remote = RemoteState {
            sha256: Some(file_sha256(&remote_file).unwrap()),
            ..remote
        };
        sync.download("token", &remote, name, "folder", "save.dat")
            .await
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"remote");
        assert_eq!(
            sync.plan(name, Some(&remote), "folder", "save.dat")
                .await
                .unwrap(),
            Decision::InSync
        );
    });
}

struct NoopTransfer;
impl CloudSaveTransfer for NoopTransfer {
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

#[test]
fn remember_and_remember_guarded_enforce_in_sync_and_credential_revision() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        fs::write(
            &config_path,
            r#"{"googleDrive":{"clientId":"id","clientSecret":"secret"}}"#,
        )
        .unwrap();
        let store = Arc::new(ConfigManager::new(&config_path).unwrap());
        let config = store.load().unwrap();
        let guard = RecoveryCredentials::new(
            store.clone(),
            store.snapshot(&config).unwrap(),
            config.clone(),
        );

        let save = dir.path().join("save.dat");
        fs::write(&save, b"synced").unwrap();
        let save_str = save.to_str().unwrap();
        let digest = file_sha256(&save).unwrap();

        let remote = RemoteState {
            id: "rem-1".into(),
            modified: 1_700_000_000_000_000_000,
            sha256: Some(digest),
            version: Some("1".into()),
        };

        let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
        let sync = TrackedFileSync::new(NoopTransfer, history.clone());
        let key = history.key(&save, "folder", "save.dat").unwrap();

        // 1. Save modified on disk before remember -> must fail with "save changed during synchronization"
        fs::write(&save, b"modified").unwrap();
        let err1 = sync
            .remember(save_str, &remote, "folder", "save.dat")
            .await
            .unwrap_err();
        assert!(
            err1.to_string()
                .contains("save changed during synchronization")
        );

        let err2 = sync
            .remember_guarded(save_str, &remote, "folder", "save.dat", guard.clone())
            .await
            .unwrap_err();
        assert!(
            err2.to_string()
                .contains("save changed during synchronization")
        );
        assert!(history.get(&key).unwrap().is_none());

        // 2. Save restored to synced, but credentials rotated/cleared
        fs::write(&save, b"synced").unwrap();
        store.update_credentials(|_| None).unwrap();
        let err3 = sync
            .remember_guarded(save_str, &remote, "folder", "save.dat", guard)
            .await
            .unwrap_err();
        assert_eq!(err3.kind(), io::ErrorKind::PermissionDenied);
        assert!(history.get(&key).unwrap().is_none());

        // 3. Fresh valid guard succeeds and records checkpoint
        store
            .update_credentials(|_| {
                Some(Credentials {
                    client_id: "new-client".into(),
                    client_secret: "new-secret".into(),
                    refresh_token: None,
                })
            })
            .unwrap();
        let valid_config = store.load().unwrap();
        let valid_guard = RecoveryCredentials::new(
            store.clone(),
            store.snapshot(&valid_config).unwrap(),
            valid_config,
        );
        sync.remember_guarded(save_str, &remote, "folder", "save.dat", valid_guard)
            .await
            .unwrap();
        assert!(history.get(&key).unwrap().is_some());
    });
}

struct MidFlightRotatingTransfer {
    store: Arc<ConfigManager>,
    content: Vec<u8>,
}

impl CloudSaveTransfer for MidFlightRotatingTransfer {
    type Error = io::Error;
    async fn download(
        &self,
        _: &str,
        _: &str,
        destination: &str,
        _: Option<i128>,
        _: &str,
    ) -> Result<(), Self::Error> {
        fs::write(destination, &self.content)?;
        // Rotate credentials while download is in flight
        self.store.update_credentials(|_| None)?;
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

#[test]
fn download_guarded_aborts_on_mid_flight_credential_change_preserving_local_save_and_skipping_backup()
 {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        fs::write(
            &config_path,
            r#"{"googleDrive":{"clientId":"id","clientSecret":"secret"}}"#,
        )
        .unwrap();
        let store = Arc::new(ConfigManager::new(&config_path).unwrap());
        let config = store.load().unwrap();
        let guard = RecoveryCredentials::new(
            store.clone(),
            store.snapshot(&config).unwrap(),
            config.clone(),
        );

        let save = dir.path().join("save.dat");
        fs::write(&save, b"original local").unwrap();
        let save_str = save.to_str().unwrap();

        let remote_content = b"new remote content";
        let remote = RemoteState {
            id: "rem-download".into(),
            modified: 1_700_000_000_000_000_000,
            sha256: Some(sha2_digest(remote_content)),
            version: Some("1".into()),
        };

        let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
        let key = history.key(&save, "folder", "save.dat").unwrap();

        let transfer = MidFlightRotatingTransfer {
            store: store.clone(),
            content: remote_content.to_vec(),
        };

        let sync = TrackedFileSync::new(transfer, history.clone());

        let result = sync
            .download_guarded("token", &remote, save_str, "folder", "save.dat", guard)
            .await;

        let Err(TrackedError::Local(err)) = result else {
            panic!("Expected TrackedError::Local, got: {result:?}");
        };
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

        // 2. Local save preserves original bytes
        assert_eq!(fs::read(&save).unwrap(), b"original local");

        // 3. No backup directory and no history checkpoint
        assert!(!history.backup_directory().exists());
        assert!(history.get(&key).unwrap().is_none());

        // 4. No orphan .restore temporary files
        let orphan_restore_files = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("restore"))
            .count();
        assert_eq!(orphan_restore_files, 0);
    });
}

// Used only by the Linux pinned-download test.
#[cfg(target_os = "linux")]
struct PinnedDownloadTransfer {
    content_map: std::collections::HashMap<String, Vec<u8>>,
}

#[cfg(target_os = "linux")]
impl CloudSaveTransfer for PinnedDownloadTransfer {
    type Error = io::Error;
    async fn download(
        &self,
        _: &str,
        id: &str,
        destination: &str,
        _: Option<i128>,
        _: &str,
    ) -> Result<(), Self::Error> {
        let content = self
            .content_map
            .get(id)
            .ok_or_else(|| io::Error::other("unexpected remote id"))?;
        fs::write(destination, content)
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

#[cfg(target_os = "linux")]
#[test]
fn download_pinned_and_directory_guarded_enforce_root_match_credentials_and_symlink_safety() {
    use emusync_domain::sync::DirectoryDownload;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        fs::write(
            &config_path,
            r#"{"googleDrive":{"clientId":"id","clientSecret":"secret"}}"#,
        )
        .unwrap();
        let store = Arc::new(ConfigManager::new(&config_path).unwrap());
        let config = store.load().unwrap();
        let guard = RecoveryCredentials::new(
            store.clone(),
            store.snapshot(&config).unwrap(),
            config.clone(),
        );

        let root = dir.path().join("root");
        fs::create_dir_all(root.join("slot")).unwrap();
        let slot_save = root.join("slot/save.dat");
        fs::write(&slot_save, b"v1").unwrap();
        let v1_hash = file_sha256(&slot_save).unwrap();

        let remote_v2_content = b"v2 content";
        let remote_v2 = RemoteState {
            id: "rem-v2".into(),
            modified: 1_700_000_000_000_000_000,
            sha256: Some(sha2_digest(remote_v2_content)),
            version: Some("2".into()),
        };

        let remote_v3_content = b"v3 content";
        let remote_v3 = RemoteState {
            id: "rem-v3".into(),
            modified: 1_710_000_000_000_000_000,
            sha256: Some(sha2_digest(remote_v3_content)),
            version: Some("3".into()),
        };

        let mut content_map = std::collections::HashMap::new();
        content_map.insert("rem-v2".into(), remote_v2_content.to_vec());
        content_map.insert("rem-v3".into(), remote_v3_content.to_vec());

        let transfer = PinnedDownloadTransfer { content_map };
        let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
        let sync = TrackedFileSync::new(transfer, history.clone());

        let root_str = root.to_str().unwrap();

        // 1. download_directory_guarded fails when root + relative != local
        let mismatch_request = DirectoryDownload {
            token: "token",
            remote: &remote_v2,
            local: "/other/unrelated/path/save.dat",
            root: root_str,
            relative: "slot/save.dat",
            folder: "folder",
            remote_name: "slot/save.dat",
        };
        let res1 = sync
            .download_directory_guarded(mismatch_request, guard.clone())
            .await;
        let Err(TrackedError::Local(err1)) = res1 else {
            panic!("Expected TrackedError::Local for mismatched destination");
        };
        assert_eq!(err1.kind(), io::ErrorKind::InvalidInput);
        assert!(
            err1.to_string()
                .contains("directory destination does not match the selected root")
        );

        // 2. download_pinned_guarded fails with InvalidInput on directory traversal
        let res2 = sync
            .download_pinned_guarded(
                "token",
                &remote_v2,
                root_str,
                "folder",
                "../escape.dat",
                guard.clone(),
            )
            .await;
        let Err(TrackedError::Local(err2)) = res2 else {
            panic!("Expected TrackedError::Local for path traversal");
        };
        assert_eq!(err2.kind(), io::ErrorKind::InvalidInput);
        assert!(!root.join("../escape.dat").exists());

        // 3. Valid download_pinned_guarded replaces v1 with v2, backs up v1, records checkpoint
        sync.download_pinned_guarded(
            "token",
            &remote_v2,
            root_str,
            "folder",
            "slot/save.dat",
            guard,
        )
        .await
        .unwrap();

        assert_eq!(fs::read(&slot_save).unwrap(), b"v2 content");
        let key_v2 = history.key(&slot_save, "folder", "slot/save.dat").unwrap();
        assert_eq!(history.get(&key_v2).unwrap().unwrap().remote_id, "rem-v2");
        let backup_v1_path = history
            .backup_directory()
            .join(&key_v2)
            .join(&v1_hash)
            .join("save.dat");
        assert_eq!(fs::read(&backup_v1_path).unwrap(), b"v1");

        // 4. download_pinned creates missing subdirectories and records checkpoint
        sync.download_pinned(
            "token",
            &remote_v3,
            root_str,
            "folder",
            "new_slot/save2.dat",
        )
        .await
        .unwrap();

        let new_save_path = root.join("new_slot/save2.dat");
        assert_eq!(fs::read(&new_save_path).unwrap(), b"v3 content");
        let key_v3 = history
            .key(&new_save_path, "folder", "new_slot/save2.dat")
            .unwrap();
        assert_eq!(history.get(&key_v3).unwrap().unwrap().remote_id, "rem-v3");
    });
}

fn sha2_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for &b in result.as_slice() {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}
