//! Google Drive setup and connection dialog.

use crate::theme::{
    DIVIDER, ERROR, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SUCCESS, SURFACE,
};
use egui::{
    Button, Color32, CornerRadius, Frame, Id, Margin, Modal, RichText, Stroke, TextEdit, Vec2,
};

/// State for the drive setup dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct DriveSetupDialogState {
    pub client_id: String,
    pub client_secret: String,
    pub is_linked: bool,
    pub error_message: Option<String>,
}

impl DriveSetupDialogState {
    pub fn new(client_id: String, client_secret: String, is_linked: bool) -> Self {
        Self {
            client_id,
            client_secret,
            is_linked,
            error_message: None,
        }
    }
}

/// Renders the Google Drive setup dialog.
pub fn render_drive_setup_dialog(
    ctx: &egui::Context,
    state: &mut DriveSetupDialogState,
    on_save_connect: &mut impl FnMut(String, String),
    on_unlink: &mut impl FnMut(),
    on_open_config: &mut impl FnMut(),
    on_close: &mut impl FnMut(),
) {
    Modal::new(Id::unique("drive_setup_dialog_modal"))
        .backdrop_color(Color32::from_black_alpha(160))
        .frame(
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, DIVIDER))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(24)),
        )
        .show(ctx, |ui| {
            ui.set_width(450.0);

            ui.vertical(|ui| {
                // Header
                ui.horizontal(|ui| {
                    let (status_text, status_color) = if state.is_linked {
                        ("Account Linked and Active", SUCCESS)
                    } else {
                        ("Configure Cross-Device Save Synchronization", ON_SURFACE_DIM)
                    };
                    ui.label(RichText::new(status_text).size(13.0).color(status_color));
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                ui.label(
                    RichText::new(
                        "EmuSync uses your Google Drive to sync game saves automatically across your devices.",
                    )
                    .size(12.0)
                    .color(ON_SURFACE),
                );

                ui.add_space(12.0);

                // Client ID
                ui.label(RichText::new("OAuth Client ID:").size(12.0).strong().color(ON_BACKGROUND));
                ui.add(
                    TextEdit::singleline(&mut state.client_id)
                        .hint_text("Enter OAuth Client ID")
                        .desired_width(f32::INFINITY),
                );

                ui.add_space(8.0);

                // Client Secret
                ui.label(RichText::new("OAuth Client Secret:").size(12.0).strong().color(ON_BACKGROUND));
                ui.add(
                    TextEdit::singleline(&mut state.client_secret)
                        .hint_text("Enter OAuth Client Secret")
                        .password(true)
                        .desired_width(f32::INFINITY),
                );

                if let Some(err) = &state.error_message {
                    ui.add_space(8.0);
                    ui.label(RichText::new(err).size(12.0).color(ERROR));
                }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(12.0);

                // Action buttons
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            Button::new(RichText::new("Open Config Folder").size(11.0).color(ON_SURFACE_DIM))
                                .min_size(Vec2::new(130.0, 26.0)),
                        )
                        .clicked()
                    {
                        on_open_config();
                    }

                    if state.is_linked
                        && ui
                            .add(
                                Button::new(RichText::new("Unlink").size(11.0).color(ERROR))
                                    .min_size(Vec2::new(70.0, 26.0)),
                            )
                            .clicked()
                    {
                        on_unlink();
                        on_close();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                Button::new(
                                    RichText::new(if state.is_linked { "Re-Connect" } else { "Connect" })
                                        .size(12.0)
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(PRIMARY)
                                .min_size(Vec2::new(90.0, 26.0)),
                            )
                            .clicked()
                        {
                            let id = state.client_id.trim().to_string();
                            let secret = state.client_secret.trim().to_string();
                            if id.is_empty() || secret.is_empty() {
                                state.error_message = Some("Client ID and Client Secret are required.".to_string());
                            } else {
                                state.error_message = None;
                                on_save_connect(id, secret);
                                on_close();
                            }
                        }

                        if ui
                            .add(
                                Button::new(RichText::new("Cancel").size(12.0).color(ON_SURFACE))
                                    .min_size(Vec2::new(70.0, 26.0)),
                            )
                            .clicked()
                        {
                            on_close();
                        }
                    });
                });
            });
        });
}
