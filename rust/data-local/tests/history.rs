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
