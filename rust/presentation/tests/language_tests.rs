//! The interface follows the configured language, and the header chip changes it.

mod common;

use common::*;
use egui::Context;
use emusync_domain::config::Language;
use emusync_presentation::{EmuSyncApp, UiAction};

fn app_in(language: Language) -> EmuSyncApp {
    let mut app = EmuSyncApp::new();
    let mut config = config_with(Vec::new(), None);
    config.language = language;
    app.state.config = Some(config);
    app
}

#[test]
fn the_interface_is_drawn_in_the_configured_language() {
    let ctx = Context::default();
    let mut app = app_in(Language::Spanish);

    assert!(text_exists(&ctx, &mut app, "Cerrar con seguridad"));
    assert!(!text_exists(&ctx, &mut app, "Safe Close"));
    assert!(text_exists(&ctx, &mut app, "ES"));

    app.state.config.as_mut().unwrap().language = Language::English;
    assert!(text_exists(&ctx, &mut app, "Safe Close"));
    assert!(text_exists(&ctx, &mut app, "EN"));
}

#[test]
fn choosing_a_language_in_the_header_asks_for_it() {
    let ctx = Context::default();
    let mut app = app_in(Language::English);

    let chip = text_center(&ctx, &mut app, "EN").expect("language chip");
    run_frames(&ctx, &mut app, click_at(chip));
    let spanish = text_center(&ctx, &mut app, "Español").expect("language list");
    run_frames(&ctx, &mut app, click_at(spanish));

    assert_eq!(
        app.take_actions()
            .into_iter()
            .filter(|action| matches!(action, UiAction::SetLanguage(_)))
            .collect::<Vec<_>>(),
        vec![UiAction::SetLanguage(Language::Spanish)]
    );
}
