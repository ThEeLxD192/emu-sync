use emusync_data_local::{HistoryCheckpoint, SyncHistoryStore, file_sha256};
use std::{
    fs,
    path::Path,
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
    fs::write(
        a.state_file(),
        r#"{"formatVersion":1,"saves":{"legacy":{"localSha256":"old","remoteId":"r","remoteModifiedTime":"2026-09-01T12:00:00Z"}}}"#,
    )
    .unwrap();
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
            Path::new("/synthetic/sub/../save.dat"),
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
    assert!(store.backup(&"0".repeat(63), &save, &hash).is_err());
    assert!(store.backup(&key, &save, &"0".repeat(65)).is_err());
    assert!(
        store
            .backup(&key, &save, &format!("{}g", "0".repeat(63)))
            .is_err()
    );
}

#[test]
fn file_sha256_computes_exact_digest_and_detects_missing() {
    let dir = tempfile::tempdir().unwrap();
    let empty_file = dir.path().join("empty.bin");
    fs::write(&empty_file, b"").unwrap();
    // SHA256 of empty bytes
    assert_eq!(
        file_sha256(&empty_file).unwrap(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );

    let hello_file = dir.path().join("hello.txt");
    fs::write(&hello_file, b"hello world").unwrap();
    // SHA256 of "hello world"
    assert_eq!(
        file_sha256(&hello_file).unwrap(),
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );

    let missing = dir.path().join("missing.bin");
    assert!(file_sha256(&missing).is_err());
}

#[test]
fn history_checkpoint_round_trip_serialization() {
    let cp = HistoryCheckpoint {
        local_sha256: "abc123".into(),
        remote_id: "rem-456".into(),
        remote_modified_time: "2026-11-07T00:00:00Z".into(),
        remote_sha256: Some("sha-789".into()),
        remote_version: Some("v2".into()),
    };
    let json = serde_json::to_string(&cp).unwrap();
    let deserialized: HistoryCheckpoint = serde_json::from_str(&json).unwrap();
    assert_eq!(cp, deserialized);
}

#[test]
fn backup_rejects_non_regular_existing_copy_and_maintains_strict_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let store = SyncHistoryStore::new(&config_path).unwrap();

    let save = dir.path().join("save.dat");
    fs::write(&save, b"data").unwrap();
    let hash = file_sha256(&save).unwrap();
    let key = store.key(&save, "folder", "save.dat").unwrap();

    // 1. Initial backup succeeds
    store.backup(&key, &save, &hash).unwrap();

    // 2. Second backup with identical content is idempotent and succeeds
    store.backup(&key, &save, &hash).unwrap();

    let destination = store
        .backup_directory()
        .join(&key)
        .join(&hash)
        .join("save.dat");
    assert_eq!(fs::read(&destination).unwrap(), b"data");

    // 3. Strict permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let backup_dir = store.backup_directory();
        let key_dir = backup_dir.join(&key);
        let version_dir = key_dir.join(&hash);
        let source_json = key_dir.join("source.json");

        assert_eq!(
            fs::metadata(backup_dir).unwrap().permissions().mode() & 0o777,
            0o700,
            "backup directory must have 0o700 mode"
        );
        assert_eq!(
            fs::metadata(&key_dir).unwrap().permissions().mode() & 0o777,
            0o700,
            "key directory must have 0o700 mode"
        );
        assert_eq!(
            fs::metadata(&version_dir).unwrap().permissions().mode() & 0o777,
            0o700,
            "version directory must have 0o700 mode"
        );
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o600,
            "backup file must have 0o600 mode"
        );
        assert_eq!(
            fs::metadata(&source_json).unwrap().permissions().mode() & 0o777,
            0o600,
            "source.json must have 0o600 mode"
        );
    }

    // 4. Replacing destination with a directory causes backup to fail
    fs::remove_file(&destination).unwrap();
    fs::create_dir(&destination).unwrap();
    let err_dir = store.backup(&key, &save, &hash).unwrap_err();
    assert!(
        err_dir
            .to_string()
            .contains("existing recovery copy is not a regular file"),
        "Expected 'existing recovery copy is not a regular file', got: {err_dir}"
    );

    // 5. On Unix, replacing destination with a symlink to an external file (even with same content) fails
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        fs::remove_dir(&destination).unwrap();
        let external_file = dir.path().join("external_match.dat");
        fs::write(&external_file, b"data").unwrap();
        symlink(&external_file, &destination).unwrap();

        let err_symlink = store.backup(&key, &save, &hash).unwrap_err();
        assert!(
            err_symlink
                .to_string()
                .contains("existing recovery copy is not a regular file"),
            "Expected 'existing recovery copy is not a regular file', got: {err_symlink}"
        );
        // Ensure external file was not altered or overwritten
        assert_eq!(fs::read(&external_file).unwrap(), b"data");
    }
}
