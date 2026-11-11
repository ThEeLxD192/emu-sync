//! Update dialog states, their buttons and the header download indicator.

mod common;

use common::*;
use egui::Context;
use emusync_application::UpdateState;
use emusync_presentation::{
    ActiveDialog, EmuSyncApp, UiAction, dialogs::update_dialog_dismissible,
};
use std::path::PathBuf;

fn ready(steam: bool) -> UpdateState {
    UpdateState::ReadyToRestart {
        downloaded_path: "/tmp/new.AppImage".to_string(),
        is_steam_game_mode: steam,
    }
}

fn downloading(progress: f32) -> UpdateState {
    UpdateState::Downloading {
        progress,
        info: update_info("9.9.9"),
    }
}

fn app_showing(state: UpdateState) -> EmuSyncApp {
    let mut app = EmuSyncApp::new();
    app.state.update_state = state;
    app.open_update_dialog();
    app
}

#[test]
fn app_opens_update_dialog_and_emits_download_action() {
    let info = update_info("1.3.0");
    let mut app = app_showing(UpdateState::Available(info.clone()));
    assert_eq!(app.active_dialog(), &ActiveDialog::Update);

    let ctx = Context::default();
    click_button(&ctx, &mut app, "Update Now");
    assert_eq!(app.take_actions(), vec![UiAction::DownloadUpdate(info)]);
}

#[test]
fn update_dialog_cannot_be_dismissed_while_downloading() {
    let info = update_info("9.9.9");
    for (state, closes) in [
        (
            UpdateState::Downloading {
                progress: 0.5,
                info: info.clone(),
            },
            false,
        ),
        (UpdateState::Available(info), true),
    ] {
        let ctx = Context::default();
        let mut app = app_showing(state.clone());

        run_frames(&ctx, &mut app, vec![Vec::new(), escape()]);

        assert_eq!(
            matches!(app.active_dialog(), ActiveDialog::None),
            closes,
            "Escape with {state:?}"
        );
        assert!(app.take_actions().is_empty());
    }
}

#[test]
fn ready_to_restart_sends_the_downloaded_path() {
    for (steam, button) in [(false, "Restart Now"), (true, "Close to Apply")] {
        let ctx = Context::default();
        let mut app = app_showing(ready(steam));
        app.take_actions();

        click_button(&ctx, &mut app, button);

        assert_eq!(
            app.take_actions(),
            vec![UiAction::RestartApp(PathBuf::from("/tmp/new.AppImage"))],
            "{button}"
        );
    }
}

#[test]
fn restart_later_and_error_close_emit_nothing_and_close() {
    let table = [
        (ready(false), "Restart Later"),
        (ready(true), "Later"),
        (UpdateState::Error("x".to_string()), "Close"),
        (UpdateState::Idle, "Close"),
    ];
    for (state, button) in table {
        let ctx = Context::default();
        let mut app = app_showing(state.clone());
        app.take_actions();

        click_button(&ctx, &mut app, button);

        assert_eq!(app.take_actions(), Vec::<UiAction>::new(), "{state:?}");
        assert_eq!(app.active_dialog(), &ActiveDialog::None, "{state:?}");
    }
}

#[test]
fn downloading_dialog_offers_no_actions() {
    let ctx = Context::default();
    let mut app = app_showing(downloading(0.5));
    run_frames(&ctx, &mut app, (0..6).map(|_| Vec::new()).collect());

    assert!(text_exists(&ctx, &mut app, "Downloading update..."));
    for label in ["Update Now", "Later", "Close"] {
        assert!(!text_exists(&ctx, &mut app, label), "{label}");
    }
    run_frames(&ctx, &mut app, vec![escape()]);
    assert_eq!(app.active_dialog(), &ActiveDialog::Update);
    assert_eq!(app.take_actions(), Vec::<UiAction>::new());
}

#[test]
fn header_download_percent_is_clamped_for_hostile_progress() {
    let table = [
        (1.7, "Downloading 100%"),
        (-0.3, "Downloading 0%"),
        (f32::NAN, "Downloading 0%"),
        (0.505, "Downloading 51%"),
    ];
    for (progress, label) in table {
        let ctx = Context::default();
        let mut app = EmuSyncApp::new();
        app.state.update_state = downloading(progress);
        run_frames(&ctx, &mut app, vec![Vec::new()]);

        assert!(text_exists(&ctx, &mut app, label), "{progress} -> {label}");
    }
}

#[test]
fn dismissible_only_when_not_downloading() {
    assert!(!update_dialog_dismissible(&downloading(0.0)));
    for state in [
        UpdateState::Idle,
        UpdateState::Checking,
        UpdateState::Available(update_info("1.0.0")),
        ready(false),
        UpdateState::Error("x".to_string()),
    ] {
        assert!(update_dialog_dismissible(&state), "{state:?}");
    }
}
