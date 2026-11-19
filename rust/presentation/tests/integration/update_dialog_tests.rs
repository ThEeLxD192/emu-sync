//! Update dialog states, their buttons and the header download indicator.

use crate::common;

use common::*;
use egui::Context;
use emusync_application::UpdateState;
use emusync_domain::config::Language;
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

fn in_language(mut app: EmuSyncApp, language: Language) -> EmuSyncApp {
    let mut config = config_with(Vec::new(), None);
    config.language = language;
    app.state.config = Some(config);
    app
}

#[test]
fn a_failed_check_is_explained_in_each_language_and_closes_without_actions() {
    let table = [
        (
            Language::English,
            "Update Error",
            "Could not check for updates: HTTP 503",
            "Close",
            [
                "Update Now",
                "Restart Now",
                "You are on the latest version of EmuSync.",
            ],
        ),
        (
            Language::Spanish,
            "Error al actualizar",
            "No se pudo comprobar si hay actualizaciones: HTTP 503",
            "Cerrar",
            [
                "Actualizar ahora",
                "Reiniciar ahora",
                "Ya tienes la última versión de EmuSync.",
            ],
        ),
    ];
    for (language, title, message, close, absent_texts) in table {
        let ctx = Context::default();
        let mut app = in_language(
            app_showing(UpdateState::CheckFailed("HTTP 503".to_string())),
            language,
        );
        run_frames(&ctx, &mut app, (0..6).map(|_| Vec::new()).collect());

        assert!(text_exists(&ctx, &mut app, title), "{language:?}: {title}");
        assert!(
            text_exists(&ctx, &mut app, message),
            "{language:?}: {message}"
        );
        assert!(
            !text_exists(&ctx, &mut app, "HTTP 503"),
            "{language:?}: the detail is shown inside the sentence, not alone"
        );
        for absent in absent_texts {
            assert!(
                !text_exists(&ctx, &mut app, absent),
                "{language:?}: {absent}"
            );
        }

        click_button(&ctx, &mut app, close);

        assert_eq!(app.take_actions(), Vec::<UiAction>::new(), "{language:?}");
        assert_eq!(app.active_dialog(), &ActiveDialog::None, "{language:?}");
    }
}

#[test]
fn a_failed_check_dialog_closes_with_escape() {
    let failed = UpdateState::CheckFailed("Network error: reset".to_string());
    assert!(update_dialog_dismissible(&failed));

    let ctx = Context::default();
    let mut app = app_showing(failed);
    run_frames(&ctx, &mut app, vec![Vec::new(), escape()]);

    assert_eq!(app.active_dialog(), &ActiveDialog::None);
    assert_eq!(app.take_actions(), Vec::<UiAction>::new());
}

#[test]
fn after_a_failed_check_the_header_offers_a_new_manual_check_in_each_language() {
    for (language, button) in [
        (Language::English, "Check Updates"),
        (Language::Spanish, "Buscar actualizaciones"),
    ] {
        let ctx = Context::default();
        let mut app = in_language(EmuSyncApp::new(), language);
        app.state.update_state = UpdateState::CheckFailed("HTTP 503".to_string());

        click_button(&ctx, &mut app, button);

        assert_eq!(
            app.take_actions(),
            vec![UiAction::CheckForUpdates { manual: true }],
            "{language:?}"
        );
        assert_eq!(app.active_dialog(), &ActiveDialog::None, "{language:?}");
    }
}
