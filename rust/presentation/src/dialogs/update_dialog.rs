//! Update dialog, laid out like Kotlin's `UpdateDialog`.

use crate::{
    components::{icons::paint_glyph, pill::Pill},
    dialogs::form_fields::{primary_button, text_button},
    theme::{
        DIVIDER, ERROR, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, PRIMARY_CONTAINER, SURFACE,
        SURFACE_VARIANT, bold, glyph,
    },
};
use egui::{
    Align, Color32, FontId, Frame, Id, Label, Layout, Margin, Modal, RichText, ScrollArea, Sense,
    Stroke, Ui, vec2,
};
use emusync_application::UpdateState;
use emusync_domain::update::UpdateInfo;
use std::path::PathBuf;

/// Callbacks the update dialog reports to the application.
pub struct UpdateDialogEvents<'a> {
    pub on_download: &'a mut dyn FnMut(UpdateInfo),
    pub on_restart: &'a mut dyn FnMut(PathBuf),
    pub on_close: &'a mut dyn FnMut(),
}

/// Whether Escape or a click outside may close the dialog: never while downloading,
/// as Kotlin's `onDismissRequest`.
pub fn update_dialog_dismissible(state: &UpdateState) -> bool {
    !matches!(state, UpdateState::Downloading { .. })
}

/// Renders the update dialog for `state`.
pub fn render_update_dialog(
    ctx: &egui::Context,
    state: &UpdateState,
    current_version: &str,
    events: UpdateDialogEvents<'_>,
) {
    let UpdateDialogEvents {
        on_download,
        on_restart,
        on_close,
    } = events;
    let modal = Modal::new(Id::unique("update_dialog_modal"))
        .backdrop_color(Color32::from_black_alpha(160))
        .frame(
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, DIVIDER))
                .corner_radius(16)
                .inner_margin(Margin::same(24)),
        )
        .show(ctx, |ui| {
            ui.set_width(460.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            match state {
                UpdateState::Available(info) => {
                    icon_header(
                        ui,
                        glyph::UPDATE,
                        "Update Available",
                        &format!("Version {} (current: {current_version})", info.version),
                    );
                    ui.add_space(16.0);
                    ui.label(RichText::new("Release Notes:").font(bold(12.0)).color(ON_SURFACE));
                    ui.add_space(6.0);
                    Frame::new()
                        .fill(SURFACE_VARIANT)
                        .corner_radius(8)
                        .inner_margin(Margin::same(12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                                ui.add(
                                    Label::new(
                                        RichText::new(&info.release_notes)
                                            .size(12.0)
                                            .color(ON_SURFACE),
                                    )
                                    .wrap(),
                                );
                            });
                        });
                    ui.add_space(20.0);
                    button_row(ui, |ui| {
                        let update_now = Pill {
                            icon: Some((glyph::DOWNLOAD, Color32::WHITE)),
                            text: "Update Now",
                            text_color: Color32::WHITE,
                            font: FontId::proportional(13.0),
                            fill: PRIMARY,
                            padding: vec2(20.0, 0.0),
                            min_height: 40.0,
                            corner_radius: 20,
                            ..Pill::default()
                        };
                        if update_now.show(ui).clicked() {
                            on_download(info.clone());
                        }
                        if text_button(ui, "Later", PRIMARY).clicked() {
                            on_close();
                        }
                    });
                }
                UpdateState::Downloading { progress, info } => {
                    ui.label(
                        RichText::new("Downloading update...")
                            .font(bold(15.0))
                            .color(ON_SURFACE),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("Downloading version {} from GitHub", info.version))
                            .size(12.0)
                            .color(ON_SURFACE_DIM),
                    );
                    ui.add_space(16.0);
                    progress_bar(ui, *progress);
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        ui.label(
                            RichText::new(format!("{}%", progress_percent(*progress)))
                                .size(12.0)
                                .color(ON_SURFACE_DIM),
                        );
                    });
                }
                UpdateState::ReadyToRestart {
                    downloaded_path,
                    is_steam_game_mode,
                } => {
                    let steam = *is_steam_game_mode;
                    icon_header(
                        ui,
                        glyph::RESTART,
                        "Update Complete!",
                        if steam {
                            "The new version is ready to apply."
                        } else {
                            "The new version is ready to install."
                        },
                    );
                    ui.add_space(16.0);
                    ui.add(
                        Label::new(
                            RichText::new(if steam {
                                "In Steam Deck Game Mode, closing EmuSync applies the update. You can then relaunch it from Steam to start the new version."
                            } else {
                                "To apply changes and enjoy the new version, restart EmuSync."
                            })
                            .size(13.0)
                            .color(ON_SURFACE),
                        )
                        .wrap(),
                    );
                    ui.add_space(20.0);
                    button_row(ui, |ui| {
                        if primary_button(ui, if steam { "Close to Apply" } else { "Restart Now" })
                            .clicked()
                        {
                            on_restart(PathBuf::from(downloaded_path));
                        }
                        if text_button(ui, if steam { "Later" } else { "Restart Later" }, PRIMARY)
                            .clicked()
                        {
                            on_close();
                        }
                    });
                }
                UpdateState::Error(message) => {
                    ui.label(RichText::new("Update Error").font(bold(15.0)).color(ERROR));
                    ui.add_space(8.0);
                    ui.add(Label::new(RichText::new(message).size(13.0).color(ON_SURFACE)).wrap());
                    ui.add_space(16.0);
                    button_row(ui, |ui| {
                        if primary_button(ui, "Close").clicked() {
                            on_close();
                        }
                    });
                }
                // Rust-only states, reached from the "Check Updates" extra.
                UpdateState::Checking => {
                    ui.label(
                        RichText::new("Checking for updates...")
                            .size(13.0)
                            .color(PRIMARY),
                    );
                }
                UpdateState::Idle => {
                    ui.label(
                        RichText::new("You are on the latest version of EmuSync.")
                            .size(13.0)
                            .color(ON_SURFACE),
                    );
                    ui.add_space(16.0);
                    button_row(ui, |ui| {
                        if primary_button(ui, "Close").clicked() {
                            on_close();
                        }
                    });
                }
            }
        });
    if modal.should_close() && update_dialog_dismissible(state) {
        on_close();
    }
}

fn icon_header(ui: &mut Ui, icon: &str, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
        ui.painter().rect_filled(rect, 10, PRIMARY_CONTAINER);
        paint_glyph(ui, rect.center(), icon, 24.0, PRIMARY);
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(title).font(bold(20.0)).color(ON_SURFACE));
            ui.label(RichText::new(subtitle).size(12.0).color(ON_SURFACE_DIM));
        });
    });
}

fn button_row(ui: &mut Ui, add_buttons: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 40.0),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            add_buttons(ui);
        },
    );
}

fn progress_bar(ui: &mut Ui, progress: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
    ui.painter().rect_filled(rect, 4, SURFACE_VARIANT);
    let filled = egui::Rect::from_min_size(
        rect.min,
        vec2(rect.width() * progress.clamp(0.0, 1.0), rect.height()),
    );
    ui.painter().rect_filled(filled, 4, PRIMARY);
}

/// Whole-number percentage of a download progress, clamped to 0–100.
pub(crate) fn progress_percent(progress: f32) -> u32 {
    (progress.clamp(0.0, 1.0) * 100.0).round() as u32
}
