//! Unit and integration tests for presentation form states and validations.

use emusync_domain::library::{
    Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType,
};
use emusync_presentation::{EntryFormState, EntryType, GameEditFormState, SaveTargetFormState};

#[test]
fn save_targets_form_manages_rows_and_toggles_types() {
    let mut form = SaveTargetFormState::default();
    assert_eq!(form.rows.len(), 1);
    assert_eq!(form.rows[0].target_type, SaveTargetType::File);

    form.toggle_type(0);
    assert_eq!(form.rows[0].target_type, SaveTargetType::Directory);

    form.add_row();
    assert_eq!(form.rows.len(), 2);

    form.update_path(0, "saves/game.dat".to_string());
    form.update_cloud_path(0, "custom_cloud.dat".to_string());

    form.remove_row(1);
    assert_eq!(form.rows.len(), 1);

    // Removing last row resets to a default row
    form.remove_row(0);
    assert_eq!(form.rows.len(), 1);
    assert!(form.rows[0].path.is_empty());
}

#[test]
fn save_targets_form_validates_cloud_paths() {
    let mut form = SaveTargetFormState::default();

    // Valid relative path
    form.update_path(0, "local/path".to_string());
    form.update_cloud_path(0, "remote/save.dat".to_string());
    assert!(form.cloud_paths_are_valid());

    // Invalid traversal path
    form.update_cloud_path(0, "../save.dat".to_string());
    assert!(!form.cloud_paths_are_valid());

    // Invalid absolute path
    form.update_cloud_path(0, "/etc/save.dat".to_string());
    assert!(!form.cloud_paths_are_valid());

    // Blank cloud path is valid (inherits local filename)
    form.update_cloud_path(0, "".to_string());
    assert!(form.cloud_paths_are_valid());
}

#[test]
fn entry_form_applies_type_defaults() {
    let mut form = EntryFormState::new(None, &[]);
    assert_eq!(form.entry_type, EntryType::Emulator);
    assert_eq!(form.arguments, "{ROM}");

    form.entry_type = EntryType::Native;
    form.apply_type_defaults();
    assert_eq!(form.arguments, "");

    form.entry_type = EntryType::Emulator;
    form.apply_type_defaults();
    assert_eq!(form.arguments, "{ROM}");
}

#[test]
fn entry_form_validates_required_fields_and_arguments() {
    let mut form = EntryFormState::new(None, &[]);
    // Blank name & exec
    assert!(form.build_entry().is_err());

    form.name = "PlayStation 2".to_string();
    form.executable_path = "/usr/bin/pcsx2".to_string();
    form.arguments = "invalid \"unclosed".to_string();
    assert!(form.build_entry().is_err());

    form.arguments = "-f -- \"game with space\"".to_string();
    // Missing roms dir & extensions for emulator
    assert!(form.build_entry().is_err());

    form.roms_directory = "/roms/ps2".to_string();
    form.extensions = "iso, chd, bin".to_string();

    let entry = form.build_entry().expect("valid emulator entry");
    match entry {
        GameEntry::Emulator(emu) => {
            assert_eq!(emu.common.name, "PlayStation 2");
            assert_eq!(emu.common.executable_path, "/usr/bin/pcsx2");
            assert_eq!(emu.common.arguments, vec!["-f", "--", "game with space"]);
            assert_eq!(emu.roms_directory, "/roms/ps2");
            assert_eq!(emu.extensions, vec!["iso", "chd", "bin"]);
        }
        GameEntry::Native(_) => panic!("expected emulator"),
    }
}

#[test]
fn entry_form_preserves_metadata_when_editing() {
    let initial = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "RetroArch".to_string(),
            executable_path: "retroarch".to_string(),
            steam_app_id: Some(123456),
            drive_file_id: Some("drive-id-99".to_string()),
            cloud_folder: Some("cloud_folder_name".to_string()),
            ..EntryCommon::default()
        },
        roms_directory: "/roms".to_string(),
        extensions: vec!["sfc".to_string()],
        steam_process_name: Some("retroarch_bin".to_string()),
        title_by_rom: vec![("game.sfc".to_string(), "Overridden Title".to_string())],
        cover_path_by_rom: vec![("game.sfc".to_string(), "/covers/game.png".to_string())],
        ..Emulator::default()
    });

    let targets = vec![SaveTarget {
        path: "/saves/snes".to_string(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
    }];

    let mut form = EntryFormState::new(Some(initial), &targets);
    assert_eq!(form.name, "RetroArch");
    form.name = "RetroArch Updated".to_string();

    let updated = form.build_entry().expect("valid edit");
    match updated {
        GameEntry::Emulator(emu) => {
            assert_eq!(emu.common.name, "RetroArch Updated");
            assert_eq!(emu.common.steam_app_id, Some(123456));
            assert_eq!(emu.common.drive_file_id, Some("drive-id-99".to_string()));
            assert_eq!(
                emu.common.cloud_folder,
                Some("cloud_folder_name".to_string())
            );
            assert_eq!(emu.steam_process_name, Some("retroarch_bin".to_string()));
            assert_eq!(
                emu.title_by_rom,
                vec![("game.sfc".to_string(), "Overridden Title".to_string())]
            );
            assert_eq!(
                emu.cover_path_by_rom,
                vec![("game.sfc".to_string(), "/covers/game.png".to_string())]
            );
        }
        _ => panic!("expected emulator"),
    }
}

#[test]
fn entry_form_requires_targets_for_native_game() {
    let mut form = EntryFormState::new(None, &[]);
    form.entry_type = EntryType::Native;
    form.name = "Hollow Knight".to_string();
    form.executable_path = "/games/hollow_knight".to_string();

    // No save targets -> error
    assert!(form.build_entry().is_err());

    form.saves.update_path(0, "/saves/hk".to_string());
    let entry = form.build_entry().expect("valid native game");
    assert!(matches!(entry, GameEntry::Native(_)));
}

#[test]
fn game_edit_form_computes_restore_paths_and_result() {
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Celeste".to_string(),
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });

    let game = emusync_presentation::GameItem::new("Celeste", entry, None);
    let mut form = GameEditFormState::new(&game, &[]);

    // Cloud restore paths computation
    let cloud_files = vec!["0.celeste".to_string(), "settings.celeste".to_string()];
    form.restore_destination_folder = "/local/celeste/saves".to_string();

    let paths = form.restore_paths(&cloud_files);
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0], "/local/celeste/saves/0.celeste");
    assert_eq!(paths[1], "/local/celeste/saves/settings.celeste");

    // Whole folder restore toggle
    form.restore_whole_folder = true;
    let whole_folder_paths = form.restore_paths(&cloud_files);
    assert_eq!(whole_folder_paths, vec!["/local/celeste/saves"]);

    // Build result
    let (title, targets, cover) = form
        .build_result(&cloud_files)
        .expect("valid restore result");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, "/local/celeste/saves");
    assert_eq!(targets[0].kind, SaveTargetType::Directory);
    assert_eq!(title, Some("Celeste".to_string()));
    assert_eq!(cover, None);
}
