use emusync_data_local::{
    StagedRestore, SyncHistoryStore, UploadEvidence, UploadSnapshot, file_sha256,
};
use std::fs;

#[test]
fn snapshots_and_restores_preserve_saves_on_verification_failures() {
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
fn backup_failure_prevents_replacement_and_cleans_staging() {
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
fn upload_evidence_recovered_constructor() {
    let path = std::path::PathBuf::from("/test/path/save.sav");
    let digest = "0".repeat(64);
    let evidence = UploadEvidence::recovered(path, digest);
    let debug = format!("{evidence:?}");
    assert!(debug.contains("save.sav"));
}

#[test]
fn staged_restore_rejects_existing_directory_and_rejects_concurrent_new_file() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("directory_target");
    fs::create_dir(&sub).unwrap();
    assert!(StagedRestore::prepare(&sub).is_err());

    let save = dir.path().join("new_save.dat");
    let store = SyncHistoryStore::new(&dir.path().join("config.json")).unwrap();
    let key = store.key(&save, "folder", "new_save.dat").unwrap();
    let stage = StagedRestore::prepare(&save).unwrap();
    fs::write(stage.path(), b"downloaded").unwrap();

    // Concurrently create save file before commit
    fs::write(&save, b"concurrently created").unwrap();

    // Commit must fail and preserve the concurrent file
    assert!(stage.commit(None, &store, &key).is_err());
    assert_eq!(fs::read(&save).unwrap(), b"concurrently created");
}

#[test]
fn upload_snapshot_verifies_remote_checksum_case_insensitively_and_staged_restore_skips_redundant_backup()
 {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    fs::write(&save, b"same content").unwrap();

    let store = SyncHistoryStore::new(&dir.path().join("config.json")).unwrap();
    let key = store.key(&save, "folder", "save.dat").unwrap();

    let snapshot = UploadSnapshot::prepare(&save).unwrap();

    // 1. Accepts uppercase hex checksum if matching digest
    let upper = snapshot.digest().to_uppercase();
    assert!(snapshot.verify_uploaded(Some(&upper), &save).is_ok());

    // 2. Fails if remote checksum differs
    let wrong = "f".repeat(64);
    let err = snapshot.verify_uploaded(Some(&wrong), &save).unwrap_err();
    assert!(
        err.to_string()
            .contains("uploaded content could not be verified"),
        "Expected 'uploaded content could not be verified', got: {err}"
    );

    // 3. StagedRestore does not create redundant backup when content is identical
    let stage = StagedRestore::prepare(&save).unwrap();
    fs::write(stage.path(), b"same content").unwrap();
    let committed_digest = stage.commit(Some(&upper), &store, &key).unwrap();
    assert_eq!(committed_digest, snapshot.digest());
    assert_eq!(fs::read(&save).unwrap(), b"same content");
    assert!(
        !store.backup_directory().exists(),
        "Backup directory must not be created for identical content replacement"
    );
}
