//! Entry form defaults, validation and metadata preservation.

use emusync_domain::library::{
    Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType,
};
use emusync_presentation::{EntryFormState, EntryType};

fn valid_emulator_form() -> EntryFormState {
    let mut form = EntryFormState::new(None, &[]);
    form.name = "GBA".to_string();
    form.executable_path = "/usr/bin/mgba".to_string();
    form.roms_directory = "/roms".to_string();
    form.extensions = "gba".to_string();
    form
}

fn dir_target(path: &str) -> SaveTarget {
    SaveTarget {
        path: path.to_string(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
        excluded_folders: Vec::new(),
    }
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
        excluded_folders: Vec::new(),
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
fn existing_groups_keep_case_variants_like_kotlin_distinct() {
    use emusync_presentation::forms::entry_form::existing_groups;

    let entry = |group: &str| {
        GameEntry::Native(NativeGame {
            common: EntryCommon {
                name: format!("game-{group}"),
                group: Some(group.to_string()),
                ..EntryCommon::default()
            },
            ..NativeGame::default()
        })
    };
    let entries = [
        entry("Switch"),
        entry(" Switch "),
        entry("switch"),
        entry(" "),
    ];
    assert_eq!(existing_groups(&entries), vec!["Switch", "switch"]);
}

#[test]
fn new_entry_scopes_multiple_directories_but_edits_keep_targets() {
    let mut form = valid_emulator_form();
    form.saves.update_path(0, "/d1".to_string());
    form.saves.update_type(0, SaveTargetType::Directory);
    form.saves.add_row();
    form.saves.update_path(1, "/d2".to_string());
    form.saves.update_type(1, SaveTargetType::Directory);

    let created = form.build_entry().expect("valid new entry");
    let targets = &created.common().save_targets;
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].cloud_path.as_deref(), Some("d1"));
    assert_eq!(targets[1].cloud_path.as_deref(), Some("d2"));
    assert_eq!(created.common().save_paths, vec!["/d1", "/d2"]);

    let initial = GameEntry::Emulator(Emulator::new(
        "GBA",
        "/usr/bin/mgba",
        "/roms",
        vec!["gba".into()],
    ));
    let initial_targets = [dir_target("/d1"), dir_target("/d2")];
    let mut edit = EntryFormState::new(Some(initial), &initial_targets);
    let edited = edit.build_entry().expect("valid edit");
    assert!(
        edited
            .common()
            .save_targets
            .iter()
            .all(|t| t.cloud_path.is_none()),
        "{:?}",
        edited.common().save_targets
    );
}

#[test]
fn cloud_path_error_wins_over_missing_required_fields() {
    let mut form = EntryFormState::new(None, &[]);
    form.saves.update_path(0, "/saves".to_string());
    form.saves.update_cloud_path(0, "..\\x".to_string());
    assert!(form.name.is_empty());

    assert_eq!(
        form.build_entry(),
        Err("Cloud paths must be relative and cannot contain '..'.".to_string())
    );
}

#[test]
fn switching_edited_emulator_to_native_drops_rom_maps_and_keeps_identity() {
    let initial = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "Mixed".to_string(),
            executable_path: "/bin/mixed".to_string(),
            drive_file_id: Some("drive-1".to_string()),
            steam_app_id: Some(777),
            ..EntryCommon::default()
        },
        roms_directory: "/roms".to_string(),
        extensions: vec!["gba".to_string()],
        title_by_rom: vec![("a.gba".to_string(), "A".to_string())],
        ..Emulator::default()
    });
    let mut form = EntryFormState::new(Some(initial), &[dir_target("/saves")]);
    form.entry_type = EntryType::Native;

    let entry = form.build_entry().expect("valid native conversion");

    let GameEntry::Native(native) = entry else {
        panic!("expected a native game");
    };
    assert_eq!(native.common.drive_file_id.as_deref(), Some("drive-1"));
    assert_eq!(native.common.steam_app_id, Some(777));
    assert_eq!(native.cover_path, None);
}

#[test]
fn successful_build_clears_previous_error() {
    let mut form = valid_emulator_form();
    form.name.clear();
    assert!(form.build_entry().is_err());
    assert!(form.error_message.is_some());

    form.name = "GBA".to_string();
    assert!(form.build_entry().is_ok());

    assert_eq!(form.error_message, None);
}

#[test]
fn extensions_split_on_commas_and_spaces_ignoring_empties() {
    let mut form = valid_emulator_form();
    form.extensions = " ,gba,, gbc  ,".to_string();

    let entry = form.build_entry().expect("valid entry");

    let GameEntry::Emulator(emulator) = entry else {
        panic!("expected an emulator");
    };
    assert_eq!(emulator.extensions, vec!["gba", "gbc"]);
}

#[test]
fn native_switch_removes_only_the_bare_rom_placeholder() {
    let mut form = EntryFormState::new(None, &[]);
    form.entry_type = EntryType::Native;

    form.arguments = "{ROM} --x".to_string();
    form.apply_type_defaults();
    assert_eq!(form.arguments, "{ROM} --x");

    form.arguments = " {ROM} ".to_string();
    form.apply_type_defaults();
    assert_eq!(form.arguments, "");
}

// Parity with Kotlin: `EntryFormState.buildEntry` (EntryFormState.kt:55-60) validates
// only `saves.buildTargets()`, i.e. rows with a local path, so an invalid cloud path on
// a row without one is dropped with the row instead of rejecting the form.
#[test]
fn entry_form_ignores_invalid_cloud_path_on_a_row_without_local_path_like_kotlin() {
    let mut form = EntryFormState::new(None, &[]);
    form.entry_type = EntryType::Native;
    form.name = "Game".to_string();
    form.executable_path = "/games/game".to_string();
    form.saves.update_path(0, "/saves".to_string());
    form.saves.add_row();
    form.saves.update_cloud_path(1, "../escape".to_string());

    let entry = form.build_entry().expect("the empty row is dropped");

    assert_eq!(entry.common().save_targets.len(), 1);
    assert_eq!(entry.common().save_targets[0].cloud_path, None);
}
