//! Results of background work (pickers, restore previews, save kind detection)
//! applied to whatever dialog is open when they arrive.

mod common;

use common::*;
use emusync_domain::library::SaveTargetType;
use emusync_presentation::{
    ActiveDialog, EmuSyncApp, EntryType, UiAction,
    model::{PathTarget, PickMode},
};

fn entry_dialog_with_rows(rows: &[(&str, SaveTargetType)]) -> EmuSyncApp {
    let mut app = EmuSyncApp::new();
    app.open_add_entry();
    let form = app.entry_form_mut().expect("entry dialog");
    for (index, (path, kind)) in rows.iter().enumerate() {
        if index > 0 {
            form.saves.add_row();
        }
        form.saves.update_path(index, path.to_string());
        form.saves.update_type(index, kind.clone());
    }
    app
}

/// An entry dialog for a native game, whose save rows are visible without scrolling.
fn native_entry_dialog() -> EmuSyncApp {
    let mut app = EmuSyncApp::new();
    app.open_add_entry();
    app.entry_form_mut().expect("entry dialog").entry_type = EntryType::Native;
    app
}

#[test]
fn picked_save_path_takes_the_picked_kind_and_stale_detection_is_ignored() {
    let ctx = egui::Context::default();
    let mut app = native_entry_dialog();

    let (id, target, mode) = pick_request(click_nth(&ctx, &mut app, "Folder", 0));
    assert_eq!(
        (target.clone(), mode),
        (PathTarget::SavePath(0), PickMode::Directory)
    );
    app.apply_picked_path(id, target, mode, "/saves/folder".to_string());
    let ActiveDialog::Entry(form) = &app.active_dialog() else {
        panic!("entry dialog must stay open");
    };
    assert_eq!(form.saves.rows[0].path, "/saves/folder");
    assert_eq!(form.saves.rows[0].target_type, SaveTargetType::Directory);

    // A detection for a path the row no longer holds must not change its kind.
    app.apply_detected_save_type(0, "/saves/old-file", SaveTargetType::File);
    let ActiveDialog::Entry(form) = &app.active_dialog() else {
        unreachable!()
    };
    assert_eq!(form.saves.rows[0].target_type, SaveTargetType::Directory);

    app.apply_detected_save_type(0, "/saves/folder", SaveTargetType::File);
    let ActiveDialog::Entry(form) = &app.active_dialog() else {
        unreachable!()
    };
    assert_eq!(form.saves.rows[0].target_type, SaveTargetType::File);

    let (id, target, mode) = pick_request(click_nth(&ctx, &mut app, "Browse", 0));
    assert_eq!(target, PathTarget::Executable);
    app.apply_picked_path(id, target, mode, "/usr/bin/mgba".to_string());
    let ActiveDialog::Entry(form) = &app.active_dialog() else {
        unreachable!()
    };
    assert_eq!(form.executable_path, "/usr/bin/mgba");
}

#[test]
fn results_without_a_matching_dialog_are_ignored_without_panicking() {
    let mut none = EmuSyncApp::new();
    let mut update = EmuSyncApp::new();
    update.open_update_dialog();
    let mut delete = EmuSyncApp::new();
    delete.request_delete_entry(native_entry("A"));

    for (name, app) in [
        ("none", &mut none),
        ("update", &mut update),
        ("delete", &mut delete),
    ] {
        let before = app.active_dialog().clone();

        app.apply_picked_path(1, PathTarget::SavePath(0), PickMode::File, "/x".to_string());
        app.apply_restore_preview(2, "/x", vec!["/x/a".to_string()]);
        app.apply_detected_save_type(0, "/x", SaveTargetType::File);

        assert_eq!(app.active_dialog(), &before, "{name}");
        assert!(app.take_actions().is_empty(), "{name}");
    }
}

#[test]
fn picked_path_for_a_field_the_open_dialog_lacks_is_dropped() {
    let ctx = egui::Context::default();

    // The entry form asks for an executable; a game dialog replaces it.
    let mut game = native_entry_dialog();
    let (id, target, mode) = pick_request(click_nth(&ctx, &mut game, "Browse", 0));
    assert_eq!(target, PathTarget::Executable);
    game.open_edit_game(&setup_request(1, &[]).item, &[]);
    let before = game.active_dialog().clone();
    game.apply_picked_path(id, target, mode, "/bin/x".to_string());
    game.apply_picked_path(
        id + 1,
        PathTarget::RomsDirectory,
        PickMode::Directory,
        "/r".to_string(),
    );
    assert_eq!(game.active_dialog(), &before);

    // The game dialog asks for a cover; an entry form replaces it.
    let mut entry = EmuSyncApp::new();
    entry.open_edit_game(&setup_request(1, &[]).item, &[]);
    let (id, target, mode) = pick_request(click_nth(&ctx, &mut entry, "Select Image...", 0));
    assert_eq!(target, PathTarget::CoverPath);
    entry.open_add_entry();
    let before = entry.active_dialog().clone();
    entry.apply_picked_path(id, PathTarget::RestoreDestination, mode, "/d".to_string());
    assert_eq!(entry.active_dialog(), &before);
}

#[test]
fn out_of_range_save_row_results_are_ignored() {
    let ctx = egui::Context::default();
    let mut app = native_entry_dialog();
    app.entry_form_mut().expect("entry dialog").saves.add_row();

    // The picker of the second row is open when that row disappears.
    scroll_dialog_to_end(&ctx, &mut app);
    let (id, target, mode) = pick_request(
        // Each row paints a "File" picker and then a "File" kind toggle.
        click_nth(&ctx, &mut app, "File", 2),
    );
    assert_eq!(target, PathTarget::SavePath(1));
    app.entry_form_mut()
        .expect("entry dialog")
        .saves
        .remove_row(1);
    let before = app.active_dialog().clone();

    app.apply_picked_path(id, target, mode, "/x".to_string());
    app.apply_detected_save_type(99, "/x", SaveTargetType::Directory);

    assert_eq!(app.active_dialog(), &before);
    let ActiveDialog::Entry(form) = app.active_dialog() else {
        unreachable!()
    };
    assert_eq!(form.saves.rows.len(), 1);
    assert_eq!(form.saves.rows[0].path, "");
    assert_eq!(form.saves.rows[0].target_type, SaveTargetType::File);
}

#[test]
fn detection_for_a_removed_row_does_not_retarget_the_shifted_row() {
    let mut app =
        entry_dialog_with_rows(&[("/a", SaveTargetType::File), ("/b", SaveTargetType::File)]);
    app.entry_form_mut()
        .expect("entry dialog")
        .saves
        .remove_row(0);

    app.apply_detected_save_type(0, "/a", SaveTargetType::Directory);

    let ActiveDialog::Entry(form) = app.active_dialog() else {
        unreachable!()
    };
    assert_eq!(form.saves.rows.len(), 1);
    assert_eq!(form.saves.rows[0].path, "/b");
    assert_eq!(form.saves.rows[0].target_type, SaveTargetType::File);
}

#[test]
fn restore_preview_for_a_replaced_save_setup_is_ignored() {
    let ctx = egui::Context::default();
    let mut app = EmuSyncApp::new();

    // Setup 1 asks for the preview of /dest for [a.sav].
    app.open_save_setup(&setup_request(1, &["a.sav"]));
    let (old_id, destination, names) =
        preview_request(choose_restore_destination(&ctx, &mut app, "/dest"));
    assert_eq!(
        (destination.as_str(), names),
        ("/dest", vec!["a.sav".to_string()])
    );

    // Setup 2 replaces it and the user picks the same destination.
    app.open_save_setup(&setup_request(2, &["b.sav"]));
    assert_eq!(
        app.take_actions(),
        vec![UiAction::CancelSaveSetup { request_id: 1 }]
    );
    let (new_id, destination, names) =
        preview_request(choose_restore_destination(&ctx, &mut app, "/dest"));
    assert_ne!(old_id, new_id);
    assert_eq!(
        (destination.as_str(), names),
        ("/dest", vec!["b.sav".to_string()]),
        "setup 2 never asked for its own preview"
    );

    // The old answer arrives late.
    app.apply_restore_preview(old_id, "/dest", vec!["/dest/a.sav".to_string()]);

    // Setup 2 refuses to save with paths resolved for another game.
    click_button(&ctx, &mut app, "Save");
    assert_eq!(
        app.take_actions(),
        Vec::<UiAction>::new(),
        "setup 2 saved with paths resolved for another game"
    );

    // Its own answer unblocks it.
    app.apply_restore_preview(new_id, "/dest", vec!["/dest/b.sav".to_string()]);
    click_button(&ctx, &mut app, "Save");
    assert!(matches!(
        app.take_actions().as_slice(),
        [UiAction::CompleteSaveSetup { request, .. }] if request.id == 2
    ));
}

#[test]
fn picked_path_requested_by_the_entry_form_is_not_applied_to_a_save_setup() {
    let ctx = egui::Context::default();
    let mut app = native_entry_dialog();
    // The entry form asks for a save path (its first "File" button)...
    let (id, target, mode) = pick_request(click_nth(&ctx, &mut app, "File", 0));
    assert_eq!(
        (target.clone(), mode),
        (PathTarget::SavePath(0), PickMode::File)
    );

    // ...and a save setup replaces it before the picker answers.
    app.open_save_setup(&setup_request(1, &[]));
    app.take_actions();
    app.apply_picked_path(id, target, mode, "/picked/for/the/entry/form".to_string());

    let ActiveDialog::GameEdit { form, .. } = app.active_dialog() else {
        panic!("the save setup must stay open");
    };
    assert_eq!(form.saves.rows[0].path, "");
}

/// Setup 1 (`a.sav`) is replaced by setup 2 (`b.sav`) and both preview `/dest`; the
/// answers arrive in `order` (indexes into `[old, new]`). Returns the paths the open
/// form shows and the actions Save publishes.
fn replaced_setup_previews(order: [usize; 2]) -> (Vec<String>, Vec<UiAction>) {
    let ctx = egui::Context::default();
    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request(1, &["a.sav"]));
    let (old_id, ..) = preview_request(choose_restore_destination(&ctx, &mut app, "/dest"));
    app.open_save_setup(&setup_request(2, &["b.sav"]));
    app.take_actions();
    let (new_id, ..) = preview_request(choose_restore_destination(&ctx, &mut app, "/dest"));

    let answers = [
        (old_id, "/dest/a.sav".to_string()),
        (new_id, "/dest/b.sav".to_string()),
    ];
    for index in order {
        let (id, path) = answers[index].clone();
        app.apply_restore_preview(id, "/dest", vec![path]);
    }

    let ActiveDialog::GameEdit {
        form, cloud_files, ..
    } = app.active_dialog()
    else {
        panic!("setup 2 must stay open");
    };
    let shown = form.selected_restore_paths(cloud_files);
    click_button(&ctx, &mut app, "Save");
    (shown, app.take_actions())
}

#[test]
fn out_of_order_previews_for_the_same_destination_apply_only_the_current_request() {
    for order in [[1, 0], [0, 1]] {
        let (shown, actions) = replaced_setup_previews(order);
        assert_eq!(shown, vec!["/dest/b.sav".to_string()], "order {order:?}");
        match actions.as_slice() {
            [
                UiAction::CompleteSaveSetup {
                    request,
                    restore,
                    targets,
                    ..
                },
            ] => {
                assert_eq!(request.id, 2, "order {order:?}");
                assert_eq!(
                    restore,
                    &Some(emusync_presentation::RestoreChoice {
                        destination: "/dest".to_string(),
                        whole_folder: false,
                    }),
                    "order {order:?}"
                );
                assert!(targets.is_empty(), "order {order:?}");
            }
            other => panic!("order {order:?}: expected one CompleteSaveSetup, got {other:?}"),
        }
    }
}

#[test]
fn out_of_order_pickers_for_the_same_field_apply_only_the_current_dialog() {
    let ctx = egui::Context::default();
    let mut app = native_entry_dialog();
    let (old_id, target, mode) = pick_request(click_nth(&ctx, &mut app, "File", 0));
    assert_eq!(target, PathTarget::SavePath(0));

    app.open_save_setup(&setup_request(1, &[]));
    let (new_id, new_target, new_mode) = pick_request(click_nth(&ctx, &mut app, "File", 0));
    assert_eq!(new_target, PathTarget::SavePath(0));

    app.apply_picked_path(new_id, new_target, new_mode, "/new".to_string());
    app.apply_picked_path(old_id, target, mode, "/old".to_string());

    let ActiveDialog::GameEdit { form, .. } = app.active_dialog() else {
        panic!("the save setup must stay open");
    };
    assert_eq!(form.saves.rows[0].path, "/new");
}

#[test]
fn unknown_or_repeated_request_ids_are_ignored() {
    let ctx = egui::Context::default();

    // Pickers.
    let mut app = native_entry_dialog();
    let (id, target, mode) = pick_request(click_nth(&ctx, &mut app, "File", 0));
    let before = app.active_dialog().clone();
    app.apply_picked_path(9999, target.clone(), mode, "/unknown".to_string());
    assert_eq!(app.active_dialog(), &before, "unknown picker id");

    app.apply_picked_path(id, target.clone(), mode, "/first".to_string());
    let ActiveDialog::Entry(form) = app.active_dialog() else {
        unreachable!()
    };
    assert_eq!(form.saves.rows[0].path, "/first");
    let applied = app.active_dialog().clone();
    app.apply_picked_path(id, target, mode, "/second".to_string());
    assert_eq!(app.active_dialog(), &applied, "repeated picker id");

    // Restore previews.
    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request(1, &["a.sav"]));
    let (id, ..) = preview_request(choose_restore_destination(&ctx, &mut app, "/dest"));
    let before = app.active_dialog().clone();
    app.apply_restore_preview(9999, "/dest", vec!["/dest/a.sav".to_string()]);
    assert_eq!(app.active_dialog(), &before, "unknown preview id");

    app.apply_restore_preview(id, "/dest", vec!["/dest/a.sav".to_string()]);
    let applied = app.active_dialog().clone();
    assert_ne!(applied, before, "the real answer must apply");
    app.apply_restore_preview(id, "/dest", vec!["/dest/other.sav".to_string()]);
    assert_eq!(app.active_dialog(), &applied, "repeated preview id");
}

#[test]
fn answer_with_a_mismatched_target_or_destination_is_ignored() {
    let ctx = egui::Context::default();

    let mut app = native_entry_dialog();
    let (id, target, mode) = pick_request(click_nth(&ctx, &mut app, "File", 0));
    assert_eq!(target, PathTarget::SavePath(0));
    let before = app.active_dialog().clone();
    app.apply_picked_path(id, PathTarget::CoverPath, mode, "/cover".to_string());
    assert_eq!(app.active_dialog(), &before);

    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request(1, &["a.sav"]));
    let (id, ..) = preview_request(choose_restore_destination(&ctx, &mut app, "/dest"));
    let before = app.active_dialog().clone();
    app.apply_restore_preview(id, "/other", vec!["/other/a.sav".to_string()]);
    assert_eq!(app.active_dialog(), &before);
}

#[test]
fn published_requests_carry_distinct_non_zero_ids() {
    let ctx = egui::Context::default();
    let mut app = native_entry_dialog();
    let (first, ..) = pick_request(click_nth(&ctx, &mut app, "File", 0));
    let (second, ..) = pick_request(click_nth(&ctx, &mut app, "File", 0));

    app.open_save_setup(&setup_request(1, &["a.sav"]));
    let (preview, ..) = preview_request(choose_restore_destination(&ctx, &mut app, "/dest"));

    assert!(first != 0 && second != 0 && preview != 0);
    assert!(
        first < second && second < preview,
        "{first} {second} {preview}"
    );
}

#[test]
fn excluded_folder_picker_opens_in_its_save_folder_and_keeps_the_choice_relative() {
    let ctx = egui::Context::default();
    let mut app = native_entry_dialog();
    let form = app.entry_form_mut().expect("entry dialog");
    form.saves.update_path(0, "/saves/ctr".to_string());
    form.saves.update_type(0, SaveTargetType::Directory);

    run_frames(&ctx, &mut app, vec![vec![]]);
    scroll_dialog_to_end(&ctx, &mut app);
    click_nth(&ctx, &mut app, "+ Add Excluded Folder", 0);
    scroll_dialog_to_end(&ctx, &mut app);
    // Folder buttons, top to bottom: the save path picker, its kind toggle, then the
    // excluded folder's picker.
    let actions = click_nth(&ctx, &mut app, "Folder", 2);
    let [
        UiAction::PickPath {
            request_id,
            target,
            mode,
            initial_path,
        },
    ] = actions.as_slice()
    else {
        panic!("expected exactly one PickPath, got {actions:?}");
    };
    assert_eq!(
        (target, *mode, initial_path.as_str()),
        (
            &PathTarget::ExcludedFolder { save: 0, folder: 0 },
            PickMode::Directory,
            "/saves/ctr"
        )
    );

    app.apply_picked_path(
        *request_id,
        target.clone(),
        *mode,
        "/saves/ctr/1/CUSA/RaceReplay".to_string(),
    );

    let ActiveDialog::Entry(form) = app.active_dialog() else {
        panic!("entry dialog must stay open");
    };
    assert_eq!(form.saves.rows[0].path, "/saves/ctr");
    assert_eq!(form.saves.rows[0].excluded_folders, ["1/CUSA/RaceReplay"]);
}
