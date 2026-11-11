//! Delete confirmation: nothing is deleted without an explicit, matching answer.

mod common;

use common::*;
use egui::{Context, RawInput};
use emusync_domain::library::{EntryCommon, GameEntry, NativeGame};
use emusync_presentation::{ActiveDialog, EmuSyncApp, UiAction};

#[test]
fn deleting_a_category_waits_for_confirmation_and_escape_cancels() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    let entry = native_entry("Hollow Knight");
    app.state.config = Some(config_with(vec![entry.clone()], None));
    app.state.selected_entry = Some(entry.clone());

    app.request_delete_entry(entry);
    let output = ctx.run_ui(RawInput::default(), |ui| app.update(ui));
    output.drop_without_applying_deltas();
    assert!(matches!(
        app.active_dialog(),
        ActiveDialog::ConfirmDelete(_)
    ));
    assert!(
        app.take_actions().is_empty(),
        "nothing is deleted before confirming"
    );

    run_frames(&ctx, &mut app, vec![escape()]);

    assert!(matches!(app.active_dialog(), ActiveDialog::None));
    assert!(
        !app.take_actions()
            .iter()
            .any(|a| matches!(a, UiAction::DeleteEntry(_)))
    );
}

#[test]
fn confirming_deletes_exactly_the_requested_entry() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    let (a, b) = (native_entry("A"), native_entry("B"));
    app.state.config = Some(config_with(vec![a.clone(), b.clone()], None));
    app.request_delete_entry(a.clone());
    // The selection moves while the dialog is open.
    app.state.selected_entry = Some(b);
    app.take_actions();

    click_button(&ctx, &mut app, "Delete");

    assert_eq!(app.take_actions(), vec![UiAction::DeleteEntry(a)]);
    assert_eq!(app.active_dialog(), &ActiveDialog::None);
}

#[test]
fn cancel_button_deletes_nothing() {
    let ctx = Context::default();
    let mut app = EmuSyncApp::new();
    app.request_delete_entry(native_entry("A"));
    app.take_actions();

    click_button(&ctx, &mut app, "Cancel");

    assert_eq!(app.take_actions(), Vec::<UiAction>::new());
    assert_eq!(app.active_dialog(), &ActiveDialog::None);
}

#[test]
fn steam_registered_entry_warns_its_shortcut_is_removed() {
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Celeste".to_string(),
            steam_app_id: Some(12345),
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });
    let both = "The category is removed from EmuSync. Its Steam shortcut is removed too.";
    let first_only = "The category is removed from EmuSync.";

    for (registered, shown, hidden) in [(true, both, first_only), (false, first_only, both)] {
        let ctx = Context::default();
        let mut app = EmuSyncApp::new();
        app.state.steam_available = true;
        if registered {
            app.state
                .steam_registered_entries
                .insert("12345".to_string());
        }
        app.request_delete_entry(entry.clone());
        run_frames(&ctx, &mut app, (0..6).map(|_| Vec::new()).collect());

        assert!(
            text_exists(&ctx, &mut app, shown),
            "registered={registered}"
        );
        assert!(
            !text_exists(&ctx, &mut app, hidden),
            "registered={registered}"
        );
    }
}
