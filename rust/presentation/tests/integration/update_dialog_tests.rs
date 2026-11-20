//! Update dialog states, their buttons and the header download indicator.

use crate::common;

use common::*;
use egui::Context;
use emusync_application::{DownloadFailure, UpdateState};
use emusync_domain::config::Language;
use emusync_presentation::{
    ActiveDialog, EmuSyncApp, UiAction,
    dialogs::{update_dialog_dismissible, update_failure_message},
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
        (
            UpdateState::DownloadFailed(DownloadFailure::Transfer("x".to_string())),
            "Close",
        ),
        (
            UpdateState::RestartFailed {
                downloaded_path: "/tmp/new.AppImage".to_string(),
            },
            "Close",
        ),
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
        UpdateState::CheckFailed("x".to_string()),
        UpdateState::DownloadFailed(DownloadFailure::Transfer("x".to_string())),
        UpdateState::RestartFailed {
            downloaded_path: "/tmp/new.AppImage".to_string(),
        },
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
fn download_and_restart_failures_are_explained_in_each_language() {
    let transfer = || {
        UpdateState::DownloadFailed(DownloadFailure::Transfer(
            "HTTP error: 503 Service Unavailable".to_string(),
        ))
    };
    let destination = || {
        UpdateState::DownloadFailed(DownloadFailure::DestinationUnavailable(
            "Permission denied (os error 13)".to_string(),
        ))
    };
    let unreadable = || {
        UpdateState::DownloadFailed(DownloadFailure::SizeUnreadable(
            "permission denied".to_string(),
        ))
    };
    let mismatch = || {
        UpdateState::DownloadFailed(DownloadFailure::SizeMismatch {
            expected_bytes: 4096,
            received_bytes: 2048,
        })
    };
    let restart = || UpdateState::RestartFailed {
        downloaded_path: "/tmp/EmuSync-update.AppImage".to_string(),
    };
    let table = [
        (
            Language::English,
            transfer(),
            "Failed to download update: HTTP error: 503 Service Unavailable",
        ),
        (
            Language::Spanish,
            transfer(),
            "No se pudo descargar la actualización: HTTP error: 503 Service Unavailable",
        ),
        (
            Language::English,
            destination(),
            "Failed to download update: could not prepare where to save it: Permission denied (os error 13)",
        ),
        (
            Language::Spanish,
            destination(),
            "No se pudo descargar la actualización: no se pudo preparar dónde guardarla: Permission denied (os error 13)",
        ),
        (
            Language::English,
            unreadable(),
            "Failed to download update: could not read the update size: permission denied",
        ),
        (
            Language::Spanish,
            unreadable(),
            "No se pudo descargar la actualización: no se pudo leer su tamaño: permission denied",
        ),
        (
            Language::English,
            mismatch(),
            "Failed to download update: the update size does not match the release: expected 4096 bytes, received 2048",
        ),
        (
            Language::Spanish,
            mismatch(),
            "No se pudo descargar la actualización: su tamaño no coincide con el de la versión publicada: se esperaban 4096 bytes y llegaron 2048",
        ),
        (
            Language::English,
            restart(),
            "Could not restart automatically. The update file is saved at: /tmp/EmuSync-update.AppImage",
        ),
        (
            Language::Spanish,
            restart(),
            "No se pudo reiniciar automáticamente. El archivo de la actualización está guardado en: /tmp/EmuSync-update.AppImage",
        ),
    ];
    for (language, state, message) in table {
        let ctx = Context::default();
        let mut app = in_language(app_showing(state), language);
        run_frames(&ctx, &mut app, (0..6).map(|_| Vec::new()).collect());

        assert!(
            text_exists(&ctx, &mut app, message),
            "{language:?}: {message}"
        );
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

/// Every text painted in one frame.
fn painted_texts(ctx: &Context, app: &mut EmuSyncApp) -> Vec<String> {
    let input = egui::RawInput {
        screen_rect: Some(screen()),
        ..egui::RawInput::default()
    };
    let output = ctx.run_ui(input, |ui| app.update(ui));
    let texts = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    texts
}

#[test]
fn download_and_restart_failures_in_spanish_show_no_english_and_close_with_cerrar() {
    let failures = [
        UpdateState::DownloadFailed(DownloadFailure::DestinationUnavailable(
            "os error 13".to_string(),
        )),
        UpdateState::DownloadFailed(DownloadFailure::Transfer("HTTP 503".to_string())),
        UpdateState::DownloadFailed(DownloadFailure::SizeUnreadable("os error 13".to_string())),
        UpdateState::DownloadFailed(DownloadFailure::SizeMismatch {
            expected_bytes: 4096,
            received_bytes: 2048,
        }),
        UpdateState::RestartFailed {
            downloaded_path: "/tmp/EmuSync-9.9.9.AppImage".to_string(),
        },
    ];
    let english_labels = [
        "Update Error",
        "Close",
        "Restart Now",
        "Update Now",
        "Later",
    ];
    let english_fragments = [
        "Failed to download",
        "Could not",
        "does not match",
        "expected",
        "received",
        "saved at",
        "automatically",
        "prepare",
    ];
    for state in failures {
        let ctx = Context::default();
        let mut app = in_language(app_showing(state.clone()), Language::Spanish);
        run_frames(&ctx, &mut app, (0..6).map(|_| Vec::new()).collect());

        let texts = painted_texts(&ctx, &mut app);
        assert!(
            texts.iter().any(|text| text == "Error al actualizar"),
            "{state:?}: {texts:?}"
        );
        for text in &texts {
            assert!(
                !english_labels.contains(&text.as_str()),
                "{state:?}: English label {text:?} in {texts:?}"
            );
            for fragment in english_fragments {
                assert!(
                    !text.contains(fragment),
                    "{state:?}: English {fragment:?} in {text:?}"
                );
            }
        }
        for offer in ["Reiniciar ahora", "Actualizar ahora"] {
            assert!(
                !texts.iter().any(|text| text == offer),
                "{state:?}: a failure offers no {offer:?}: {texts:?}"
            );
        }

        click_button(&ctx, &mut app, "Cerrar");

        assert_eq!(app.take_actions(), Vec::<UiAction>::new(), "{state:?}");
        assert_eq!(app.active_dialog(), &ActiveDialog::None, "{state:?}");
    }
}

#[test]
fn after_a_failed_download_or_restart_the_header_offers_a_new_manual_check_in_each_language() {
    let failures = [
        UpdateState::DownloadFailed(DownloadFailure::Transfer("HTTP 503".to_string())),
        UpdateState::DownloadFailed(DownloadFailure::SizeMismatch {
            expected_bytes: 4096,
            received_bytes: 2048,
        }),
        UpdateState::RestartFailed {
            downloaded_path: "/tmp/EmuSync-9.9.9.AppImage".to_string(),
        },
    ];
    for state in failures {
        for (language, check, restart) in [
            (Language::English, "Check Updates", "Restart EmuSync"),
            (
                Language::Spanish,
                "Buscar actualizaciones",
                "Reiniciar EmuSync",
            ),
        ] {
            let ctx = Context::default();
            let mut app = in_language(EmuSyncApp::new(), language);
            app.state.update_state = state.clone();
            run_frames(&ctx, &mut app, vec![Vec::new()]);
            assert!(
                !text_exists(&ctx, &mut app, restart),
                "{language:?} {state:?}: no restart is offered once it failed"
            );

            click_button(&ctx, &mut app, check);

            assert_eq!(
                app.take_actions(),
                vec![UiAction::CheckForUpdates { manual: true }],
                "{language:?} {state:?}"
            );
            assert_eq!(
                app.active_dialog(),
                &ActiveDialog::None,
                "{language:?} {state:?}"
            );
        }
    }
}

#[test]
fn states_that_are_not_failures_have_no_failure_message_in_any_language() {
    let not_failures = [
        UpdateState::Idle,
        UpdateState::Checking,
        UpdateState::Available(update_info("9.9.9")),
        downloading(0.5),
        ready(false),
        ready(true),
    ];
    for language in [Language::English, Language::Spanish] {
        let texts = emusync_presentation::i18n::texts(language);
        for state in &not_failures {
            assert_eq!(
                update_failure_message(texts, state),
                None,
                "{language:?} {state:?}: a manual check would show it as an error"
            );
        }
    }
}

#[test]
fn failure_details_that_look_like_placeholders_are_shown_literally() {
    let texts = emusync_presentation::i18n::texts(Language::Spanish);
    let table = [
        (
            UpdateState::DownloadFailed(DownloadFailure::DestinationUnavailable(
                "{detail}".to_string(),
            )),
            "No se pudo descargar la actualización: no se pudo preparar dónde guardarla: {detail}",
        ),
        (
            UpdateState::DownloadFailed(DownloadFailure::Transfer(
                "{detail} {expected} {received}".to_string(),
            )),
            "No se pudo descargar la actualización: {detail} {expected} {received}",
        ),
        (
            UpdateState::RestartFailed {
                downloaded_path: "/tmp/{path}/{detail}.AppImage".to_string(),
            },
            "No se pudo reiniciar automáticamente. El archivo de la actualización está guardado en: /tmp/{path}/{detail}.AppImage",
        ),
        (
            UpdateState::DownloadFailed(DownloadFailure::SizeMismatch {
                expected_bytes: u64::MAX,
                received_bytes: 0,
            }),
            "No se pudo descargar la actualización: su tamaño no coincide con el de la versión publicada: se esperaban 18446744073709551615 bytes y llegaron 0",
        ),
    ];
    for (state, message) in table {
        assert_eq!(
            update_failure_message(texts, &state).as_deref(),
            Some(message),
            "{state:?}"
        );
    }
}
