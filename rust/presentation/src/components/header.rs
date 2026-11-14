//! Top application header bar, laid out like the Kotlin `AppHeader`.

use crate::{
    actions::UiAction,
    components::pill::{LABEL_SIZE, Pill, pill},
    i18n::{self, Language, fill, tr},
    state::AppUiState,
    theme::{DIVIDER, ERROR, ON_SURFACE_DIM, PRIMARY, SUCCESS, SURFACE_VARIANT, bold, glyph},
};
use egui::{Align, Color32, FontId, Layout, Popup, RichText, Stroke, Ui, vec2};
use emusync_application::UpdateState;

/// Renders the header: branding on the left; Drive status, update chip, version and
/// the safe-close action on the right.
pub fn render_header(
    ui: &mut Ui,
    state: &AppUiState,
    on_action: &mut impl FnMut(UiAction),
    on_show_drive: &mut impl FnMut(),
    on_show_update: &mut impl FnMut(),
) {
    let t = tr(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(
            RichText::new("EmuSync")
                .font(bold(22.0))
                .color(PRIMARY)
                .extra_letter_spacing(0.5),
        );

        // Right-to-left: items are added from the window edge inwards.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if pill(
                ui,
                &Pill {
                    text: t.safe_close,
                    text_color: Color32::WHITE,
                    font: FontId::proportional(14.0),
                    fill: ERROR,
                    padding: vec2(16.0, 8.0),
                    min_height: 40.0,
                    corner_radius: 20,
                    ..Pill::default()
                },
            )
            .clicked()
            {
                on_action(UiAction::Exit);
            }

            ui.add_space(12.0);
            render_language_selector(ui, on_action);

            ui.add_space(16.0);
            ui.label(
                RichText::new(format!("v{}", state.current_version))
                    .size(LABEL_SIZE)
                    .color(ON_SURFACE_DIM),
            );

            ui.add_space(12.0);
            render_update_slot(ui, &state.update_state, on_action, on_show_update);

            ui.add_space(12.0);
            render_drive_status(ui, state, on_action, on_show_drive);
        });
    });
}

/// Action for the "Connect Drive" button, matching Kotlin: connect directly with stored
/// client credentials, or `None` to open the Drive setup dialog when either is blank.
pub fn connect_drive_action(state: &AppUiState) -> Option<UiAction> {
    let credentials = state.config.as_ref()?.credentials.as_ref()?;
    if credentials.client_id.trim().is_empty() || credentials.client_secret.trim().is_empty() {
        return None;
    }
    Some(UiAction::ConnectDrive {
        client_id: credentials.client_id.clone(),
        client_secret: credentials.client_secret.clone(),
    })
}

fn render_drive_status(
    ui: &mut Ui,
    state: &AppUiState,
    on_action: &mut impl FnMut(UiAction),
    on_show_drive: &mut impl FnMut(),
) {
    if state.is_drive_connected() {
        let chip = Pill {
            icon: Some((glyph::CLOUD, SUCCESS)),
            text: tr(ui.ctx()).drive_linked,
            fill: SURFACE_VARIANT,
            ..Pill::default()
        };
        if pill(ui, &chip).clicked() {
            on_show_drive();
        }
        return;
    }

    let button = Pill {
        icon: Some((glyph::CLOUD_SYNC, PRIMARY)),
        text: tr(ui.ctx()).connect_drive,
        text_color: PRIMARY,
        fill: Color32::TRANSPARENT,
        stroke: Stroke::new(1.0, DIVIDER),
        padding: vec2(12.0, 6.0),
        corner_radius: 16,
        ..Pill::default()
    };
    if pill(ui, &button).clicked() {
        match connect_drive_action(state) {
            Some(action) => on_action(action),
            None => on_show_drive(),
        }
    }
}

fn render_update_slot(
    ui: &mut Ui,
    update_state: &UpdateState,
    on_action: &mut impl FnMut(UiAction),
    on_show_update: &mut impl FnMut(),
) {
    let t = tr(ui.ctx());
    let chip = match update_state {
        UpdateState::Available(info) => Pill {
            icon: Some((glyph::UPDATE, Color32::WHITE)),
            text: &fill(t.update_to, &[("version", &info.version)]),
            text_color: Color32::WHITE,
            font: bold(LABEL_SIZE),
            fill: PRIMARY,
            ..Pill::default()
        }
        .show(ui),
        UpdateState::ReadyToRestart { .. } => Pill {
            icon: Some((glyph::RESTART, Color32::WHITE)),
            text: t.restart_app,
            text_color: Color32::WHITE,
            font: bold(LABEL_SIZE),
            fill: SUCCESS,
            ..Pill::default()
        }
        .show(ui),
        // Rust-only extras: Kotlin shows nothing in these states.
        UpdateState::Downloading { progress, .. } => Pill {
            icon: Some((glyph::DOWNLOAD, PRIMARY)),
            text: &fill(
                t.downloading_percent,
                &[(
                    "percent",
                    &crate::dialogs::update_dialog::progress_percent(*progress),
                )],
            ),
            text_color: PRIMARY,
            fill: SURFACE_VARIANT,
            ..Pill::default()
        }
        .show(ui),
        UpdateState::Checking => {
            ui.label(
                RichText::new(t.checking_updates)
                    .size(LABEL_SIZE)
                    .color(ON_SURFACE_DIM),
            );
            return;
        }
        UpdateState::Idle | UpdateState::Error(_) => {
            let check = Pill {
                text: t.check_updates,
                text_color: ON_SURFACE_DIM,
                fill: Color32::TRANSPARENT,
                stroke: Stroke::new(1.0, DIVIDER),
                ..Pill::default()
            };
            if pill(ui, &check).clicked() {
                on_action(UiAction::CheckForUpdates { manual: true });
            }
            return;
        }
    };
    if chip.clicked() {
        on_show_update();
    }
}

/// The language chip: shows the current language and opens the list to choose another.
fn render_language_selector(ui: &mut Ui, on_action: &mut impl FnMut(UiAction)) {
    let current = i18n::language(ui.ctx());
    let code = current.code().to_uppercase();
    let chip = Pill {
        icon: Some((glyph::TRANSLATE, ON_SURFACE_DIM)),
        text: &code,
        text_color: ON_SURFACE_DIM,
        fill: Color32::TRANSPARENT,
        stroke: Stroke::new(1.0, DIVIDER),
        ..Pill::default()
    };
    let response = pill(ui, &chip).on_hover_text(tr(ui.ctx()).language);
    Popup::menu(&response).show(|ui| {
        for language in Language::ALL {
            if ui
                .selectable_label(language == current, language.native_name())
                .clicked()
                && language != current
            {
                on_action(UiAction::SetLanguage(language));
            }
        }
    });
}
