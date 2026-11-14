//! Google Drive setup dialog: credentials, link state and secret hygiene.

mod common;

use common::*;
use egui::Context;
use emusync_domain::config::Credentials;
use emusync_presentation::{ActiveDialog, EmuSyncApp, UiAction, dialogs::DriveSetupDialogState};

fn app_with_credentials(id: &str, secret: &str, token: Option<&str>) -> EmuSyncApp {
    let mut app = EmuSyncApp::new();
    app.state.config = Some(config_with(
        Vec::new(),
        Some(Credentials {
            client_id: id.to_string(),
            client_secret: secret.to_string(),
            refresh_token: token.map(str::to_string),
        }),
    ));
    app
}

#[test]
fn drive_secret_never_appears_in_debug_output() {
    let action = UiAction::ConnectDrive {
        client_id: "id".into(),
        client_secret: "GOCSPX-secret".into(),
    };
    assert!(!format!("{action:?}").contains("GOCSPX-secret"));
    let state = DriveSetupDialogState::new("id".into(), "GOCSPX-secret".into(), false);
    assert!(!format!("{state:?}").contains("GOCSPX-secret"));
}

#[test]
fn drive_setup_sends_trimmed_credentials_and_flags_blank_ones() {
    let mut state = DriveSetupDialogState::new("  id.apps  ".into(), "   ".into(), false);
    assert!(!state.secret_revealed, "the secret starts masked");

    assert_eq!(state.submit(), None, "a blank secret must not connect");
    assert_eq!(
        state.error,
        Some(emusync_presentation::dialogs::DriveSetupError::MissingCredentials)
    );

    state.field_edited();
    assert_eq!(state.error, None, "editing clears the error");

    state.client_secret = " GOCSPX-secret ".into();
    assert_eq!(
        state.submit(),
        Some(("id.apps".to_string(), "GOCSPX-secret".to_string()))
    );
}

#[test]
fn drive_setup_opens_with_stored_credentials_and_closes_on_escape() {
    let ctx = Context::default();
    let mut app = app_with_credentials("id", "secret", Some("token"));
    app.state.config_path = "/home/user/.config/emusync/config.json".into();

    app.open_drive_setup();
    let ActiveDialog::DriveSetup(state) = &app.active_dialog() else {
        panic!("drive dialog must open");
    };
    assert_eq!(
        (state.client_id.as_str(), state.client_secret.as_str()),
        ("id", "secret")
    );
    assert!(
        state.is_linked,
        "a stored refresh token shows the unlink action"
    );

    run_frames(&ctx, &mut app, vec![Vec::new(), escape()]);

    assert!(matches!(app.active_dialog(), ActiveDialog::None));
    assert!(
        app.take_actions().is_empty(),
        "closing must not connect or unlink"
    );
}

#[test]
fn save_and_connect_click_sends_trimmed_credentials_and_closes() {
    let ctx = Context::default();
    let mut app = app_with_credentials(" id ", " sec ", None);
    app.open_drive_setup();
    app.take_actions();

    click_button(&ctx, &mut app, "Save & Connect");

    assert_eq!(
        app.take_actions(),
        vec![UiAction::ConnectDrive {
            client_id: "id".to_string(),
            client_secret: "sec".to_string(),
        }]
    );
    assert_eq!(app.active_dialog(), &ActiveDialog::None);
}

#[test]
fn blank_credentials_click_keeps_dialog_with_error() {
    let ctx = Context::default();
    let mut app = app_with_credentials("id", "  ", None);
    app.open_drive_setup();
    app.take_actions();

    click_button(&ctx, &mut app, "Save & Connect");

    assert_eq!(app.take_actions(), Vec::<UiAction>::new());
    let ActiveDialog::DriveSetup(state) = app.active_dialog() else {
        panic!("the drive dialog must stay open");
    };
    assert_eq!(
        state.error,
        Some(emusync_presentation::dialogs::DriveSetupError::MissingCredentials)
    );
}

#[test]
fn unlink_click_sends_only_unlink_and_closes() {
    let ctx = Context::default();
    let mut app = app_with_credentials("id", "sec", Some("1//token"));
    app.open_drive_setup();
    app.take_actions();

    click_button(&ctx, &mut app, "Unlink Account");

    assert_eq!(app.take_actions(), vec![UiAction::UnlinkDrive]);
    assert_eq!(app.active_dialog(), &ActiveDialog::None);
}

#[test]
fn unlinked_dialog_offers_no_unlink() {
    for token in [None, Some("   ")] {
        let ctx = Context::default();
        let mut app = app_with_credentials("id", "sec", token);
        app.open_drive_setup();
        run_frames(&ctx, &mut app, (0..6).map(|_| Vec::new()).collect());

        assert_eq!(
            text_center(&ctx, &mut app, "Unlink Account"),
            None,
            "{token:?}"
        );
        assert!(
            text_center(&ctx, &mut app, "Save & Connect").is_some(),
            "the dialog is rendered"
        );
    }
}

#[test]
fn open_folder_keeps_the_dialog_open() {
    let ctx = Context::default();
    let mut app = app_with_credentials("id", "sec", None);
    app.open_drive_setup();
    app.take_actions();

    click_button(&ctx, &mut app, "Open Folder");

    assert_eq!(app.take_actions(), vec![UiAction::OpenConfigDirectory]);
    assert!(matches!(app.active_dialog(), ActiveDialog::DriveSetup(_)));
}

#[test]
fn open_drive_dialog_debug_never_shows_the_secret() {
    let mut app = app_with_credentials("id", "GOCSPX-x", Some("1//refresh"));
    app.open_drive_setup();

    let rendered = format!("{:?}", app.active_dialog());

    assert!(!rendered.contains("GOCSPX-x"), "{rendered}");
}
