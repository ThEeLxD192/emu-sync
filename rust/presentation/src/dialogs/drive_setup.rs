//! Google Drive setup dialog, laid out like Kotlin's `DriveSetupDialog`.

use crate::{
    components::{icons::paint_glyph, pill::Pill},
    dialogs::form_fields::{field_label, secret_field, text_button, text_field},
    theme::{
        BACKGROUND, DIVIDER, ERROR, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SUCCESS, SURFACE,
        SURFACE_VARIANT, bold, glyph,
    },
};
use egui::{
    Align, Color32, FontId, Frame, Id, Label, Layout, Margin, Modal, RichText, Sense, Stroke, Ui,
    vec2,
};

const MISSING_CREDENTIALS: &str = "Please enter both Client ID and Client Secret.";

/// State for the drive setup dialog.
#[derive(Clone, PartialEq)]
pub struct DriveSetupDialogState {
    pub client_id: String,
    pub client_secret: String,
    pub is_linked: bool,
    pub error_message: Option<String>,
    /// Rust-only: the secret is masked unless the user reveals it.
    pub secret_revealed: bool,
}

impl std::fmt::Debug for DriveSetupDialogState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DriveSetupDialogState")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("is_linked", &self.is_linked)
            .field("error_message", &self.error_message)
            .field("secret_revealed", &self.secret_revealed)
            .finish()
    }
}

impl DriveSetupDialogState {
    pub fn new(client_id: String, client_secret: String, is_linked: bool) -> Self {
        Self {
            client_id,
            client_secret,
            is_linked,
            error_message: None,
            secret_revealed: false,
        }
    }

    /// Trimmed credentials to save and connect with, or `None` after flagging that
    /// one of them is blank (Kotlin checks `isBlank`, then trims).
    pub fn submit(&mut self) -> Option<(String, String)> {
        if self.client_id.trim().is_empty() || self.client_secret.trim().is_empty() {
            self.error_message = Some(MISSING_CREDENTIALS.to_string());
            return None;
        }
        self.error_message = None;
        Some((
            self.client_id.trim().to_string(),
            self.client_secret.trim().to_string(),
        ))
    }

    /// Editing a field clears the previous error, as in Kotlin.
    pub fn field_edited(&mut self) {
        self.error_message = None;
    }
}

/// Callbacks the Drive dialog reports to the application.
pub struct DriveSetupEvents<'a> {
    pub on_save_connect: &'a mut dyn FnMut(String, String),
    pub on_unlink: &'a mut dyn FnMut(),
    pub on_open_config: &'a mut dyn FnMut(),
    pub on_close: &'a mut dyn FnMut(),
}

/// Renders the Google Drive setup dialog.
pub fn render_drive_setup_dialog(
    ctx: &egui::Context,
    state: &mut DriveSetupDialogState,
    config_path: &str,
    mut events: DriveSetupEvents<'_>,
) {
    let modal = Modal::new(Id::unique("drive_setup_dialog_modal"))
        .backdrop_color(Color32::from_black_alpha(160))
        .frame(
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, DIVIDER))
                .corner_radius(16)
                .inner_margin(Margin::same(24)),
        )
        .show(ctx, |ui| {
            ui.set_width(520.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            header(ui, state.is_linked);
            ui.add_space(16.0);
            info_banner(ui);
            ui.add_space(16.0);

            field_label(ui, "Client ID");
            if text_field(
                ui,
                "drive_client_id",
                &mut state.client_id,
                "xxxxxx.apps.googleusercontent.com",
                ui.available_width(),
                false,
            )
            .changed()
            {
                state.field_edited();
            }
            ui.add_space(12.0);
            field_label(ui, "Client Secret");
            if secret_field(
                ui,
                "drive_client_secret",
                &mut state.client_secret,
                "GOCSPX-xxxxxx",
                &mut state.secret_revealed,
            )
            .changed()
            {
                state.field_edited();
            }
            if let Some(error) = &state.error_message {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(12.0).color(ERROR));
            }
            ui.add_space(16.0);

            config_location(ui, config_path, events.on_open_config);
            ui.add_space(24.0);
            actions(ui, state, &mut events);
        });
    if modal.should_close() {
        (events.on_close)();
    }
}

fn header(ui: &mut Ui, is_linked: bool) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
        let (icon, color) = if is_linked {
            (glyph::CLOUD, SUCCESS)
        } else {
            (glyph::CLOUD_SYNC, PRIMARY)
        };
        paint_glyph(ui, rect.center(), icon, 28.0, color);
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Google Drive Sync")
                    .font(bold(20.0))
                    .color(ON_SURFACE),
            );
            let (status, color) = if is_linked {
                ("Account linked and active", SUCCESS)
            } else {
                (
                    "Configure cross-device save synchronization",
                    ON_SURFACE_DIM,
                )
            };
            ui.label(RichText::new(status).size(12.0).color(color));
        });
    });
}

fn info_banner(ui: &mut Ui) {
    Frame::new()
        .fill(SURFACE_VARIANT)
        .corner_radius(8)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                paint_glyph(ui, rect.center(), glyph::INFO, 18.0, PRIMARY);
                ui.add_space(10.0);
                ui.add(
                    Label::new(
                        RichText::new(
                            "Requires an OAuth 2.0 Client ID (Desktop App) from Google Cloud Console with the Google Drive API enabled.",
                        )
                        .size(12.0)
                        .color(ON_SURFACE_DIM),
                    )
                    .wrap(),
                );
            });
        });
}

fn config_location(ui: &mut Ui, config_path: &str, on_open_config: &mut dyn FnMut()) {
    Frame::new()
        .fill(BACKGROUND)
        .stroke(Stroke::new(1.0, DIVIDER))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let open = Pill {
                    icon: Some((glyph::FOLDER_OPEN, PRIMARY)),
                    text: "Open Folder",
                    text_color: PRIMARY,
                    fill: Color32::TRANSPARENT,
                    stroke: Stroke::new(1.0, DIVIDER),
                    padding: vec2(10.0, 4.0),
                    min_height: 32.0,
                    corner_radius: 16,
                    ..Pill::default()
                };
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if open.show(ui).clicked() {
                        on_open_config();
                    }
                    ui.add_space(8.0);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.label(
                            RichText::new("Config Location:")
                                .size(11.0)
                                .color(ON_SURFACE_DIM),
                        );
                        ui.add(
                            Label::new(
                                RichText::new(config_path)
                                    .font(FontId::monospace(12.0))
                                    .color(ON_SURFACE),
                            )
                            .truncate(),
                        )
                        .on_hover_text(config_path);
                    });
                });
            });
        });
}

fn actions(ui: &mut Ui, state: &mut DriveSetupDialogState, events: &mut DriveSetupEvents<'_>) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 40.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            if state.is_linked {
                let unlink = Pill {
                    icon: Some((glyph::LINK_OFF, ERROR)),
                    text: "Unlink Account",
                    text_color: ERROR,
                    font: FontId::proportional(13.0),
                    fill: Color32::TRANSPARENT,
                    padding: vec2(12.0, 0.0),
                    min_height: 40.0,
                    corner_radius: 20,
                    ..Pill::default()
                };
                if unlink.show(ui).clicked() {
                    (events.on_unlink)();
                    (events.on_close)();
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let connect = Pill {
                    icon: Some((glyph::CLOUD_SYNC, Color32::WHITE)),
                    text: "Save & Connect",
                    text_color: Color32::WHITE,
                    font: FontId::proportional(13.0),
                    fill: PRIMARY,
                    padding: vec2(20.0, 0.0),
                    min_height: 40.0,
                    corner_radius: 20,
                    ..Pill::default()
                };
                if connect.show(ui).clicked()
                    && let Some((client_id, client_secret)) = state.submit()
                {
                    (events.on_save_connect)(client_id, client_secret);
                    (events.on_close)();
                }
                if text_button(ui, "Cancel", ON_SURFACE_DIM).clicked() {
                    (events.on_close)();
                }
            });
        },
    );
}
