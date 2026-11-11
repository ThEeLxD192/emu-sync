//! Save target rows and cloud path validation.

use emusync_domain::library::{SaveTarget, SaveTargetType};
use emusync_presentation::SaveTargetFormState;

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
fn cloud_path_validation_rejects_parent_segments_only_when_filled() {
    use emusync_presentation::forms::save_targets::cloud_path_is_valid;

    assert!(cloud_path_is_valid(""));
    assert!(cloud_path_is_valid("   "));
    assert!(cloud_path_is_valid("saves/slot1.sav"));
    assert!(!cloud_path_is_valid("../outside.sav"));
    assert!(!cloud_path_is_valid("saves/../../x"));
}

#[test]
fn out_of_range_edits_are_ignored() {
    let mut form = SaveTargetFormState::default();
    form.update_path(0, "/saves".to_string());
    form.update_cloud_path(0, "cloud".to_string());
    let before = form.clone();

    form.toggle_type(5);
    form.update_path(5, "/x".to_string());
    form.update_type(5, SaveTargetType::Directory);
    form.update_cloud_path(5, "x".to_string());
    form.remove_row(5);

    assert_eq!(form, before);
}

#[test]
fn unknown_kind_toggles_to_file() {
    let mut form = SaveTargetFormState::default();
    form.update_type(0, SaveTargetType::Unknown);

    form.toggle_type(0);

    assert_eq!(form.rows[0].target_type, SaveTargetType::File);
}

#[test]
fn hostile_cloud_paths_are_rejected() {
    let mut form = SaveTargetFormState::default();
    form.update_path(0, "/saves".to_string());

    for hostile in ["C:\\saves", "\\server\\x", "a//b", "./a", "a/./b", "a\0b"] {
        form.update_cloud_path(0, hostile.to_string());
        assert!(!form.cloud_paths_are_valid(), "{hostile:?}");
    }

    form.update_cloud_path(0, "  a/b  ".to_string());
    assert!(form.cloud_paths_are_valid());
    assert_eq!(form.build_targets()[0].cloud_path.as_deref(), Some("a/b"));
}

#[test]
fn rows_without_local_path_are_dropped_from_targets() {
    let mut form = SaveTargetFormState::default();
    form.add_row();
    form.add_row();
    form.update_cloud_path(0, "only-cloud".to_string());
    form.update_path(1, "   ".to_string());
    form.update_path(2, " /real ".to_string());

    let targets = form.build_targets();

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, "/real");
}

fn ctr_folder() -> SaveTargetFormState {
    let mut form = SaveTargetFormState::default();
    form.update_path(0, "/saves/ctr/".to_string());
    form.update_type(0, SaveTargetType::Directory);
    form
}

#[test]
fn picked_excluded_folder_is_stored_relative_to_its_save_folder() {
    let mut form = ctr_folder();
    form.add_excluded_folder(0);
    form.add_excluded_folder(0);

    form.pick_excluded_folder(0, 0, "/saves/ctr/1/CUSA/RaceReplay".to_string());
    form.pick_excluded_folder(0, 1, "/saves/ctr/cache/".to_string());

    assert_eq!(
        form.rows[0].excluded_folders,
        ["1/CUSA/RaceReplay", "cache"]
    );
    assert_eq!(form.validation_error(), None);
    assert_eq!(
        form.build_targets()[0].excluded_folders,
        ["1/CUSA/RaceReplay", "cache"]
    );
}

#[test]
fn a_folder_outside_the_save_folder_or_the_save_folder_itself_blocks_saving() {
    for picked in ["/saves/other/replays", "/saves/ctr", "/saves/ctrl/replays"] {
        let mut form = ctr_folder();
        form.add_excluded_folder(0);
        form.pick_excluded_folder(0, 0, picked.to_string());

        assert_eq!(form.rows[0].excluded_folders, [picked], "{picked}");
        assert!(form.validation_error().is_some(), "{picked}");
    }
    let mut typed = ctr_folder();
    typed.add_excluded_folder(0);
    typed.update_excluded_folder(0, 0, "../replays".to_string());
    assert!(typed.validation_error().is_some());
}

#[test]
fn excluded_folders_survive_loading_and_are_dropped_for_files_and_blanks() {
    let loaded = SaveTargetFormState::from_targets(&[SaveTarget {
        path: "/saves/ctr".into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
        excluded_folders: vec!["replays".into()],
    }]);
    assert_eq!(loaded.build_targets()[0].excluded_folders, ["replays"]);

    let mut form = ctr_folder();
    form.add_excluded_folder(0);
    form.add_excluded_folder(0);
    form.update_excluded_folder(0, 1, "replays".to_string());
    assert_eq!(form.build_targets()[0].excluded_folders, ["replays"]);

    form.remove_excluded_folder(0, 1);
    form.update_excluded_folder(0, 0, "/not/inside".to_string());
    form.toggle_type(0);
    assert_eq!(form.rows[0].target_type, SaveTargetType::File);
    assert_eq!(form.validation_error(), None);
    assert!(form.build_targets()[0].excluded_folders.is_empty());
}
