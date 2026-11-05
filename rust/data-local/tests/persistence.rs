use emusync_data_local::{ConfigManager, HistoryCheckpoint, SyncHistoryStore, file_sha256};
use emusync_domain::{
    config::{ConfigurationStore, CredentialStore, Credentials},
    library::GameEntry,
};
use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};

fn checkpoint(local: &str) -> HistoryCheckpoint {
    HistoryCheckpoint {
        local_sha256: local.into(),
        remote_id: "remote".into(),
        remote_modified_time: "2026-10-01T12:00:00Z".into(),
        remote_sha256: None,
        remote_version: None,
    }
}

#[test]
fn history_keeps_legacy_data_and_concurrent_records_and_refuses_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let a = Arc::new(SyncHistoryStore::new(&path).unwrap());
    fs::write(a.state_file(), r#"{"formatVersion":1,"saves":{"legacy":{"localSha256":"old","remoteId":"r","remoteModifiedTime":"2026-09-01T12:00:00Z"}}}"#).unwrap();
    let barrier = Arc::new(Barrier::new(9));
    thread::scope(|scope| {
        for n in 0..8 {
            let path = &path;
            let barrier = barrier.clone();
            scope.spawn(move || {
                let store = SyncHistoryStore::new(path).unwrap();
                barrier.wait();
                store
                    .record(&n.to_string(), checkpoint(&n.to_string()))
                    .unwrap();
            });
        }
        barrier.wait();
    });
    assert_eq!(a.get("legacy").unwrap().unwrap().local_sha256, "old");
    for n in 0..8 {
        assert_eq!(
            a.get(&n.to_string()).unwrap().unwrap(),
            checkpoint(&n.to_string())
        );
    }
    let file = dir.path().join("nested/../save.dat");
    assert_eq!(
        a.key(&file, "folder", "save.dat").unwrap(),
        a.key(&dir.path().join("save.dat"), "folder", "save.dat")
            .unwrap()
    );
    #[cfg(unix)]
    assert_eq!(
        a.key(
            std::path::Path::new("/synthetic/sub/../save.dat"),
            "folder",
            "slot/save.dat"
        )
        .unwrap(),
        "479dc29725c5190cb0e60c234cf9ec987bb9c8e70c0650edf90154391fbdce60"
    );
    assert_ne!(
        a.key(&file, "folder", "save.dat").unwrap(),
        a.key(&file, "other", "save.dat").unwrap()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(a.state_file()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    for invalid in ["broken", r#"{"formatVersion":2,"saves":{}}"#] {
        fs::write(a.state_file(), invalid).unwrap();
        assert!(a.record("x", checkpoint("x")).is_err());
        assert_eq!(fs::read_to_string(a.state_file()).unwrap(), invalid);
    }
}

#[test]
fn backup_verifies_source_and_existing_recovery_copy() {
    let dir = tempfile::tempdir().unwrap();
    let store = SyncHistoryStore::new(&dir.path().join("config.json")).unwrap();
    let save = dir.path().join("save.dat");
    fs::write(&save, b"ORIGINAL").unwrap();
    let hash = file_sha256(&save).unwrap();
    let key = store.key(&save, "folder", "save.dat").unwrap();
    store.backup(&key, &save, &hash).unwrap();
    let copy = store
        .backup_directory()
        .join(&key)
        .join(&hash)
        .join("save.dat");
    assert_eq!(fs::read(&copy).unwrap(), b"ORIGINAL");
    fs::write(&copy, b"damaged").unwrap();
    assert!(store.backup(&key, &save, &hash).is_err());
    assert_eq!(fs::read(&save).unwrap(), b"ORIGINAL");
    assert!(store.backup("../outside", &save, &hash).is_err());
}

#[test]
fn config_loads_legacy_defaults_round_trips_and_preserves_concurrent_updates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    fs::write(&path, r#"{"googleDrive":{"clientId":"id","clientSecret":"secret"},"entries":[{"type":"emulator","name":"Emu","executablePath":"emu","romsDirectory":"roms","extensions":["gb"],"titleByRom":{"first":"One","second":"Two"}},{"type":"native","name":"Game","executablePath":"game","arguments":[]}],"future":true}"#).unwrap();
    let store = ConfigManager::new(&path).unwrap();
    let config = store.load().unwrap();
    let GameEntry::Emulator(emu) = &config.entries[0] else {
        panic!()
    };
    assert_eq!(emu.common.arguments, ["{ROM}"]);
    assert_eq!(emu.title_by_rom[0].0, "first");
    store.save(&config).unwrap();
    assert_eq!(store.load().unwrap(), config);
    let snapshot = store.snapshot(&config).unwrap();
    store
        .update_credentials(|_| {
            Some(Credentials {
                client_id: "new".into(),
                client_secret: "secret".into(),
                refresh_token: None,
            })
        })
        .unwrap();
    assert!(!store.compare_and_set(&snapshot, None, &config).unwrap());
    let latest = store.snapshot(&config).unwrap();
    store
        .update(|mut c| {
            c.entries.remove(1);
            c
        })
        .unwrap();
    assert!(
        store
            .compare_and_set(
                &latest,
                Some(Credentials {
                    client_id: "new".into(),
                    client_secret: "secret".into(),
                    refresh_token: Some("token".into())
                }),
                &config
            )
            .unwrap()
    );
    assert_eq!(store.load().unwrap().entries.len(), 1);
}

#[test]
fn explicit_empty_emulator_arguments_survive_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    fs::write(&path,r#"{"entries":[{"type":"emulator","name":"E","executablePath":"e","romsDirectory":"r","extensions":[],"arguments":[]}]}"#).unwrap();
    let store = ConfigManager::new(&path).unwrap();
    let config = store.load().unwrap();
    store.save(&config).unwrap();
    assert_eq!(store.load().unwrap(), config);
}

#[test]
fn snapshots_and_restores_preserve_saves_on_verification_failures() {
    use emusync_data_local::{StagedRestore, UploadSnapshot};
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    fs::write(&save, b"old").unwrap();
    let store = SyncHistoryStore::new(&dir.path().join("config.json")).unwrap();
    let key = store.key(&save, "folder", "save.dat").unwrap();
    let snapshot = UploadSnapshot::prepare(&save).unwrap();
    assert_eq!(fs::read(snapshot.path()).unwrap(), b"old");
    fs::write(&save, b"new local").unwrap();
    assert!(snapshot.verify_uploaded(None, &save).is_err());
    let snapshot_dir = snapshot.directory().to_owned();
    drop(snapshot);
    assert!(!snapshot_dir.exists());
    let restore = StagedRestore::prepare(&save).unwrap();
    let staging = restore.path().to_owned();
    fs::write(&staging, b"remote").unwrap();
    assert!(restore.commit(Some(&"0".repeat(64)), &store, &key).is_err());
    assert_eq!(fs::read(&save).unwrap(), b"new local");
    assert!(!staging.exists());
    let restore = StagedRestore::prepare(&save).unwrap();
    fs::write(restore.path(), b"remote").unwrap();
    fs::write(&save, b"newer local").unwrap();
    assert!(restore.commit(None, &store, &key).is_err());
    assert_eq!(fs::read(&save).unwrap(), b"newer local");
    let old_hash = file_sha256(&save).unwrap();
    let restore = StagedRestore::prepare(&save).unwrap();
    fs::write(restore.path(), b"remote").unwrap();
    restore.commit(None, &store, &key).unwrap();
    assert_eq!(fs::read(&save).unwrap(), b"remote");
    assert_eq!(
        fs::read(
            store
                .backup_directory()
                .join(key)
                .join(old_hash)
                .join("save.dat")
        )
        .unwrap(),
        b"newer local"
    );
}

#[test]
fn tracked_transfers_verify_content_record_history_and_keep_local_changes() {
    use emusync_data_local::TrackedFileSync;
    use emusync_domain::sync::{CloudSaveTransfer, Decision, LocalSaveFiles, RemoteState};
    use std::path::Path;
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

#[test]
fn concurrent_config_updates_merge_and_legacy_targets_keep_paths() {
    use emusync_domain::library::{EntryCommon, NativeGame, SaveTargetType};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let save = dir.path().join("legacy.sav");
    fs::write(&save, b"save").unwrap();
    let manager = ConfigManager::new(&path).unwrap();
    manager
        .update(|mut c| {
            c.entries.push(GameEntry::Native(NativeGame {
                common: EntryCommon {
                    name: "legacy".into(),
                    save_paths: vec![save.to_str().unwrap().into()],
                    ..Default::default()
                },
                ..Default::default()
            }));
            c
        })
        .unwrap();
    let barrier = Arc::new(Barrier::new(9));
    thread::scope(|scope| {
        for n in 0..8 {
            let path = &path;
            let barrier = barrier.clone();
            scope.spawn(move || {
                let store = ConfigManager::new(path).unwrap();
                barrier.wait();
                store
                    .update(|mut c| {
                        c.entries.push(GameEntry::Native(NativeGame {
                            common: EntryCommon {
                                name: n.to_string(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }));
                        c
                    })
                    .unwrap();
            });
        }
        barrier.wait();
    });
    let config = manager.load().unwrap();
    assert_eq!(config.entries.len(), 9);
    let common = config.entries[0].common();
    assert_eq!(common.save_paths, [save.to_str().unwrap()]);
    assert_eq!(common.save_targets[0].kind, SaveTargetType::File);
    assert_eq!(common.save_targets[0].path, save.to_str().unwrap());
}

#[test]
fn backup_failure_prevents_replacement_and_cleans_staging() {
    use emusync_data_local::StagedRestore;
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    fs::write(&save, b"old").unwrap();
    let store = SyncHistoryStore::new(&dir.path().join("config.json")).unwrap();
    let key = store.key(&save, "folder", "save.dat").unwrap();
    fs::write(store.backup_directory(), b"not a directory").unwrap();
    let stage = StagedRestore::prepare(&save).unwrap();
    let staged = stage.path().to_owned();
    fs::write(&staged, b"remote").unwrap();
    assert!(stage.commit(None, &store, &key).is_err());
    assert_eq!(fs::read(&save).unwrap(), b"old");
    assert!(!staged.exists());
    assert!(store.get(&key).unwrap().is_none());
}

#[test]
fn literal_config_preserves_all_nondefault_fields_and_ordered_titles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let literal = r#"{"googleDrive":{"clientId":"client","clientSecret":"secret","refreshToken":"refresh"},"entries":[{"type":"emulator","name":"System","executablePath":"/bin/emu","arguments":["--game","{ROM}"],"romsDirectory":"/roms","extensions":["iso","bin"],"savePaths":["/synthetic/system-file"],"saveTargets":[{"path":"/synthetic/system-file","type":"file","cloudPath":"slot/save.dat"},{"path":"/synthetic/system-dir","type":"directory","cloudPath":"profile"},{"path":"/synthetic/unknown","type":"unknown","cloudPath":"legacy"}],"savePathsByRom":{"Game.iso":["/synthetic/game-file"]},"saveTargetsByRom":{"Game.iso":[{"path":"/synthetic/game-file","type":"file","cloudPath":"game.sav"}]},"coverPathByRom":{"Game.iso":"/covers/game.png"},"titleByRom":{"first":"One","second":"Two"},"group":"Console","cloudFolder":"Existing saves","driveFileId":"remote-system","steamAppId":-123,"steamProcessName":"emu-process","fullscreenArgs":"--fullscreen","coversDirectory":"/covers"},{"type":"native","name":"Native","executablePath":"/bin/game","arguments":["--safe"],"savePaths":["/synthetic/native-file"],"saveTargets":[{"path":"/synthetic/native-file","type":"file","cloudPath":"native.sav"}],"waitForProcess":"game-process","group":"PC","cloudFolder":"Native saves","driveFileId":"remote-native","steamAppId":456,"coverPath":"/covers/native.png"}]}"#;
    fs::write(&path, literal).unwrap();
    let store = ConfigManager::new(&path).unwrap();
    let config = store.load().unwrap();
    store.save(&config).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(literal).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap()
    );
    let GameEntry::Emulator(emu) = &config.entries[0] else {
        panic!()
    };
    assert_eq!(emu.title_by_rom[0].0, "first");
    assert_eq!(emu.title_by_rom[1].0, "second");
}
