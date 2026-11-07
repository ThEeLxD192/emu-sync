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
    time::Duration,
};

fn snapshot(dir: &Path, bytes: &[u8]) -> UploadSnapshot {
    let source = dir.join("save.dat");
    fs::write(&source, bytes).unwrap();
    UploadSnapshot::prepare(&source).unwrap()
}
fn dir_local() -> String {
    "/saves/save.dat".into()
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
    let worker = {
        let (store, finished) = (store.clone(), finished.clone());
        thread::spawn(move || {
            let entries = store.list().unwrap();
            finished.store(true, Ordering::SeqCst);
            entries.len()
        })
    };
    thread::sleep(Duration::from_millis(300));
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
