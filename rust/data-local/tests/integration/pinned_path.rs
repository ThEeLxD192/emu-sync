#![cfg(target_os = "linux")]
use emusync_data_local::PinnedRemoteDestination;
use std::{fs, io::Write, os::unix::fs::symlink};

#[test]
fn parent_swap_to_symlink_does_not_redirect_creation_outside_selected_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::create_dir(root.join("slot")).unwrap();
    let pinned = PinnedRemoteDestination::open(&root, "slot/save.dat")
        .unwrap()
        .unwrap();
    fs::rename(root.join("slot"), root.join("original-slot")).unwrap();
    symlink(&outside, root.join("slot")).unwrap();
    pinned
        .create_new()
        .unwrap()
        .write_all(b"synthetic")
        .unwrap();
    assert_eq!(
        fs::read(root.join("original-slot/save.dat")).unwrap(),
        b"synthetic"
    );
    assert!(!outside.join("save.dat").exists());
}
#[test]
fn refuses_traversal_existing_files_and_untrusted_symlinks() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("save"), b"untouched").unwrap();
    assert!(
        PinnedRemoteDestination::open(&root, "../save")
            .unwrap()
            .is_none()
    );
    symlink(&outside, root.join("link")).unwrap();
    assert!(PinnedRemoteDestination::open(&root, "link/save").is_err());
    symlink(outside.join("save"), root.join("save")).unwrap();
    let pinned = PinnedRemoteDestination::open(&root, "save")
        .unwrap()
        .unwrap();
    assert!(pinned.create_new().is_err());
    assert_eq!(fs::read(outside.join("save")).unwrap(), b"untouched");
    fs::write(root.join("existing"), b"original").unwrap();
    let existing = PinnedRemoteDestination::open(&root, "existing")
        .unwrap()
        .unwrap();
    assert!(existing.create_new().is_err());
    assert_eq!(fs::read(root.join("existing")).unwrap(), b"original");
}

#[test]
fn pinned_restore_rejects_parent_symlink_swapped_after_staging() {
    use emusync_data_local::{StagedRestore, SyncHistoryStore};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::create_dir(root.join("slot")).unwrap();
    fs::write(root.join("slot/save.dat"), b"original").unwrap();
    fs::write(outside.join("save.dat"), b"outside").unwrap();
    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();
    let logical = root.join("slot/save.dat");
    let key = history.key(&logical, "folder", "slot/save.dat").unwrap();
    let stage = StagedRestore::prepare_pinned(&root, "slot/save.dat")
        .unwrap()
        .unwrap();
    fs::write(stage.path(), b"restored").unwrap();
    fs::rename(root.join("slot"), root.join("original-slot")).unwrap();
    symlink(&outside, root.join("slot")).unwrap();
    assert!(stage.commit(None, &history, &key).is_err());
    assert_eq!(
        fs::read(root.join("original-slot/save.dat")).unwrap(),
        b"original"
    );
    assert_eq!(fs::read(outside.join("save.dat")).unwrap(), b"outside");
    assert!(!history.backup_directory().exists());
}
#[test]
fn pinned_restore_backs_up_original_and_preserves_logical_source_path() {
    use emusync_data_local::{StagedRestore, SyncHistoryStore};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let logical = root.join("save.dat");
    fs::write(&logical, b"original").unwrap();
    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();
    let key = history.key(&logical, "folder", "save.dat").unwrap();
    let stage = StagedRestore::prepare_pinned(&root, "save.dat")
        .unwrap()
        .unwrap();
    fs::write(stage.path(), b"restored").unwrap();
    stage.commit(None, &history, &key).unwrap();
    assert_eq!(fs::read(&logical).unwrap(), b"restored");
    let old_hash = "0682c5f2076f099c34cfdd15a9e063849ed437a49677e6fcc5b4198c76575be5";
    assert_eq!(
        fs::read(
            history
                .backup_directory()
                .join(&key)
                .join(old_hash)
                .join("save.dat")
        )
        .unwrap(),
        b"original"
    );
    let source: serde_json::Value = serde_json::from_slice(
        &fs::read(history.backup_directory().join(&key).join("source.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(source["originalPath"], logical.to_str().unwrap());
}
#[test]
fn pinned_restore_rejects_new_file_created_after_staging() {
    use emusync_data_local::{StagedRestore, SyncHistoryStore};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();
    let key = history.key(&root.join("save"), "folder", "save").unwrap();
    let stage = StagedRestore::prepare_pinned(&root, "save")
        .unwrap()
        .unwrap();
    fs::write(stage.path(), b"remote").unwrap();
    fs::write(root.join("save"), b"new local").unwrap();
    assert!(stage.commit(None, &history, &key).is_err());
    assert_eq!(fs::read(root.join("save")).unwrap(), b"new local");
}

#[test]
fn pinned_restore_rejects_final_symlink_inserted_after_staging() {
    use emusync_data_local::{StagedRestore, SyncHistoryStore};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let external = temp.path().join("outside");
    fs::write(&external, b"outside").unwrap();
    fs::write(root.join("save"), b"original").unwrap();
    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();
    let key = history.key(&root.join("save"), "folder", "save").unwrap();
    let stage = StagedRestore::prepare_pinned(&root, "save")
        .unwrap()
        .unwrap();
    fs::write(stage.path(), b"remote").unwrap();
    fs::rename(root.join("save"), root.join("original")).unwrap();
    symlink(&external, root.join("save")).unwrap();
    assert!(stage.commit(None, &history, &key).is_err());
    assert_eq!(fs::read(&external).unwrap(), b"outside");
    assert_eq!(fs::read(root.join("original")).unwrap(), b"original");
}

#[test]
fn pinned_restore_creates_missing_descendants_without_following_links() {
    use emusync_data_local::{StagedRestore, SyncHistoryStore};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();
    let key = history
        .key(&root.join("one/two/save"), "folder", "one/two/save")
        .unwrap();
    let stage = StagedRestore::prepare_pinned(&root, "one/two/save")
        .unwrap()
        .unwrap();
    fs::write(stage.path(), b"remote").unwrap();
    stage.commit(None, &history, &key).unwrap();
    assert_eq!(fs::read(root.join("one/two/save")).unwrap(), b"remote");
    symlink(&outside, root.join("link")).unwrap();
    assert!(StagedRestore::prepare_pinned(&root, "link/new/save").is_err());
    assert!(!outside.join("new").exists());
    assert!(
        StagedRestore::prepare_pinned(&root, "fresh/../outside")
            .unwrap()
            .is_none()
    );
    assert!(!root.join("fresh").exists());
}

#[test]
fn concurrent_pinned_creations_share_new_directories_without_losing_files() {
    use std::sync::{Arc, Barrier};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    std::thread::scope(|scope| {
        for (name, bytes) in [
            ("one/two/a", b"a".as_slice()),
            ("one/two/b", b"b".as_slice()),
        ] {
            let root = &root;
            let barrier = barrier.clone();
            scope.spawn(move || {
                barrier.wait();
                let target = PinnedRemoteDestination::open_or_create(root, name)
                    .unwrap()
                    .unwrap();
                target.create_new().unwrap().write_all(bytes).unwrap();
            });
        }
    });
    assert_eq!(fs::read(root.join("one/two/a")).unwrap(), b"a");
    assert_eq!(fs::read(root.join("one/two/b")).unwrap(), b"b");
}

#[test]
fn pinned_restore_creates_selected_root_and_accepts_user_selected_root_symlink() {
    use emusync_data_local::{StagedRestore, SyncHistoryStore};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("missing/root");
    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();
    let key = history
        .key(&root.join("nested/save"), "folder", "nested/save")
        .unwrap();
    let stage = StagedRestore::prepare_pinned(&root, "nested/save")
        .unwrap()
        .unwrap();
    fs::write(stage.path(), b"new").unwrap();
    stage.commit(None, &history, &key).unwrap();
    assert_eq!(fs::read(root.join("nested/save")).unwrap(), b"new");
    let link = temp.path().join("selected-link");
    symlink(&root, &link).unwrap();
    let key = history
        .key(&link.join("other/save"), "folder", "other/save")
        .unwrap();
    let stage = StagedRestore::prepare_pinned(&link, "other/save")
        .unwrap()
        .unwrap();
    fs::write(stage.path(), b"linked").unwrap();
    stage.commit(None, &history, &key).unwrap();
    assert_eq!(fs::read(root.join("other/save")).unwrap(), b"linked");
}

#[test]
fn pinned_restore_rejects_descendant_directory_swapped_to_symlink_pointing_to_moved_target() {
    use emusync_data_local::{StagedRestore, SyncHistoryStore};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("slot")).unwrap();
    fs::write(root.join("slot/save.dat"), b"original").unwrap();
    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();
    let logical = root.join("slot/save.dat");
    let key = history.key(&logical, "folder", "slot/save.dat").unwrap();
    let stage = StagedRestore::prepare_pinned(&root, "slot/save.dat")
        .unwrap()
        .unwrap();
    fs::write(stage.path(), b"restored").unwrap();
    fs::rename(root.join("slot"), root.join("moved_slot")).unwrap();
    symlink(root.join("moved_slot"), root.join("slot")).unwrap();
    assert!(stage.commit(None, &history, &key).is_err());
    assert_eq!(
        fs::read(root.join("moved_slot/save.dat")).unwrap(),
        b"original"
    );
    assert!(!history.backup_directory().exists());
}

#[test]
fn pinned_restore_rejects_grandparent_ancestor_swapped_to_symlink_pointing_to_moved_parent() {
    use emusync_data_local::{StagedRestore, SyncHistoryStore};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir_all(root.join("a/b")).unwrap();
    fs::write(root.join("a/b/save.dat"), b"original").unwrap();

    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();
    let logical = root.join("a/b/save.dat");
    let key = history.key(&logical, "folder", "a/b/save.dat").unwrap();

    let stage = StagedRestore::prepare_pinned(&root, "a/b/save.dat")
        .unwrap()
        .unwrap();
    fs::write(stage.path(), b"restored").unwrap();

    // Adversarial injection: rename ancestor `a` to `a_moved` and create symlink `a -> a_moved`
    fs::rename(root.join("a"), root.join("a_moved")).unwrap();
    symlink(root.join("a_moved"), root.join("a")).unwrap();

    let err = stage.commit(None, &history, &key).unwrap_err();
    assert!(
        err.to_string()
            .contains("save directory changed during restore"),
        "Expected 'save directory changed during restore', got: {err}"
    );

    assert_eq!(
        fs::read(root.join("a_moved/b/save.dat")).unwrap(),
        b"original",
        "Original save must remain intact"
    );
    assert!(!history.backup_directory().exists());
}

#[test]
fn pinned_restore_rejects_in_place_modification_and_hash_mismatch_and_non_regular_target() {
    use emusync_data_local::{StagedRestore, SyncHistoryStore};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();

    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();

    // 1. Destination is a directory
    let dir_as_save = root.join("dir_as_save");
    fs::create_dir(&dir_as_save).unwrap();
    let err_dir = match StagedRestore::prepare_pinned(&root, "dir_as_save") {
        Err(e) => e,
        Ok(_) => panic!("Expected error for directory destination"),
    };
    assert!(
        err_dir.to_string().contains("save is not a regular file"),
        "Expected 'save is not a regular file', got: {err_dir}"
    );

    // 2. Hash mismatch
    let save_file = root.join("save.dat");
    fs::write(&save_file, b"initial").unwrap();
    let key = history.key(&save_file, "folder", "save.dat").unwrap();

    let stage_mismatch = StagedRestore::prepare_pinned(&root, "save.dat")
        .unwrap()
        .unwrap();
    fs::write(stage_mismatch.path(), b"downloaded").unwrap();
    let wrong_hash = "0".repeat(64);
    let err_mismatch = stage_mismatch
        .commit(Some(&wrong_hash), &history, &key)
        .unwrap_err();
    assert!(
        err_mismatch
            .to_string()
            .contains("downloaded content could not be verified"),
        "Expected 'downloaded content could not be verified', got: {err_mismatch}"
    );
    assert_eq!(fs::read(&save_file).unwrap(), b"initial");

    // 3. In-place modification between prepare_pinned and commit
    let stage_concurrent = StagedRestore::prepare_pinned(&root, "save.dat")
        .unwrap()
        .unwrap();
    fs::write(stage_concurrent.path(), b"downloaded").unwrap();
    // Simulate concurrent modification in place
    fs::write(&save_file, b"concurrent edit").unwrap();

    let err_concurrent = stage_concurrent.commit(None, &history, &key).unwrap_err();
    assert!(
        err_concurrent
            .to_string()
            .contains("save changed during download"),
        "Expected 'save changed during download', got: {err_concurrent}"
    );
    assert_eq!(fs::read(&save_file).unwrap(), b"concurrent edit");

    // Ensure no orphan .restore files remain in root
    let remaining_restore_files = fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".restore"))
        .count();
    assert_eq!(
        remaining_restore_files, 0,
        "No orphan .restore files should remain"
    );
}
