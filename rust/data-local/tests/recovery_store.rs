use emusync_data_local::{
    RecoveryRecord, RecoveryState, RecoveryStore, Unverifiable, UploadSnapshot,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

fn snapshot(dir: &Path, bytes: &[u8]) -> UploadSnapshot {
    let source = dir.join("save.dat");
    fs::write(&source, bytes).unwrap();
    UploadSnapshot::prepare(&source).unwrap()
}
/// Absolute on each platform, as recorded paths always are.
fn dir_local() -> String {
    if cfg!(windows) {
        r"C:\saves\save.dat".into()
    } else {
        "/saves/save.dat".into()
    }
}
fn record(snapshot: &UploadSnapshot) -> RecoveryRecord {
    RecoveryRecord {
        remote_id: Some("remote".into()),
        local: dir_local(),
        folder: "folder".into(),
        remote_name: "save.dat".into(),
        observed: None,
        digest: snapshot.digest().to_owned(),
        connection: "fingerprint".into(),
    }
}
fn entry_dir(config: &Path, id: &str) -> PathBuf {
    config.with_file_name("config.json.sync-recovery").join(id)
}

#[test]
fn staged_upload_survives_snapshot_owner_and_a_new_process_view() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");
    let snapshot = snapshot(dir.path(), b"save bytes");
    let expected = record(&snapshot);
    let id = RecoveryStore::new(&config)
        .unwrap()
        .stage(&snapshot, expected.clone())
        .unwrap();
    drop(snapshot);
    let entries = RecoveryStore::new(&config).unwrap().list().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, id);
    let RecoveryState::Verified { record, snapshot } = &entries[0].state else {
        panic!("unexpected {:?}", entries[0].state);
    };
    assert_eq!(**record, expected);
    assert_eq!(fs::read(snapshot).unwrap(), b"save bytes");
}

#[cfg(unix)]
#[test]
fn recovery_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");
    let snapshot = snapshot(dir.path(), b"x");
    let store = RecoveryStore::new(&config).unwrap();
    let id = store.stage(&snapshot, record(&snapshot)).unwrap();
    let mode = |path: PathBuf| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let root = config.with_file_name("config.json.sync-recovery");
    assert_eq!(mode(root.clone()), 0o700);
    assert_eq!(mode(entry_dir(&config, &id)), 0o700);
    assert_eq!(mode(entry_dir(&config, &id).join("snapshot")), 0o600);
    assert_eq!(mode(entry_dir(&config, &id).join("manifest.json")), 0o600);
}

#[test]
fn unverifiable_entries_are_reported_and_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");
    let store = RecoveryStore::new(&config).unwrap();
    let mut ids = Vec::new();
    for _ in 0..5 {
        let snapshot = snapshot(dir.path(), b"original");
        ids.push(store.stage(&snapshot, record(&snapshot)).unwrap());
    }
    fs::write(entry_dir(&config, &ids[0]).join("snapshot"), b"tampered").unwrap();
    fs::write(
        entry_dir(&config, &ids[1]).join("manifest.json"),
        b"{ not json",
    )
    .unwrap();
    let future = br#"{"version":2,"remote_id":"x","unknown":"kept"}"#;
    fs::write(entry_dir(&config, &ids[2]).join("manifest.json"), future).unwrap();
    fs::remove_file(entry_dir(&config, &ids[3]).join("snapshot")).unwrap();
    let before: Vec<_> = ids
        .iter()
        .map(|id| fs::read(entry_dir(&config, id).join("manifest.json")).unwrap())
        .collect();
    let states: std::collections::HashMap<_, _> = store
        .list()
        .unwrap()
        .into_iter()
        .map(|entry| (entry.id, entry.state))
        .collect();
    assert_eq!(
        states[&ids[0]],
        RecoveryState::Unverifiable(Unverifiable::DigestMismatch)
    );
    assert_eq!(
        states[&ids[1]],
        RecoveryState::Unverifiable(Unverifiable::UnreadableManifest)
    );
    assert_eq!(
        states[&ids[2]],
        RecoveryState::Unverifiable(Unverifiable::UnknownVersion(2))
    );
    assert_eq!(
        states[&ids[3]],
        RecoveryState::Unverifiable(Unverifiable::SnapshotMissing)
    );
    assert!(matches!(states[&ids[4]], RecoveryState::Verified { .. }));
    for (id, bytes) in ids.iter().zip(before) {
        assert_eq!(
            fs::read(entry_dir(&config, id).join("manifest.json")).unwrap(),
            bytes
        );
    }
}

#[test]
fn removal_accepts_only_entry_identifiers() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");
    let outside = dir.path().join("keep.txt");
    fs::write(&outside, b"keep").unwrap();
    let snapshot = snapshot(dir.path(), b"x");
    let store = RecoveryStore::new(&config).unwrap();
    let id = store.stage(&snapshot, record(&snapshot)).unwrap();
    for bad in ["", "..", "op-", "op-../..", "op-a/b", ".lock", "other"] {
        assert!(store.remove(bad).is_err(), "{bad:?}");
    }
    assert!(outside.exists());
    store.remove(&id).unwrap();
    store.remove(&id).unwrap();
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn stage_rejects_a_record_for_other_content_and_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");
    let snapshot = snapshot(dir.path(), b"x");
    let mut wrong = record(&snapshot);
    wrong.digest = "0".repeat(64);
    let store = RecoveryStore::new(&config).unwrap();
    assert!(store.stage(&snapshot, wrong).is_err());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn directory_lock_excludes_other_holders_until_released() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");
    let snapshot = snapshot(dir.path(), b"x");
    let store = Arc::new(RecoveryStore::new(&config).unwrap());
    store.stage(&snapshot, record(&snapshot)).unwrap();
    // Another handle on the lock file stands in for another process.
    let held = fs::OpenOptions::new()
        .write(true)
        .open(config.with_file_name("config.json.sync-recovery/.lock"))
        .unwrap();
    held.lock().unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let worker = {
        let (store, finished) = (store.clone(), finished.clone());
        thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let entries = store.list().unwrap();
            finished.store(true, Ordering::SeqCst);
            entries.len()
        })
    };
    ready_rx.recv().unwrap();
    thread::yield_now();
    assert!(!finished.load(Ordering::SeqCst));
    held.unlock().unwrap();
    assert_eq!(worker.join().unwrap(), 1);
}

#[cfg(unix)]
#[test]
fn symbolic_link_recovery_directory_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    let config = dir.path().join("config.json");
    std::os::unix::fs::symlink(&elsewhere, dir.path().join("config.json.sync-recovery")).unwrap();
    let snapshot = snapshot(dir.path(), b"x");
    let store = RecoveryStore::new(&config).unwrap();
    assert!(store.stage(&snapshot, record(&snapshot)).is_err());
    assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
}

#[test]
fn has_unresolved_matches_exact_entry_and_ignores_mismatches_or_unverifiable() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");
    let store = RecoveryStore::new(&config).unwrap();
    let snapshot = snapshot(dir.path(), b"save bytes");
    let rec = record(&snapshot);
    let local_path = Path::new(&rec.local);
    assert!(
        !store
            .has_unresolved(local_path, &rec.folder, &rec.remote_name)
            .unwrap()
    );

    let id = store.stage(&snapshot, rec.clone()).unwrap();
    assert!(
        store
            .has_unresolved(local_path, &rec.folder, &rec.remote_name)
            .unwrap()
    );
    assert!(
        !store
            .has_unresolved(Path::new("/other/save.dat"), &rec.folder, &rec.remote_name)
            .unwrap()
    );
    assert!(
        !store
            .has_unresolved(local_path, "other-folder", &rec.remote_name)
            .unwrap()
    );
    assert!(
        !store
            .has_unresolved(local_path, &rec.folder, "other.sav")
            .unwrap()
    );

    // Tampering snapshot makes it Unverifiable, which should not count as unresolved
    fs::write(entry_dir(&config, &id).join("snapshot"), b"corrupted").unwrap();
    assert!(
        !store
            .has_unresolved(local_path, &rec.folder, &rec.remote_name)
            .unwrap()
    );
}

#[cfg(unix)]
#[test]
fn dangling_symlink_recovery_directory_fails_closed_without_creating_target() {
    let dir = tempfile::tempdir().unwrap();
    let nonexistent_target = dir.path().join("does_not_exist");
    let config = dir.path().join("config.json");
    let recovery_dir = dir.path().join("config.json.sync-recovery");
    std::os::unix::fs::symlink(&nonexistent_target, &recovery_dir).unwrap();

    let store = RecoveryStore::new(&config).unwrap();
    assert!(store.list().is_err());
    assert!(
        store
            .has_unresolved(Path::new("/save"), "folder", "save.dat")
            .is_err()
    );

    let snapshot = snapshot(dir.path(), b"x");
    assert!(store.stage(&snapshot, record(&snapshot)).is_err());
    assert!(!nonexistent_target.exists());
}

#[test]
fn oversized_manifest_and_non_regular_files_are_reported_unverifiable_and_rogue_entries_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");
    let store = RecoveryStore::new(&config).unwrap();

    let snap = snapshot(dir.path(), b"snapshot content");
    let rec = record(&snap);

    let id0 = store.stage(&snap, rec.clone()).unwrap();
    let id1 = store.stage(&snap, rec.clone()).unwrap();
    let id2 = store.stage(&snap, rec).unwrap();

    // In id0: overwrite manifest.json with valid JSON > 65_537 bytes (MAX_MANIFEST_BYTES is 64*1024 = 65536)
    let mut large_record = record(&snap);
    large_record.folder = "A".repeat(70_000);
    let mut val = serde_json::to_value(&large_record).unwrap();
    val.as_object_mut()
        .unwrap()
        .insert("version".into(), serde_json::json!(1));
    let manifest0_path = entry_dir(&config, &id0).join("manifest.json");
    fs::write(&manifest0_path, serde_json::to_vec(&val).unwrap()).unwrap();
    assert!(fs::metadata(&manifest0_path).unwrap().len() > 65_536);

    // In id1: replace manifest.json with a directory
    let manifest1_path = entry_dir(&config, &id1).join("manifest.json");
    fs::remove_file(&manifest1_path).unwrap();
    fs::create_dir(&manifest1_path).unwrap();

    // In id2: replace snapshot with a directory
    let snapshot2_path = entry_dir(&config, &id2).join("snapshot");
    fs::remove_file(&snapshot2_path).unwrap();
    fs::create_dir(&snapshot2_path).unwrap();

    // In recovery root: add rogue entries (regular file starting with op-, and directory not starting with op-)
    let recovery_root = config.with_file_name("config.json.sync-recovery");
    fs::write(recovery_root.join("op-notadir"), b"file bytes").unwrap();
    fs::create_dir(recovery_root.join("not-an-op")).unwrap();

    let entries = store.list().unwrap();
    assert_eq!(
        entries.len(),
        3,
        "Only valid op-* directories should be listed"
    );

    let e0 = entries.iter().find(|e| e.id == id0).unwrap();
    assert_eq!(
        e0.state,
        RecoveryState::Unverifiable(Unverifiable::UnreadableManifest)
    );

    let e1 = entries.iter().find(|e| e.id == id1).unwrap();
    assert_eq!(
        e1.state,
        RecoveryState::Unverifiable(Unverifiable::UnreadableManifest)
    );

    let e2 = entries.iter().find(|e| e.id == id2).unwrap();
    assert_eq!(
        e2.state,
        RecoveryState::Unverifiable(Unverifiable::SnapshotMissing)
    );
}

#[test]
fn non_directory_root_symlink_lock_and_non_directory_entry_removal_are_rejected() {
    // 1. Recovery root path is a regular file instead of a directory
    let dir1 = tempfile::tempdir().unwrap();
    let config1 = dir1.path().join("config.json");
    fs::write(
        config1.with_file_name("config.json.sync-recovery"),
        b"regular file",
    )
    .unwrap();
    let store1 = RecoveryStore::new(&config1).unwrap();
    let snap1 = snapshot(dir1.path(), b"data");
    let err1 = store1.stage(&snap1, record(&snap1)).unwrap_err();
    assert!(
        err1.to_string()
            .contains("recovery path is not a directory"),
        "Expected 'recovery path is not a directory', got: {err1}"
    );

    // 2. Recovery entry is a regular file instead of a directory during remove()
    let dir2 = tempfile::tempdir().unwrap();
    let config2 = dir2.path().join("config.json");
    let store2 = RecoveryStore::new(&config2).unwrap();
    let snap2 = snapshot(dir2.path(), b"data");
    store2.stage(&snap2, record(&snap2)).unwrap(); // ensures root directory exists
    let recovery_root2 = config2.with_file_name("config.json.sync-recovery");
    let op_regular = recovery_root2.join("op-regular");
    fs::write(&op_regular, b"regular file entry").unwrap();

    let err2 = store2.remove("op-regular").unwrap_err();
    assert!(
        err2.to_string()
            .contains("recovery entry is not a directory"),
        "Expected 'recovery entry is not a directory', got: {err2}"
    );
    assert!(
        op_regular.exists(),
        "Non-directory entry must not be deleted"
    );

    // 3. Recovery lock is a symbolic link
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let dir3 = tempfile::tempdir().unwrap();
        let config3 = dir3.path().join("config.json");
        let store3 = RecoveryStore::new(&config3).unwrap();
        let snap3 = snapshot(dir3.path(), b"data");
        store3.stage(&snap3, record(&snap3)).unwrap();

        let lock_path = config3.with_file_name("config.json.sync-recovery/.lock");
        fs::remove_file(&lock_path).unwrap();
        let external_lock = dir3.path().join("external_lock");
        fs::write(&external_lock, b"lock content").unwrap();
        symlink(&external_lock, &lock_path).unwrap();

        let err_list = store3.list().unwrap_err();
        assert!(
            err_list
                .to_string()
                .contains("recovery lock is a symbolic link"),
            "Expected 'recovery lock is a symbolic link', got: {err_list}"
        );

        let err_stage = store3.stage(&snap3, record(&snap3)).unwrap_err();
        assert!(
            err_stage
                .to_string()
                .contains("recovery lock is a symbolic link"),
            "Expected 'recovery lock is a symbolic link', got: {err_stage}"
        );
    }
}
