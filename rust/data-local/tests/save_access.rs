use emusync_data_local::DesktopSaveAccess;
use emusync_domain::{
    library::{EntryCommon, GameEntry, NativeGame, SaveTargetType},
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
