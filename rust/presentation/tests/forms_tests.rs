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
fn game_edit_restore_waits_for_resolved_paths_and_returns_the_choice() {
    use emusync_presentation::RestoreChoice;

    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Celeste".to_string(),
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });
    let game = emusync_presentation::GameItem::new("Celeste", entry, None);
    let mut form = GameEditFormState::new(&game, &[]);
    let cloud_files = vec!["0.celeste".to_string(), "settings.celeste".to_string()];

    // Without a destination nothing can be saved or previewed.
    assert!(!form.can_save(&cloud_files));
    assert_eq!(form.take_restore_preview_request(&cloud_files), None);

    // Per-file restore asks once for the paths resolved by the composition.
    form.restore_destination_folder = " /local/celeste ".to_string();
    assert_eq!(
        form.take_restore_preview_request(&cloud_files),
        Some("/local/celeste".to_string())
    );
    assert_eq!(form.take_restore_preview_request(&cloud_files), None);
    assert!(
        !form.can_save(&cloud_files),
        "unresolved paths block saving"
    );

    // A preview for a destination no longer in the form is ignored.
    form.apply_restore_preview("/elsewhere", vec!["/elsewhere/0.celeste".to_string()]);
    assert!(form.selected_restore_paths(&cloud_files).is_empty());

    // Every name rejected by the resolver: still nothing to save.
    form.apply_restore_preview("/local/celeste", Vec::new());
    assert!(!form.can_save(&cloud_files));

    form.apply_restore_preview(
        "/local/celeste",
        vec!["/local/celeste/0.celeste".to_string()],
    );
    assert!(form.can_save(&cloud_files));
    let result = form.build_result(&cloud_files).expect("restore is ready");
    assert!(
        result.targets.is_empty(),
        "targets are rebuilt by the composition"
    );
    assert_eq!(
        result.restore,
        Some(RestoreChoice {
            destination: "/local/celeste".to_string(),
            whole_folder: false,
        })
    );
    assert_eq!(result.title, Some("Celeste".to_string()));

    // The whole folder needs no resolution.
    form.restore_whole_folder = true;
    assert_eq!(
        form.selected_restore_paths(&cloud_files),
        vec!["/local/celeste"]
    );
    assert_eq!(form.take_restore_preview_request(&cloud_files), None);
    let result = form
        .build_result(&cloud_files)
        .expect("whole folder is ready");
    assert_eq!(result.restore.map(|c| c.whole_folder), Some(true));
}

#[test]
fn game_edit_starts_from_the_custom_cover_only() {
    use std::path::PathBuf;

    let mut emulator = Emulator::new("GBA", "/usr/bin/mgba", "/roms", vec!["gba".into()]);
    emulator.cover_path_by_rom = vec![("zelda.gba".to_string(), "/covers/custom.png".to_string())];
    let rom = Some(PathBuf::from("/roms/zelda.gba"));

    // Auto-detected cover only: the field stays empty so automatic detection keeps working.
    let detected = emusync_presentation::GameItem::new(
        "zelda.gba",
        GameEntry::Emulator(Emulator::new("GBA", "/usr/bin/mgba", "/roms", vec![])),
        rom.clone(),
    )
    .with_cover_path(Some("/roms/zelda.png".to_string()));
    assert_eq!(GameEditFormState::new(&detected, &[]).cover_path, "");

    let custom =
        emusync_presentation::GameItem::new("zelda.gba", GameEntry::Emulator(emulator), rom)
            .with_cover_path(Some("/covers/custom.png".to_string()));
    assert_eq!(
        GameEditFormState::new(&custom, &[]).cover_path,
        "/covers/custom.png"
    );
}

#[test]
fn cloud_folder_preview_falls_back_from_cloud_folder_to_group_to_name() {
    let mut form = EntryFormState::new(None, &[]);
    assert_eq!(form.cloud_folder_preview(), "...");

    form.name = "Eden".to_string();
    assert_eq!(form.cloud_folder_preview(), "Eden");

    form.group = "  Switch ".to_string();
    assert_eq!(form.cloud_folder_preview(), "Switch");

    form.cloud_folder = Some("Legacy Folder".to_string());
    assert_eq!(form.cloud_folder_preview(), "Legacy Folder");

    form.cloud_folder = Some("   ".to_string());
    assert_eq!(form.cloud_folder_preview(), "Switch");
}

#[test]
fn group_chips_toggle_ignoring_case_and_list_distinct_groups() {
    use emusync_presentation::forms::entry_form::existing_groups;

    let with_group = |name: &str, group: Option<&str>| {
        GameEntry::Native(NativeGame {
            common: EntryCommon {
                name: name.to_string(),
                group: group.map(str::to_string),
                ..EntryCommon::default()
            },
            ..NativeGame::default()
        })
    };
    let entries = [
        with_group("A", Some(" Switch ")),
        with_group("B", None),
        with_group("C", Some("   ")),
        with_group("D", Some("PlayStation")),
        with_group("E", Some("Switch")),
    ];
    assert_eq!(existing_groups(&entries), vec!["Switch", "PlayStation"]);

    let mut form = EntryFormState::new(None, &[]);
    form.toggle_group("Switch");
    assert_eq!(form.group, "Switch");
    form.group = "SWITCH".to_string();
    form.toggle_group("Switch");
    assert_eq!(
        form.group, "",
        "selecting the current group again clears it"
    );
}

#[test]
fn cloud_path_validation_rejects_parent_segments_only_when_filled() {
    use emusync_presentation::forms::save_targets::cloud_path_is_valid;

    assert!(cloud_path_is_valid(""));
    assert!(cloud_path_is_valid("   "));
    assert!(cloud_path_is_valid("saves/slot1.sav"));
    assert!(!cloud_path_is_valid("../outside.sav"));
    assert!(!cloud_path_is_valid("saves/../../x"));
}
