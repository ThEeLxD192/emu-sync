mod common;

use common::*;
use emusync_application::SaveTargetResolver;
use emusync_domain::library::{
    Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType,
};
use emusync_domain::sync::{LocalSavePath, SyncGame, sanitize_cloud_folder_name};

#[test]
fn target_resolver_resolves_cloud_path_segments() {
    let fs = MockFs::new();
    let resolver = SaveTargetResolver::new(fs);

    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Zelda".into(),
            cloud_folder: Some("CustomZeldaFolder".into()),
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    });

    let sync_game = SyncGame {
        entry,
        name: "Zelda".into(),
        effective_title: "The Legend of Zelda".into(),
        rom_path: None,
        save_targets: Vec::new(),
    };

    let segments = resolver.get_cloud_path_segments_game(&sync_game).unwrap();
    assert_eq!(segments, vec!["CustomZeldaFolder"]);
}

#[test]
fn target_resolver_get_save_targets_for_native_and_emulator() {
    let fs = MockFs::new();
    let resolver = SaveTargetResolver::new(fs);

    // 1. NativeGame
    let native = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "NativeGame".into(),
            save_targets: vec![SaveTarget {
                path: "/saves/native.sav".into(),
                kind: SaveTargetType::File,
                cloud_path: None,
                excluded_folders: Vec::new(),
            }],
            ..Default::default()
        },
        ..Default::default()
    });

    let targets = resolver.get_save_targets(&native).unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].display_name, "NativeGame");
    assert_eq!(targets[0].file.path(), "/saves/native.sav");
    assert_eq!(targets[0].cloud_path_segments, vec!["NativeGame"]);

    // 2. Emulator with global and per-ROM targets, including a duplicate
    let emu = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "RetroArch".into(),
            cloud_folder: Some("CloudEmu".into()),
            save_targets: vec![SaveTarget {
                path: "/saves/global.sav".into(),
                kind: SaveTargetType::File,
                cloud_path: None,
                excluded_folders: Vec::new(),
            }],
            ..Default::default()
        },
        save_targets_by_rom: vec![
            (
                "game1.sfc".into(),
                vec![SaveTarget {
                    path: "/saves/game1.srm".into(),
                    kind: SaveTargetType::File,
                    cloud_path: None,
                    excluded_folders: Vec::new(),
                }],
            ),
            (
                "game2.sfc".into(),
                vec![
                    // duplicate of global.sav
                    SaveTarget {
                        path: "/saves/global.sav".into(),
                        kind: SaveTargetType::File,
                        cloud_path: None,
                        excluded_folders: Vec::new(),
                    },
                ],
            ),
        ],
        ..Default::default()
    });

    let emu_targets = resolver.get_save_targets(&emu).unwrap();
    // /saves/global.sav should only appear once due to deduplication
    assert_eq!(emu_targets.len(), 2);
    assert!(
        emu_targets
            .iter()
            .any(|t| t.file.path() == "/saves/global.sav")
    );
    assert!(
        emu_targets
            .iter()
            .any(|t| t.file.path() == "/saves/game1.srm")
    );
}

#[test]
fn resolved_save_target_configured_preserves_target_properties() {
    let fs = MockFs::new();
    let resolver = SaveTargetResolver::new(fs);
    let native = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Game".into(),
            save_targets: vec![SaveTarget {
                path: "/saves/game.sav".into(),
                kind: SaveTargetType::File,
                cloud_path: Some("custom.sav".into()),
                excluded_folders: Vec::new(),
            }],
            ..Default::default()
        },
        ..Default::default()
    });

    let targets = resolver.get_save_targets(&native).unwrap();
    assert_eq!(targets.len(), 1);
    let configured = targets[0].configured();
    assert_eq!(configured.path, "/saves/game.sav");
    assert_eq!(configured.kind, SaveTargetType::File);
    assert_eq!(configured.cloud_path, Some("custom.sav".into()));
}

// --- Adversarial Target Resolver Tests ---

#[test]
fn emulator_save_targets_merges_legacy_save_paths_by_rom_and_deduplicates_by_normalized_path_and_cloud_path()
 {
    let fs = MockFs::new();
    let resolver = SaveTargetResolver::new(fs);

    let emu_inner = Emulator {
        common: EntryCommon {
            name: "MultiEmu".into(),
            cloud_folder: Some("CloudFolder".into()),
            save_targets: vec![],
            ..Default::default()
        },
        save_paths_by_rom: vec![
            ("rom_a.sfc".into(), vec!["/saves/legacy_a.srm".into()]),
            (
                "rom_dup_legacy.sfc".into(),
                vec!["/saves/shared_duplicate.srm".into()],
            ),
        ],
        save_targets_by_rom: vec![
            (
                "rom_b.sfc".into(),
                vec![SaveTarget {
                    path: "/saves/special_b.srm".into(),
                    kind: SaveTargetType::File,
                    cloud_path: None,
                    excluded_folders: Vec::new(),
                }],
            ),
            (
                "rom_c.sfc".into(),
                vec![SaveTarget {
                    path: "/saves/shared_duplicate.srm".into(),
                    kind: SaveTargetType::File,
                    cloud_path: None,
                    excluded_folders: Vec::new(),
                }],
            ),
            (
                "rom_d.sfc".into(),
                vec![SaveTarget {
                    path: "/saves/shared_duplicate.srm".into(),
                    kind: SaveTargetType::File,
                    cloud_path: Some("custom.srm".into()),
                    excluded_folders: Vec::new(),
                }],
            ),
        ],
        title_by_rom: vec![("rom_b.sfc".into(), "Title: Part 1 / 2".into())],
        ..Default::default()
    };
    let emu = GameEntry::Emulator(emu_inner.clone());

    let targets = resolver.get_save_targets(&emu).unwrap();

    assert_eq!(
        targets.len(),
        4,
        "Expected exactly 4 targets after deduplicating rom_dup_legacy"
    );

    // 1. rom_a (from legacy save_paths_by_rom)
    let expected_rom_a_title = emu_inner.effective_title("rom_a.sfc", "rom_a.sfc");
    let expected_rom_a_cloud = sanitize_cloud_folder_name(&expected_rom_a_title);
    assert_eq!(targets[0].file.path(), "/saves/legacy_a.srm");
    assert_eq!(
        targets[0].cloud_path_segments,
        vec!["CloudFolder".to_string(), expected_rom_a_cloud]
    );
    assert_eq!(targets[0].cloud_path, None);

    // 2. rom_b (with special characters sanitized)
    let expected_sanitized = sanitize_cloud_folder_name("Title: Part 1 / 2");
    assert_eq!(targets[1].file.path(), "/saves/special_b.srm");
    assert_eq!(targets[1].display_name, "Title: Part 1 / 2");
    assert_eq!(
        targets[1].cloud_path_segments,
        vec!["CloudFolder".to_string(), expected_sanitized]
    );
    assert_eq!(targets[1].cloud_path, None);

    // 3. rom_c
    assert_eq!(targets[2].file.path(), "/saves/shared_duplicate.srm");
    assert_eq!(targets[2].cloud_path, None);

    // 4. rom_d (same abs path, but different cloud_path -> preserved!)
    assert_eq!(targets[3].file.path(), "/saves/shared_duplicate.srm");
    assert_eq!(targets[3].cloud_path, Some("custom.srm".into()));
}
