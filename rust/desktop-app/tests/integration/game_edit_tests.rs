//! The game dialog starts from the game's own save targets and cloud restores are
//! resolved with `for_restore`, as Kotlin's `EditGameDialog` / `GameEditFormState`.

use emusync_application::library::game_specific_targets;
use emusync_data_local::LocalSaveTargets;
use emusync_desktop_app::coordinator::platform::restore_targets;
use emusync_domain::library::{
    Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType,
};
use std::fs;
use tempfile::tempdir;

fn file_target(path: &str) -> SaveTarget {
    SaveTarget {
        path: path.to_string(),
        kind: SaveTargetType::File,
        cloud_path: None,
        excluded_folders: Vec::new(),
    }
}

#[test]
fn emulator_game_starts_from_its_rom_targets_not_the_emulator_wide_ones() {
    let mut emulator = Emulator::new("GBA", "/usr/bin/mgba", "/roms", vec!["gba".into()]);
    emulator.common.save_targets = vec![file_target("/emulator/wide.sav")];
    emulator.save_targets_by_rom = vec![(
        "zelda.gba".to_string(),
        vec![file_target("/saves/zelda.sav")],
    )];
    let entry = GameEntry::Emulator(emulator);

    let zelda = game_specific_targets(&LocalSaveTargets, &entry, Some("zelda.gba")).unwrap();
    assert_eq!(zelda, vec![file_target("/saves/zelda.sav")]);

    // A ROM without its own targets starts empty instead of copying the emulator's.
    let metroid = game_specific_targets(&LocalSaveTargets, &entry, Some("metroid.gba")).unwrap();
    assert!(metroid.is_empty());
    assert!(
        game_specific_targets(&LocalSaveTargets, &entry, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn native_game_starts_from_its_own_targets() {
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Celeste".to_string(),
            save_targets: vec![file_target("/saves/celeste.sav")],
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });
    assert_eq!(
        game_specific_targets(&LocalSaveTargets, &entry, None).unwrap(),
        vec![file_target("/saves/celeste.sav")]
    );
}

#[test]
fn restore_keeps_cloud_files_inside_the_destination() {
    let dir = tempdir().unwrap();
    let destination = dir.path().join("saves");
    fs::create_dir(&destination).unwrap();
    let names = vec![
        "slot1.sav".to_string(),
        "profiles/main.sav".to_string(),
        "../escape.sav".to_string(),
        "slot1.sav".to_string(),
    ];

    let targets = restore_targets(destination.to_str().unwrap(), false, &names).unwrap();
    let paths: Vec<_> = targets.iter().map(|t| t.path.clone()).collect();

    assert_eq!(
        targets.len(),
        2,
        "escaping and duplicate names are dropped: {paths:?}"
    );
    assert!(targets.iter().all(
        |t| t.path.starts_with(destination.to_str().unwrap()) && t.kind == SaveTargetType::File
    ));
    assert_eq!(targets[1].cloud_path.as_deref(), Some("profiles/main.sav"));
}

#[cfg(unix)]
#[test]
fn restore_rejects_names_reaching_through_a_symlink() {
    let dir = tempdir().unwrap();
    let destination = dir.path().join("saves");
    let outside = dir.path().join("outside");
    fs::create_dir(&destination).unwrap();
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, destination.join("link")).unwrap();

    let targets = restore_targets(
        destination.to_str().unwrap(),
        false,
        &["link/slot.sav".to_string()],
    )
    .unwrap();

    assert!(
        targets.is_empty(),
        "a symlinked folder must not redirect the restore"
    );
}

#[test]
fn whole_folder_restore_targets_the_destination_and_blank_targets_nothing() {
    let targets = restore_targets(" /saves/celeste ", true, &["0.celeste".to_string()]).unwrap();
    assert_eq!(
        targets,
        vec![SaveTarget {
            path: "/saves/celeste".to_string(),
            kind: SaveTargetType::Directory,
            cloud_path: None,
            excluded_folders: Vec::new(),
        }]
    );
    assert!(
        restore_targets("   ", false, &["0.celeste".to_string()])
            .unwrap()
            .is_empty()
    );
}
