use emusync_desktop_app::resolve_effective_targets;
use emusync_domain::library::{
    Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType,
};
use std::path::Path;

fn test_emulator_with_overrides(
    global_targets: Vec<SaveTarget>,
    overrides: Vec<(String, Vec<SaveTarget>)>,
) -> GameEntry {
    GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "SNES".to_string(),
            executable_path: "/bin/emulator".to_string(),
            arguments: vec!["{ROM}".to_string()],
            save_paths: vec![],
            save_targets: global_targets,
            group: None,
            cloud_folder: None,
            drive_file_id: None,
            steam_app_id: None,
        },
        roms_directory: "/roms".to_string(),
        extensions: vec!["sfc".to_string()],
        save_paths_by_rom: vec![],
        save_targets_by_rom: overrides,
        cover_path_by_rom: vec![],
        title_by_rom: vec![],
        steam_process_name: None,
        fullscreen_args: None,
        covers_directory: None,
    })
}

#[test]
fn rom_with_exact_filename_override_matches() {
    let global_targets = vec![SaveTarget {
        path: "/saves/global.srm".to_string(),
        kind: SaveTargetType::File,
        cloud_path: Some("global.srm".to_string()),
    }];
    let overrides = vec![(
        "Chrono.sfc".to_string(),
        vec![SaveTarget {
            path: "/saves/chrono_exact.srm".to_string(),
            kind: SaveTargetType::File,
            cloud_path: Some("chrono_exact.srm".to_string()),
        }],
    )];

    let entry = test_emulator_with_overrides(global_targets, overrides);
    let resolved = resolve_effective_targets(&entry, Some(Path::new("/roms/Chrono.sfc"))).unwrap();

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].path, "/saves/chrono_exact.srm");
}

#[test]
fn rom_with_stem_fallback_override_matches() {
    let global_targets = vec![SaveTarget {
        path: "/saves/global.srm".to_string(),
        kind: SaveTargetType::File,
        cloud_path: Some("global.srm".to_string()),
    }];
    let overrides = vec![(
        "Zelda".to_string(),
        vec![SaveTarget {
            path: "/saves/zelda_stem.srm".to_string(),
            kind: SaveTargetType::File,
            cloud_path: Some("zelda_stem.srm".to_string()),
        }],
    )];

    let entry = test_emulator_with_overrides(global_targets, overrides);
    let resolved = resolve_effective_targets(&entry, Some(Path::new("/roms/Zelda.sfc"))).unwrap();

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].path, "/saves/zelda_stem.srm");
}

#[test]
fn rom_without_override_falls_back_to_global_targets() {
    let global_targets = vec![SaveTarget {
        path: "/saves/global_fallback.srm".to_string(),
        kind: SaveTargetType::File,
        cloud_path: Some("global_fallback.srm".to_string()),
    }];
    let overrides = vec![(
        "OtherRom.sfc".to_string(),
        vec![SaveTarget {
            path: "/saves/other.srm".to_string(),
            kind: SaveTargetType::File,
            cloud_path: Some("other.srm".to_string()),
        }],
    )];

    let entry = test_emulator_with_overrides(global_targets, overrides);
    let resolved = resolve_effective_targets(&entry, Some(Path::new("/roms/Mario.sfc"))).unwrap();

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].path, "/saves/global_fallback.srm");
}

#[test]
fn native_game_uses_its_save_targets() {
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Hollow Knight".into(),
            executable_path: "/bin/hollow_knight".into(),
            arguments: vec![],
            save_paths: vec![],
            save_targets: vec![SaveTarget {
                path: "/saves/hollow_knight.dat".into(),
                kind: SaveTargetType::File,
                cloud_path: Some("hk.dat".into()),
            }],
            group: None,
            cloud_folder: None,
            drive_file_id: None,
            steam_app_id: None,
        },
        cover_path: None,
        wait_for_process: None,
    });

    let resolved = resolve_effective_targets(&entry, None).unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].path, "/saves/hollow_knight.dat");
}
