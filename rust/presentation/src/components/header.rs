//! Top application header bar.

use crate::{
    actions::UiAction,
    state::AppUiState,
    theme::{DIVIDER, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SUCCESS, SURFACE},
};
use egui::{Align, Button, Color32, Frame, Layout, Margin, RichText, Stroke, Ui, Vec2};
use emusync_application::UpdateState;

/// Renders the top header bar with branding, cloud status, updates, and window actions.
pub fn render_header(
    ui: &mut Ui,
    state: &AppUiState,
    on_action: &mut impl FnMut(UiAction),
    on_show_drive: &mut impl FnMut(),
    on_show_update: &mut impl FnMut(),
) {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, DIVIDER))
        .inner_margin(Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Title and version
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("EmuSync")
                            .size(18.0)
                            .strong()
                            .color(ON_BACKGROUND),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("v{}", state.current_version))
                            .size(11.0)
                            .color(ON_SURFACE_DIM),
                    );
                });

                // Right aligned actions
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Exit button
                    if ui
                        .add(
                            Button::new(RichText::new("Exit").size(12.0).color(ON_SURFACE_DIM))
                                .min_size(Vec2::new(48.0, 26.0)),
                        )
                        .clicked()
                    {
                        on_action(UiAction::Exit);
                    }

                    ui.add_space(10.0);

                    // Update action / status
                    match &state.update_state {
                        UpdateState::Available(info) => {
                            if ui
                                .add(
                                    Button::new(
                                        RichText::new(format!(
                                            "Update Available (v{})",
                                            info.version
                                        ))
                                        .size(12.0)
                                        .strong()
                                        .color(Color32::WHITE),
                                    )
                                    .fill(PRIMARY)
                                    .min_size(Vec2::new(140.0, 26.0)),
                                )
                                .clicked()
                            {
                                on_show_update();
                            }
                        }
                        UpdateState::Downloading { progress, .. } => {
                            let percent = (*progress * 100.0) as u32;
                            if ui
                                .add(
                                    Button::new(
                                        RichText::new(format!("Downloading ({percent}%)"))
                                            .size(12.0)
                                            .color(PRIMARY),
                                    )
                                    .min_size(Vec2::new(130.0, 26.0)),
                                )
                                .clicked()
                            {
                                on_show_update();
                            }
                        }
                        UpdateState::ReadyToRestart { .. } => {
                            if ui
                                .add(
                                    Button::new(
                                        RichText::new("Restart to update")
                                            .size(12.0)
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .fill(SUCCESS)
                                    .min_size(Vec2::new(130.0, 26.0)),
                                )
                                .clicked()
                            {
                                on_show_update();
                            }
                        }
                        UpdateState::Checking => {
                            ui.label(
                                RichText::new("Checking updates...")
                                    .size(12.0)
                                    .color(ON_SURFACE_DIM),
                            );
                        }
                        _ => {
                            if ui
                                .add(
                                    Button::new(
                                        RichText::new("Check Updates").size(12.0).color(ON_SURFACE),
                                    )
                                    .min_size(Vec2::new(100.0, 26.0)),
                                )
                                .clicked()
                            {
                                on_action(UiAction::CheckForUpdates { manual: true });
                            }
                        }
                    }

                    ui.add_space(10.0);

                    // Google Drive button
                    let (drive_text, drive_color) = if state.is_drive_connected() {
                        ("Google Drive: Connected", SUCCESS)
                    } else {
                        ("Connect Google Drive", PRIMARY)
                    };

                    if ui
                        .add(
                            Button::new(RichText::new(drive_text).size(12.0).color(drive_color))
                                .min_size(Vec2::new(140.0, 26.0)),
                        )
                        .clicked()
                    {
                        on_show_drive();
                    }
                });
            });
        });
}
