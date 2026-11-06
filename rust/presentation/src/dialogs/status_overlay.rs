//! Status modal overlays for sync progress, gameplay, conflicts, and errors.

use crate::{
    model::AppStatus,
    theme::{
        DIVIDER, ERROR, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SUCCESS, SURFACE,
        WARNING,
    },
};
use egui::{Align2, Button, Color32, CornerRadius, Frame, RichText, Stroke, Vec2, Window};
use emusync_domain::sync::Decision;

/// Renders the status overlay over the application based on current AppStatus.
pub fn render_status_overlay(
    ctx: &egui::Context,
    status: &AppStatus,
    on_resolve_conflict: &mut impl FnMut(Decision),
    on_retry_post_sync: &mut impl FnMut(),
    on_dismiss: &mut impl FnMut(),
) {
    match status {
        AppStatus::Syncing { message } => {
            Window::new("Syncing")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::new()
                        .fill(SURFACE)
                        .stroke(Stroke::new(1.0, PRIMARY))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(24),
                )
                .show(ctx, |ui| {
                    ui.set_width(320.0);
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(message)
                                .size(14.0)
                                .strong()
                                .color(ON_BACKGROUND),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("Communicating with Google Drive...")
                                .size(11.0)
                                .color(ON_SURFACE_DIM),
                        );
                    });
                });
        }
        AppStatus::Playing => {
            Window::new("Playing")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::new()
                        .fill(SURFACE)
                        .stroke(Stroke::new(1.0, SUCCESS))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(24),
                )
                .show(ctx, |ui| {
                    ui.set_width(360.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🎮 Game Running").size(16.0).strong().color(SUCCESS));
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(
                                "The game is currently active. EmuSync will sync saves to the cloud when you finish and close it.",
                            )
                            .size(12.0)
                            .color(ON_SURFACE),
                        );
                    });
                });
        }
        AppStatus::Loading => {
            Window::new("Loading")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::new()
                        .fill(SURFACE)
                        .stroke(Stroke::new(1.0, DIVIDER))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(24),
                )
                .show(ctx, |ui| {
                    ui.set_width(260.0);
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("Loading library...")
                                .size(13.0)
                                .color(ON_SURFACE),
                        );
                    });
                });
        }
        AppStatus::Conflict {
            game_name,
            local_date,
            cloud_date,
        } => {
            Window::new("Save Conflict Detected")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::new()
                        .fill(SURFACE)
                        .stroke(Stroke::new(1.0, WARNING))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(24),
                )
                .show(ctx, |ui| {
                    ui.set_width(460.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Save Conflict Detected").size(16.0).strong().color(WARNING));
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!(
                                "A conflict occurred for \"{game_name}\". Both your local device and the cloud have newer saves."
                            ))
                            .size(12.0)
                            .color(ON_SURFACE),
                        );

                        ui.add_space(12.0);

                        Frame::new()
                            .fill(SURFACE)
                            .stroke(Stroke::new(1.0, DIVIDER))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(12)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new("💻 Local Device").size(12.0).strong().color(ON_BACKGROUND));
                                        ui.label(RichText::new(format!("Modified: {local_date}")).size(11.0).color(ON_SURFACE_DIM));
                                    });
                                    ui.add_space(20.0);
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new("☁ Google Drive").size(12.0).strong().color(ON_BACKGROUND));
                                        ui.label(RichText::new(format!("Modified: {cloud_date}")).size(11.0).color(ON_SURFACE_DIM));
                                    });
                                });
                            });

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(12.0);

                        ui.horizontal(|ui| {
                            if ui
                                .add(
                                    Button::new(RichText::new("Keep Local (Upload)").size(12.0).strong().color(Color32::WHITE))
                                        .fill(PRIMARY)
                                        .min_size(Vec2::new(180.0, 30.0)),
                                )
                                .clicked()
                            {
                                on_resolve_conflict(Decision::UploadLocal);
                            }

                            if ui
                                .add(
                                    Button::new(RichText::new("Keep Cloud (Download)").size(12.0).strong().color(Color32::WHITE))
                                        .fill(WARNING)
                                        .min_size(Vec2::new(180.0, 30.0)),
                                )
                                .clicked()
                            {
                                on_resolve_conflict(Decision::DownloadCloud);
                            }
                        });
                    });
                });
        }
        AppStatus::PostSyncFailed {
            game_name,
            reason,
            completed_save_count,
            pending_save_count,
        } => {
            Window::new("Save Kept Locally")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::new()
                        .fill(SURFACE)
                        .stroke(Stroke::new(1.0, ERROR))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(24),
                )
                .show(ctx, |ui| {
                    ui.set_width(440.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Save Kept Locally").size(16.0).strong().color(ERROR));
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!(
                                "Could not upload save for \"{game_name}\" to the cloud. Your local progress is safely preserved."
                            ))
                            .size(12.0)
                            .color(ON_SURFACE),
                        );

                        ui.add_space(8.0);
                        ui.label(RichText::new(format!("Reason: {reason}")).size(11.0).color(ON_SURFACE_DIM));
                        ui.label(
                            RichText::new(format!(
                                "Progress: {completed_save_count} uploaded, {pending_save_count} pending"
                            ))
                            .size(11.0)
                            .color(ON_SURFACE_DIM),
                        );

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(12.0);

                        ui.horizontal(|ui| {
                            if ui
                                .add(
                                    Button::new(RichText::new("Retry Upload").size(12.0).strong().color(Color32::WHITE))
                                        .fill(PRIMARY)
                                        .min_size(Vec2::new(110.0, 28.0)),
                                )
                                .clicked()
                            {
                                on_retry_post_sync();
                            }

                            if ui
                                .add(Button::new(RichText::new("Dismiss").size(12.0).color(ON_SURFACE)).min_size(Vec2::new(80.0, 28.0)))
                                .clicked()
                            {
                                on_dismiss();
                            }
                        });
                    });
                });
        }
        AppStatus::Error { message } => {
            Window::new("Error")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::new()
                        .fill(SURFACE)
                        .stroke(Stroke::new(1.0, ERROR))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(24),
                )
                .show(ctx, |ui| {
                    ui.set_width(400.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("An Error Occurred")
                                .size(15.0)
                                .strong()
                                .color(ERROR),
                        );
                        ui.add_space(8.0);
                        ui.label(RichText::new(message).size(12.0).color(ON_SURFACE));

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(12.0);

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    Button::new(
                                        RichText::new("Dismiss").size(12.0).color(ON_SURFACE),
                                    )
                                    .min_size(Vec2::new(80.0, 28.0)),
                                )
                                .clicked()
                            {
                                on_dismiss();
                            }
                        });
                    });
                });
        }
        AppStatus::Idle => {}
    }
}
