use emusync_data_local::resolve_remote_file;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "emusync-r3-paths-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn resolves_nested_names_and_rejects_untrusted_paths() {
    let fixture = Fixture::new();
    let root = fixture.0.join("unused").join("..");
    for name in ["slot/one/save.dat", "slot\\one\\save.dat"] {
        assert_eq!(
            resolve_remote_file(&root, name).unwrap(),
            Some(fixture.0.join("slot/one/save.dat"))
        );
    }
    for name in [
        "../outside.sav",
        "..\\outside.sav",
        "folder/../../outside.sav",
        "/tmp/outside.sav",
        "\\\\server\\share\\outside.sav",
        "C:\\outside.sav",
        "C:outside.sav",
        "slot\0/save.sav",
        "folder//save.sav",
        "folder\\\\save.sav",
        "./save.sav",
        "folder/./save.sav",
    ] {
        assert_eq!(resolve_remote_file(&root, name).unwrap(), None, "{name:?}");
    }
}

#[cfg(unix)]
#[test]
fn rejects_intermediate_and_final_symlinks_without_touching_saves() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let root = fixture.0.join("selected");
    let outside = fixture.0.join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("save.dat"), b"keep me").unwrap();
    symlink(&outside, root.join("slot")).unwrap();
    symlink(outside.join("save.dat"), root.join("save.dat")).unwrap();
    assert_eq!(resolve_remote_file(&root, "slot/save.dat").unwrap(), None);
    assert_eq!(resolve_remote_file(&root, "save.dat").unwrap(), None);
    assert_eq!(fs::read(outside.join("save.dat")).unwrap(), b"keep me");
}

#[test]
fn rejects_intermediate_regular_file_and_resolves_under_nonexistent_root_unless_symlinked_outside()
{
    let fixture = Fixture::new();
    let root = fixture.0.join("root");
    fs::create_dir(&root).unwrap();

    let slot_file = root.join("slot_file");
    fs::write(&slot_file, b"i am a regular file").unwrap();

    let missing_root = fixture.0.join("nonexistent/saves");

    // Case 1: intermediate component is a regular file
    assert_eq!(
        resolve_remote_file(&root, "slot_file/save.dat").unwrap(),
        None
    );

    // Case 2: grandfather/ancestor is a regular file (triggers NotADirectory in deeper lookups)
    assert_eq!(
        resolve_remote_file(&root, "slot_file/sub/save.dat").unwrap(),
        None
    );

    // Case 3: root does not yet exist on disk
    let resolved = resolve_remote_file(&missing_root, "slot/save.dat").unwrap();
    assert_eq!(
        resolved,
        Some(crate::storage_absolute(&missing_root.join("slot/save.dat")))
    );
}

fn storage_absolute(path: &std::path::Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap().join(path)
    }
}

/// Drive accepts names Windows cannot create as plain files; they must never resolve.
#[cfg(windows)]
#[test]
fn windows_refuses_cloud_names_that_would_open_devices_or_streams() {
    let fixture = Fixture::new();
    for name in [
        "CON",
        "nul.txt",
        "slot/COM1",
        "save.dat:stream",
        "trailing.",
        "a<b",
    ] {
        assert_eq!(
            resolve_remote_file(&fixture.0, name).unwrap(),
            None,
            "{name}"
        );
    }
    assert!(
        resolve_remote_file(&fixture.0, "slot/console.sav")
            .unwrap()
            .is_some()
    );
}
