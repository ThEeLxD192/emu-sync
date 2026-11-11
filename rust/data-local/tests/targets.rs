use emusync_data_local::LocalSaveTargets;
use emusync_domain::library::{
    Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetAccess, SaveTargetType,
};
use std::fs;

#[test]
fn target_resolution_preserves_remote_names_and_skips_invalid_entries() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().to_str().unwrap();
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

#[test]
fn rom_targets_prefer_exact_then_stem_then_legacy_and_resolve_types() {
    let dir = tempfile::tempdir().unwrap();
    let saves_dir = dir.path().join("saves");
    fs::create_dir(&saves_dir).unwrap();
    let typed = SaveTarget {
        path: saves_dir.to_str().unwrap().into(),
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

#[test]
fn rom_targets_with_blank_paths_do_not_block_stem_fallback() {
    let emulator = Emulator {
        save_paths_by_rom: vec![
            ("game.iso".into(), vec!["   ".into(), "".into()]),
            ("game".into(), vec!["/saves/fallback.srm".into()]),
        ],
        save_targets_by_rom: vec![
            ("other.iso".into(), vec![]),
            (
                "other".into(),
                vec![SaveTarget {
                    path: "/saves/stem_target.srm".into(),
                    kind: SaveTargetType::File,
                    cloud_path: None,
                }],
            ),
        ],
        ..Emulator::default()
    };

    let rom_result = LocalSaveTargets.for_rom(&emulator, "game.iso").unwrap();
    assert_eq!(rom_result.len(), 1);
    assert_eq!(rom_result[0].path, "/saves/fallback.srm");

    let target_result = LocalSaveTargets.for_rom(&emulator, "other.iso").unwrap();
    assert_eq!(target_result.len(), 1);
    assert_eq!(target_result[0].path, "/saves/stem_target.srm");
}

#[test]
fn type_of_distinguishes_file_directory_missing_and_blank_paths() {
    let dir = tempfile::tempdir().unwrap();
    let save_file = dir.path().join("save.bin");
    fs::write(&save_file, b"content").unwrap();
    let saves_dir = dir.path().join("saves_dir");
    fs::create_dir(&saves_dir).unwrap();
    let missing = dir.path().join("missing.bin");

    assert_eq!(LocalSaveTargets.type_of("").unwrap(), None);
    assert_eq!(LocalSaveTargets.type_of("   ").unwrap(), None);
    assert_eq!(
        LocalSaveTargets.type_of(missing.to_str().unwrap()).unwrap(),
        None
    );
    assert_eq!(
        LocalSaveTargets
            .type_of(save_file.to_str().unwrap())
            .unwrap(),
        Some(SaveTargetType::File)
    );
    assert_eq!(
        LocalSaveTargets
            .type_of(saves_dir.to_str().unwrap())
            .unwrap(),
        Some(SaveTargetType::Directory)
    );
}

#[test]
fn for_entry_reconciles_stale_target_kinds_with_disk_and_strips_blank_cloud_path() {
    let dir = tempfile::tempdir().unwrap();
    let actual_dir = dir.path().join("actual_dir");
    fs::create_dir(&actual_dir).unwrap();
    let actual_file = dir.path().join("actual_file");
    fs::write(&actual_file, b"file bytes").unwrap();

    let entry_with_targets = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Entry1".into(),
            save_targets: vec![
                SaveTarget {
                    path: actual_dir.to_str().unwrap().into(),
                    kind: SaveTargetType::File, // Contradicts disk (is dir)
                    cloud_path: Some("   ".into()),
                },
                SaveTarget {
                    path: actual_file.to_str().unwrap().into(),
                    kind: SaveTargetType::Directory, // Contradicts disk (is file)
                    cloud_path: Some("slot/file.sav".into()),
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    });

    let targets1 = LocalSaveTargets.for_entry(&entry_with_targets).unwrap();
    assert_eq!(targets1.len(), 2);
    assert_eq!(targets1[0].kind, SaveTargetType::Directory);
    assert_eq!(targets1[0].cloud_path, None);
    assert_eq!(targets1[1].kind, SaveTargetType::File);
    assert_eq!(targets1[1].cloud_path.as_deref(), Some("slot/file.sav"));

    let entry_with_paths = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Entry2".into(),
            save_targets: vec![],
            save_paths: vec![
                "".into(),
                "   ".into(),
                actual_dir.to_str().unwrap().into(),
                actual_file.to_str().unwrap().into(),
            ],
            ..Default::default()
        },
        ..Default::default()
    });

    let targets2 = LocalSaveTargets.for_entry(&entry_with_paths).unwrap();
    assert_eq!(targets2.len(), 2);
    assert_eq!(targets2[0].kind, SaveTargetType::Directory);
    assert_eq!(targets2[0].path, actual_dir.to_str().unwrap());
    assert_eq!(targets2[1].kind, SaveTargetType::File);
    assert_eq!(targets2[1].path, actual_file.to_str().unwrap());
}

#[test]
fn for_restore_deduplicates_identical_remote_names_preserving_first_occurrence_order() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().to_str().unwrap();
    let names = vec![
        "b/save.dat".to_string(),
        "a/save.dat".to_string(),
        "b/save.dat".to_string(),
        "../escape.dat".to_string(),
        "a/save.dat".to_string(),
    ];

    let targets = LocalSaveTargets.for_restore(dest, &names).unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].cloud_path.as_deref(), Some("b/save.dat"));
    assert_eq!(targets[1].cloud_path.as_deref(), Some("a/save.dat"));
}
