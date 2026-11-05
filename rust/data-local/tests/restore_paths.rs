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
fn target_resolution_preserves_remote_names_and_skips_invalid_entries() {
    use emusync_data_local::LocalSaveTargets;
    use emusync_domain::library::{SaveTargetAccess, SaveTargetType};
    let fixture = Fixture::new();
    let destination = fixture.0.to_str().unwrap();
    let targets = LocalSaveTargets
        .for_restore(
            destination,
            &[
                "slot/save.dat".into(),
                "slot\\save.dat".into(),
                "../outside".into(),
            ],
        )
        .unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].path, targets[1].path);
    assert_eq!(targets[0].cloud_path.as_deref(), Some("slot/save.dat"));
    assert_eq!(targets[1].cloud_path.as_deref(), Some("slot\\save.dat"));
    assert_eq!(targets[0].kind, SaveTargetType::File);
    assert!(
        LocalSaveTargets
            .for_restore(" ", &["save".into()])
            .unwrap()
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn walking_local_targets_follows_directory_links_but_rejects_cycles() {
    use emusync_data_local::DesktopSaveAccess;
    use emusync_domain::sync::{LocalSaveAccess, LocalSavePath};
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let root = fixture.0.join("root");
    let external = fixture.0.join("external");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&external).unwrap();
    fs::write(external.join("save"), b"save").unwrap();
    symlink(&external, root.join("linked")).unwrap();
    let access = DesktopSaveAccess;
    let path = access.path(root.to_str().unwrap());
    let listed = path.walk_top_down().unwrap();
    assert!(listed.iter().any(|p| p.path().ends_with("linked/save")));
    symlink(&root, external.join("cycle")).unwrap();
    assert!(path.walk_top_down().is_err());
}

#[test]
fn rom_targets_prefer_exact_then_stem_then_legacy_and_resolve_types() {
    use emusync_data_local::LocalSaveTargets;
    use emusync_domain::library::{Emulator, SaveTarget, SaveTargetAccess, SaveTargetType};
    let fixture = Fixture::new();
    let dir = fixture.0.join("saves");
    fs::create_dir(&dir).unwrap();
    let typed = SaveTarget {
        path: dir.to_str().unwrap().into(),
        kind: SaveTargetType::Unknown,
        cloud_path: Some("  ".into()),
    };
    let emulator = Emulator {
        save_targets_by_rom: vec![
            ("game.iso".into(), vec![typed]),
            (
                "game".into(),
                vec![SaveTarget {
                    path: "stem".into(),
                    kind: SaveTargetType::File,
                    cloud_path: None,
                }],
            ),
        ],
        save_paths_by_rom: vec![("other".into(), vec!["missing".into()])],
        ..Emulator::default()
    };
    let exact = LocalSaveTargets.for_rom(&emulator, "game.iso").unwrap();
    assert_eq!(exact[0].kind, SaveTargetType::Directory);
    assert!(exact[0].cloud_path.is_none());
    assert_eq!(
        LocalSaveTargets.for_rom(&emulator, "game.bin").unwrap()[0].path,
        "stem"
    );
    let legacy = LocalSaveTargets.for_rom(&emulator, "other.iso").unwrap();
    assert_eq!(legacy[0].path, "missing");
    assert_eq!(legacy[0].kind, SaveTargetType::Unknown);
}
