use emusync_data_local::DesktopSaveAccess;
use emusync_domain::{
    library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType},
    sync::{LocalSaveAccess, LocalSavePath},
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "emusync-save-access-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn desktop_save_path_contracts() {
    let fixture = Fixture::new();
    let save_file = fixture.0.join("folder").join("game.sav");
    fs::create_dir_all(save_file.parent().unwrap()).unwrap();
    fs::write(&save_file, b"content").unwrap();

    let access = DesktopSaveAccess;
    let save_path = access.path(save_file.to_str().unwrap());

    assert_eq!(save_path.name(), "game.sav");
    assert!(save_path.exists().unwrap());
    assert!(save_path.is_file().unwrap());
    assert!(!save_path.is_directory().unwrap());
    assert!(save_path.last_modified_ms().unwrap() > 0);

    let parent_path = access.path(save_file.parent().unwrap().to_str().unwrap());
    assert!(parent_path.is_directory().unwrap());
    assert!(!parent_path.is_file().unwrap());

    assert_eq!(save_path.relative_to(&parent_path).unwrap(), "game.sav");

    let ancestors = save_path.ancestors().unwrap();
    assert!(!ancestors.is_empty());

    let resolved = parent_path.resolve("game.sav").unwrap();
    assert_eq!(resolved.path(), save_path.path());

    let resolved_remote = parent_path.resolve_remote("sub/game.sav").unwrap();
    assert!(resolved_remote.is_some());
    assert!(
        parent_path
            .resolve_remote("../outside.sav")
            .unwrap()
            .is_none()
    );

    let non_existent = access.path(fixture.0.join("missing.sav").to_str().unwrap());
    assert!(!non_existent.exists().unwrap());
    assert!(!non_existent.is_file().unwrap());
    assert_eq!(non_existent.last_modified_ms().unwrap(), 0);
}

#[test]
fn desktop_save_access_effective_targets() {
    let fixture = Fixture::new();
    let save_file = fixture.0.join("native.sav");
    fs::write(&save_file, b"save").unwrap();

    let access = DesktopSaveAccess;
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Native".into(),
            save_paths: vec![save_file.to_str().unwrap().into()],
            ..Default::default()
        },
        ..Default::default()
    });

    let targets = access.effective_targets(&entry).unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, save_file.to_str().unwrap());
    assert_eq!(targets[0].kind, SaveTargetType::File);
}

#[cfg(unix)]
#[test]
fn walking_local_targets_follows_directory_links_but_rejects_cycles() {
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
fn desktop_save_path_handles_pre_epoch_timestamps_normalization_and_rom_targets() {
    let fixture = Fixture::new();
    let retro = fixture.0.join("retro.sav");
    fs::write(&retro, b"retro save").unwrap();

    // Set mtime to 15 seconds before UNIX_EPOCH: -15s = -15_000ms
    filetime::set_file_mtime(&retro, filetime::FileTime::from_unix_time(-15, 0)).unwrap();

    let access = DesktopSaveAccess;
    let retro_str = retro.to_str().unwrap();
    let retro_path = access.path(retro_str);

    // 1. Verify pre-epoch timestamp is returned as negative ms
    assert_eq!(retro_path.last_modified_ms().unwrap(), -15_000);

    // 2. Verify normalized_path resolves . and ..
    let sub_dir = fixture.0.join("sub");
    fs::create_dir(&sub_dir).unwrap();
    let unnormalized = fixture.0.join("sub/../retro.sav");
    let unnormalized_path = access.path(unnormalized.to_str().unwrap());
    assert_eq!(
        unnormalized_path.normalized_path().unwrap(),
        retro_path.normalized_path().unwrap()
    );

    // 3. Verify relative_to against non-ancestor directory fails with Err
    let other_root = fixture.0.join("other_root");
    fs::create_dir(&other_root).unwrap();
    let other_path = access.path(other_root.to_str().unwrap());
    assert!(retro_path.relative_to(&other_path).is_err());

    // 4. Verify effective_targets_for_rom delegates to LocalSaveTargets::for_rom
    let emulator = Emulator {
        save_targets_by_rom: vec![(
            "game.iso".into(),
            vec![SaveTarget {
                path: retro_str.into(),
                kind: SaveTargetType::Unknown,
                cloud_path: None,
            }],
        )],
        ..Emulator::default()
    };
    let targets = access
        .effective_targets_for_rom(&emulator, "game.iso")
        .unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, retro_str);
    assert_eq!(targets[0].kind, SaveTargetType::File);
}
