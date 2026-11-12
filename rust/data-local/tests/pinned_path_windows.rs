#![cfg(windows)]
use emusync_data_local::{PinnedRemoteDestination, StagedRestore, SyncHistoryStore};
use std::{fs, io::Write, path::Path, process::Command};

/// A directory junction needs no privileges, unlike a symbolic link.
fn junction(link: &Path, target: &Path) {
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
}

#[test]
fn a_pinned_directory_chain_cannot_be_renamed_or_removed_while_it_is_held() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir_all(root.join("a/b")).unwrap();
    let pinned = PinnedRemoteDestination::open(&root, "a/b/save.dat")
        .unwrap()
        .unwrap();
    // Swapping a directory for a junction needs it to be moved away first.
    assert!(fs::rename(root.join("a/b"), root.join("a/moved")).is_err());
    assert!(fs::rename(root.join("a"), root.join("moved")).is_err());
    assert!(fs::remove_dir(root.join("a/b")).is_err());
    pinned
        .create_new()
        .unwrap()
        .write_all(b"synthetic")
        .unwrap();
    assert_eq!(fs::read(root.join("a/b/save.dat")).unwrap(), b"synthetic");
    drop(pinned);
    fs::rename(root.join("a/b"), root.join("a/moved")).unwrap();
}

#[test]
fn junctions_below_the_root_are_refused_and_nothing_is_written_through_them() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("save"), b"untouched").unwrap();
    junction(&root.join("link"), &outside);
    assert!(PinnedRemoteDestination::open(&root, "link/save").is_err());
    assert!(PinnedRemoteDestination::open_or_create(&root, "link/new/save").is_err());
    assert!(StagedRestore::prepare_pinned(&root, "link/new/save").is_err());
    assert_eq!(fs::read(outside.join("save")).unwrap(), b"untouched");
    assert!(!outside.join("new").exists());
}

#[test]
fn names_windows_cannot_create_are_refused_without_touching_the_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    for name in [
        "CON",
        "slot/aux.txt",
        "nul",
        "save.dat:stream",
        "slot/trailing.",
        "a<b",
        "../save",
    ] {
        assert!(
            PinnedRemoteDestination::open_or_create(&root, name)
                .unwrap()
                .is_none(),
            "{name}"
        );
    }
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
}

#[test]
fn create_new_never_replaces_an_existing_save() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("existing"), b"original").unwrap();
    let existing = PinnedRemoteDestination::open(&root, "existing")
        .unwrap()
        .unwrap();
    assert!(existing.create_new().is_err());
    assert_eq!(fs::read(root.join("existing")).unwrap(), b"original");
}

#[test]
fn pinned_restore_backs_up_the_original_and_replaces_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let logical = root.join("slot").join("save.dat");
    fs::create_dir(root.join("slot")).unwrap();
    fs::write(&logical, b"original").unwrap();
    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();
    let key = history.key(&logical, "folder", "slot/save.dat").unwrap();
    let stage = StagedRestore::prepare_pinned(&root, "slot/save.dat")
        .unwrap()
        .unwrap();
    fs::write(stage.path(), b"restored").unwrap();
    stage.commit(None, &history, &key).unwrap();
    assert_eq!(fs::read(&logical).unwrap(), b"restored");
    // SHA-256 of "original", an independent literal.
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
}

#[test]
fn pinned_restore_creates_missing_directories_for_a_new_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
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
    assert!(!history.backup_directory().exists());
}

#[test]
fn pinned_restore_rejects_a_file_created_after_staging() {
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
fn pinned_restore_rejects_an_original_modified_after_staging() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("save"), b"original").unwrap();
    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();
    let key = history.key(&root.join("save"), "folder", "save").unwrap();
    let stage = StagedRestore::prepare_pinned(&root, "save")
        .unwrap()
        .unwrap();
    fs::write(stage.path(), b"remote").unwrap();
    fs::write(root.join("save"), b"edited meanwhile").unwrap();
    assert!(stage.commit(None, &history, &key).is_err());
    assert_eq!(fs::read(root.join("save")).unwrap(), b"edited meanwhile");
}

/// Directory downloads publish the finished `.part` file over the staged path, which
/// Windows refuses while this process holds the staged file open.
#[test]
fn a_download_can_replace_the_pinned_staged_file_before_commit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let history = SyncHistoryStore::new(&temp.path().join("config.json")).unwrap();
    let key = history
        .key(&root.join("slot/save.dat"), "folder", "slot/save.dat")
        .unwrap();
    let stage = StagedRestore::prepare_pinned(&root, "slot/save.dat")
        .unwrap()
        .unwrap();
    let (mut part, part_path) = tempfile::Builder::new()
        .prefix(".emusync-download-")
        .suffix(".part")
        .tempfile_in(stage.path().parent().unwrap())
        .unwrap()
        .into_parts();
    part.write_all(b"remote").unwrap();
    drop(part);
    part_path.persist(stage.path()).unwrap();
    stage.commit(None, &history, &key).unwrap();
    assert_eq!(fs::read(root.join("slot/save.dat")).unwrap(), b"remote");
}
