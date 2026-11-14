//! Dialog lifecycle: opening, replacing, saving and dismissing modals, and the
//! guarantee that every save setup request is answered exactly once.

mod common;

use common::*;
use egui::{Context, RawInput};
use emusync_domain::library::{Emulator, GameEntry, SaveTarget, SaveTargetType};
use emusync_presentation::{ActiveDialog, AppStatus, EmuSyncApp, EntryType, UiAction};

fn cancel(request_id: u64) -> UiAction {
    UiAction::CancelSaveSetup { request_id }
}

#[test]
fn app_opens_and_submits_add_category_modal() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();

    app.open_add_entry();
    assert!(matches!(app.active_dialog(), ActiveDialog::Entry(_)));

    // Fill the active form
    if let Some(form) = app.entry_form_mut() {
        form.name = "GameBoy Advance".to_string();
        form.executable_path = "mgba".to_string();
        form.roms_directory = "/roms/gba".to_string();
        form.extensions = "gba".to_string();
        form.saves.update_path(0, "/saves/gba".to_string());
    }

    click_button(&ctx, &mut app, "Add");

    let actions = app.take_actions();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        UiAction::AddEntry(entry) => {
            assert_eq!(entry.common().name, "GameBoy Advance");
            assert_eq!(entry.common().executable_path, "mgba");
        }
        _ => panic!("expected AddEntry action"),
    }
    assert_eq!(app.active_dialog(), &ActiveDialog::None);
}

#[test]
fn save_setup_save_completes_the_same_request() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    let request = setup_request(1, &[]);
    app.open_save_setup(&request);

    click_button(&ctx, &mut app, "Save");

    let actions = app.take_actions();
    assert!(
        matches!(
            actions.as_slice(),
            [UiAction::CompleteSaveSetup { request: sent, .. }] if **sent == request
        ),
        "{actions:?}"
    );
    assert!(matches!(app.active_dialog(), ActiveDialog::None));
}

#[test]
fn save_setup_cancel_and_escape_each_emit_one_cancel() {
    for dismiss in ["Cancel", "Escape"] {
        let ctx = Context::default();
        let mut app = EmuSyncApp::new();
        app.open_save_setup(&setup_request(1, &[]));
        run_frames(&ctx, &mut app, vec![Vec::new()]);

        if dismiss == "Cancel" {
            click_button(&ctx, &mut app, "Cancel");
        } else {
            run_frames(&ctx, &mut app, vec![escape()]);
        }

        assert_eq!(app.take_actions(), vec![cancel(1)], "{dismiss}");
        assert!(matches!(app.active_dialog(), ActiveDialog::None));
    }
}

#[test]
fn replacing_a_save_setup_cancels_the_previous_request() {
    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request(1, &[]));
    app.open_save_setup(&setup_request(1, &[]));
    assert_eq!(app.take_actions(), vec![cancel(1)]);

    app.close_dialog();
    assert_eq!(app.take_actions(), vec![cancel(1)]);

    app.close_dialog();
    assert!(app.take_actions().is_empty());
}

#[test]
fn close_save_setup_closes_the_matching_request_without_cancelling() {
    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request(1, &[]));

    app.close_save_setup(1);

    assert!(matches!(app.active_dialog(), ActiveDialog::None));
    assert!(app.take_actions().is_empty());
}

#[test]
fn close_save_setup_for_another_id_keeps_the_dialog() {
    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request(1, &[]));

    app.close_save_setup(2);

    assert!(matches!(app.active_dialog(), ActiveDialog::GameEdit { .. }));
    assert!(app.take_actions().is_empty());
}

#[test]
fn close_save_setup_leaves_other_dialogs_untouched() {
    let mut app = EmuSyncApp::new();
    app.open_add_entry();
    app.close_save_setup(1);
    assert!(matches!(app.active_dialog(), ActiveDialog::Entry(_)));

    app.close_dialog();
    app.state.update_state = emusync_application::UpdateState::Available(update_info("1.3.0"));
    app.open_update_dialog();
    app.close_save_setup(1);
    assert_eq!(app.active_dialog(), &ActiveDialog::Update);
}

#[test]
fn game_override_save_emits_override_without_cancel() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    let item = setup_request(1, &[]).item;
    app.open_edit_game(&item, &[]);

    click_button(&ctx, &mut app, "Save");

    let actions = app.take_actions();
    assert!(
        matches!(actions.as_slice(), [UiAction::EditGameOverride { .. }]),
        "{actions:?}"
    );
}

#[test]
fn save_setup_stays_clickable_above_a_launch_status() {
    for status in [AppStatus::Playing, AppStatus::Loading] {
        let ctx = Context::default();
        let mut app = EmuSyncApp::new();
        app.state.status = status.clone();
        app.open_save_setup(&setup_request(1, &[]));

        click_button(&ctx, &mut app, "Save");

        let actions = app.take_actions();
        assert!(
            matches!(actions.as_slice(), [UiAction::CompleteSaveSetup { .. }]),
            "{status:?}: {actions:?}"
        );
    }
}

#[test]
fn entry_dialog_renders_both_entry_types_headless() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.open_add_entry();
    for _ in 0..2 {
        let output = ctx.run_ui(RawInput::default(), |ui| app.update(ui));
        output.drop_without_applying_deltas();
        if let Some(form) = app.entry_form_mut() {
            form.entry_type = EntryType::Native;
        }
    }
    assert!(matches!(app.active_dialog(), ActiveDialog::Entry(_)));
    assert!(app.take_actions().is_empty());
}

#[test]
fn app_toast_message_set_and_clear() {
    let mut app = EmuSyncApp::new();
    assert_eq!(app.state.toast_message, None);

    app.set_toast("Shortcut added to Steam");
    assert_eq!(
        app.state.toast_message,
        Some("Shortcut added to Steam".to_string())
    );

    app.clear_toast();
    assert_eq!(app.state.toast_message, None);
}

type Replace = fn(&mut EmuSyncApp);
type DialogCheck = fn(&ActiveDialog) -> bool;

#[test]
fn every_dialog_replacing_a_save_setup_answers_it_exactly_once() {
    let table: [(&str, Replace, DialogCheck, Vec<UiAction>); 8] = [
        (
            "add entry",
            |app| app.open_add_entry(),
            |d| matches!(d, ActiveDialog::Entry(_)),
            vec![],
        ),
        (
            "edit entry",
            |app| app.open_edit_entry(&native_entry("A")),
            |d| matches!(d, ActiveDialog::Entry(_)),
            vec![],
        ),
        (
            "edit game",
            |app| app.open_edit_game(&setup_request(9, &[]).item, &[]),
            |d| matches!(d, ActiveDialog::GameEdit { .. }),
            vec![],
        ),
        (
            "drive setup",
            |app| app.open_drive_setup(),
            |d| matches!(d, ActiveDialog::DriveSetup(_)),
            vec![],
        ),
        (
            "confirm delete",
            |app| app.request_delete_entry(native_entry("A")),
            |d| matches!(d, ActiveDialog::ConfirmDelete(_)),
            vec![],
        ),
        (
            "update",
            |app| app.open_update_dialog(),
            |d| matches!(d, ActiveDialog::Update),
            vec![],
        ),
        (
            "close",
            |app| app.close_dialog(),
            |d| matches!(d, ActiveDialog::None),
            vec![],
        ),
        (
            "another save setup",
            |app| app.open_save_setup(&setup_request(2, &[])),
            |d| matches!(d, ActiveDialog::GameEdit { .. }),
            // The second setup is itself cancelled by the later close, exactly once.
            vec![cancel(2)],
        ),
    ];

    for (name, replace, expected_dialog, cancels_after_close) in table {
        let mut app = EmuSyncApp::new();
        app.open_save_setup(&setup_request(1, &[]));
        assert!(
            app.take_actions().is_empty(),
            "{name}: opening emits nothing"
        );

        replace(&mut app);
        assert_eq!(app.take_actions(), vec![cancel(1)], "{name}: replacement");
        assert!(expected_dialog(app.active_dialog()), "{name}: dialog");

        app.close_dialog();
        app.close_dialog();
        assert_eq!(
            app.take_actions(),
            cancels_after_close,
            "{name}: closing afterwards"
        );
        assert_eq!(app.active_dialog(), &ActiveDialog::None, "{name}");
    }
}

#[test]
fn completed_save_setup_is_never_cancelled_afterwards() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request(1, &[]));
    app.take_actions();

    click_button(&ctx, &mut app, "Save");
    let mut actions = app.take_actions();
    app.close_dialog();
    app.close_save_setup(1);
    run_frames(&ctx, &mut app, vec![Vec::new()]);
    actions.extend(app.take_actions());

    assert!(
        matches!(actions.as_slice(), [UiAction::CompleteSaveSetup { .. }]),
        "{actions:?}"
    );
    assert_eq!(app.active_dialog(), &ActiveDialog::None);
}

#[test]
fn override_dialog_cancel_emits_nothing() {
    for dismiss in ["Cancel", "Escape"] {
        let ctx = Context::default();
        let mut app = EmuSyncApp::new();
        app.open_edit_game(&setup_request(1, &[]).item, &[]);
        run_frames(&ctx, &mut app, vec![Vec::new()]);
        app.take_actions();

        if dismiss == "Cancel" {
            click_button(&ctx, &mut app, "Cancel");
        } else {
            run_frames(&ctx, &mut app, vec![escape()]);
        }

        assert_eq!(app.take_actions(), Vec::<UiAction>::new(), "{dismiss}");
        assert_eq!(app.active_dialog(), &ActiveDialog::None, "{dismiss}");
    }
}

#[test]
fn editing_an_entry_sends_the_entry_captured_when_the_dialog_opened() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    let mut original = Emulator::new("A", "/usr/bin/a", "/roms", vec!["gba".to_string()]);
    original.common.save_targets = vec![SaveTarget {
        path: "/saves/a".to_string(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
        excluded_folders: Vec::new(),
    }];
    let original = GameEntry::Emulator(original);
    app.state.config = Some(config_with(vec![original.clone()], None));
    app.state.selected_entry = Some(original.clone());

    app.open_edit_entry(&original);
    app.take_actions();

    // The library changes underneath the open dialog.
    let mut replacement = Emulator::new("A", "/opt/other/a", "/roms", vec!["gba".to_string()]);
    replacement.common.save_targets = original.common().save_targets.clone();
    let replacement = GameEntry::Emulator(replacement);
    app.state.config = Some(config_with(vec![replacement.clone()], None));
    app.state.selected_entry = Some(replacement);
    app.entry_form_mut().expect("entry dialog").name = "A renamed".to_string();

    click_button(&ctx, &mut app, "Save");

    let actions = app.take_actions();
    match actions.as_slice() {
        [UiAction::EditEntry { old, new }] => {
            assert_eq!(**old, original);
            assert_eq!(new.common().name, "A renamed");
        }
        other => panic!("expected one EditEntry, got {other:?}"),
    }
    assert_eq!(app.active_dialog(), &ActiveDialog::None);
}

#[test]
fn invalid_entry_form_keeps_dialog_open_and_emits_nothing() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.open_add_entry();
    {
        let form = app.entry_form_mut().expect("entry dialog");
        form.name = "X".to_string();
        form.executable_path = "/x".to_string();
        form.entry_type = EntryType::Native;
        form.saves.update_path(0, "/saves".to_string());
        form.saves.update_cloud_path(0, "../escape".to_string());
    }
    app.take_actions();

    click_button(&ctx, &mut app, "Add");

    assert_eq!(app.take_actions(), Vec::<UiAction>::new());
    let ActiveDialog::Entry(form) = app.active_dialog() else {
        panic!("the entry dialog must stay open");
    };
    assert_eq!(
        form.error_message,
        Some(emusync_presentation::forms::FormError::CloudPathInvalid)
    );
}

#[test]
fn restore_setup_requests_preview_once_and_saves_only_after_resolution() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request(1, &["0.sav", "1.sav"]));
    let (request_id, destination, names) =
        preview_request(choose_restore_destination(&ctx, &mut app, "/dest"));
    assert_eq!(destination, "/dest");
    assert_eq!(names, vec!["0.sav".to_string(), "1.sav".to_string()]);

    // (a) The preview is requested once, however many frames follow.
    run_frames(&ctx, &mut app, vec![Vec::new(), Vec::new(), Vec::new()]);
    assert_eq!(app.take_actions(), Vec::<UiAction>::new());

    // (b) Saving is blocked until the paths are resolved.
    click_button(&ctx, &mut app, "Save");
    assert_eq!(app.take_actions(), Vec::<UiAction>::new());
    assert!(matches!(app.active_dialog(), ActiveDialog::GameEdit { .. }));

    // (c) Once resolved, saving completes the same request.
    app.apply_restore_preview(
        request_id,
        "/dest",
        vec!["/dest/0.sav".to_string(), "/dest/1.sav".to_string()],
    );
    click_button(&ctx, &mut app, "Save");
    let actions = app.take_actions();
    match actions.as_slice() {
        [
            UiAction::CompleteSaveSetup {
                request,
                targets,
                restore,
                ..
            },
        ] => {
            assert_eq!(request.id, 1);
            assert!(targets.is_empty());
            assert_eq!(
                restore,
                &Some(emusync_presentation::RestoreChoice {
                    destination: "/dest".to_string(),
                    whole_folder: false,
                })
            );
        }
        other => panic!("expected one CompleteSaveSetup, got {other:?}"),
    }
    assert_eq!(app.active_dialog(), &ActiveDialog::None);
}

#[test]
fn invalid_cloud_path_disables_game_save() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.open_edit_game(
        &setup_request(1, &[]).item,
        &[SaveTarget {
            path: "/s".to_string(),
            kind: SaveTargetType::File,
            cloud_path: Some("/abs".to_string()),
            excluded_folders: Vec::new(),
        }],
    );
    app.take_actions();

    click_button(&ctx, &mut app, "Save");

    assert_eq!(app.take_actions(), Vec::<UiAction>::new());
    assert!(matches!(app.active_dialog(), ActiveDialog::GameEdit { .. }));
}

fn draft_name(app: &EmuSyncApp) -> Option<&str> {
    match app.active_dialog() {
        ActiveDialog::Entry(form) => Some(form.name.as_str()),
        _ => None,
    }
}

fn complete(actions: &[UiAction]) -> Option<u64> {
    match actions {
        [UiAction::CompleteSaveSetup { request, .. }] => Some(request.id),
        _ => None,
    }
}

#[test]
fn update_flag_waits_for_an_open_save_setup_and_opens_after_it() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.open_save_setup(&setup_request(1, &[]));
    app.state.show_update_dialog = true;

    run_frames(&ctx, &mut app, vec![Vec::new(), Vec::new(), Vec::new()]);

    assert_eq!(app.take_actions(), vec![]);
    assert!(matches!(app.active_dialog(), ActiveDialog::GameEdit { .. }));
    assert!(app.state.show_update_dialog);

    click_button(&ctx, &mut app, "Save");
    let actions = app.take_actions();
    assert_eq!(complete(&actions), Some(1), "{actions:?}");

    run_frames(&ctx, &mut app, vec![Vec::new()]);
    assert_eq!(app.active_dialog(), &ActiveDialog::Update);
    assert!(!app.state.show_update_dialog);
    assert_eq!(app.take_actions(), vec![]);
}

#[test]
fn update_flag_waits_for_an_open_entry_form() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.open_add_entry();
    app.entry_form_mut().expect("entry form").name = "Draft".to_string();
    app.state.show_update_dialog = true;

    run_frames(&ctx, &mut app, vec![Vec::new(), Vec::new()]);

    assert_eq!(draft_name(&app), Some("Draft"));
    assert!(app.state.show_update_dialog);
    assert_eq!(app.take_actions(), vec![]);
}

#[test]
fn save_setup_suspends_an_open_entry_form_and_restores_its_draft() {
    for ending in ["Cancel", "Escape", "Save", "close_save_setup"] {
        let ctx = Context::default();
        let mut app = EmuSyncApp::new();
        app.open_add_entry();
        app.entry_form_mut().expect("entry form").name = "Draft".to_string();

        app.open_save_setup(&setup_request(1, &[]));
        assert_eq!(
            app.take_actions(),
            vec![],
            "{ending}: opening emits nothing"
        );
        assert!(matches!(app.active_dialog(), ActiveDialog::GameEdit { .. }));

        let actions = match ending {
            "Cancel" => {
                click_button(&ctx, &mut app, "Cancel");
                app.take_actions()
            }
            "Escape" => {
                run_frames(&ctx, &mut app, vec![Vec::new()]);
                run_frames(&ctx, &mut app, vec![escape()]);
                app.take_actions()
            }
            "Save" => {
                click_button(&ctx, &mut app, "Save");
                let actions = app.take_actions();
                assert_eq!(complete(&actions), Some(1), "{actions:?}");
                vec![]
            }
            _ => {
                app.close_save_setup(1);
                app.take_actions()
            }
        };

        let expected = if matches!(ending, "Cancel" | "Escape") {
            vec![cancel(1)]
        } else {
            vec![]
        };
        assert_eq!(actions, expected, "{ending}");
        assert_eq!(draft_name(&app), Some("Draft"), "{ending}");
        if ending == "Escape" {
            run_frames(&ctx, &mut app, vec![Vec::new(), Vec::new()]);
            assert_eq!(draft_name(&app), Some("Draft"), "{ending}: still open");
        }
    }
}

#[test]
fn update_flag_raised_while_the_update_dialog_is_open_does_not_reopen_it() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.state.update_state = emusync_application::UpdateState::Available(update_info("9.9.9"));
    app.open_update_dialog();
    run_frames(&ctx, &mut app, vec![Vec::new()]);

    app.state.show_update_dialog = true;
    run_frames(&ctx, &mut app, vec![Vec::new()]);
    assert_eq!(app.active_dialog(), &ActiveDialog::Update);
    assert!(!app.state.show_update_dialog);

    click_button(&ctx, &mut app, "Later");
    run_frames(&ctx, &mut app, vec![Vec::new(), Vec::new()]);
    assert_eq!(app.active_dialog(), &ActiveDialog::None);
    assert!(!app.state.show_update_dialog);
    assert_eq!(app.take_actions(), vec![]);
}

#[test]
fn replacing_one_save_setup_with_another_keeps_the_suspended_dialog() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.open_add_entry();
    app.entry_form_mut().expect("entry form").name = "Draft".to_string();

    app.open_save_setup(&setup_request(1, &[]));
    assert_eq!(app.take_actions(), vec![]);
    app.open_save_setup(&setup_request(2, &[]));
    assert_eq!(app.take_actions(), vec![cancel(1)]);

    click_button(&ctx, &mut app, "Cancel");
    assert_eq!(app.take_actions(), vec![cancel(2)]);
    assert_eq!(draft_name(&app), Some("Draft"));
}
