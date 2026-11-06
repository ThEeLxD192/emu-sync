//! Dialog showing update details, progress bar, and restart confirmation.

use crate::theme::{
    DIVIDER, ERROR, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SUCCESS, SURFACE,
};
use egui::{
    Align2, Button, Color32, CornerRadius, Frame, Margin, ProgressBar, RichText, ScrollArea,
    Stroke, Vec2, Window,
};
use emusync_application::UpdateState;
use emusync_domain::update::UpdateInfo;
use std::path::PathBuf;

/// Renders the Software Update dialog.
pub fn render_update_dialog(
    ctx: &egui::Context,
    state: &UpdateState,
    current_version: &str,
    on_download: &mut impl FnMut(UpdateInfo),
    on_restart: &mut impl FnMut(PathBuf),
    on_close: &mut impl FnMut(),
) {
    Window::new("Software Update")
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, DIVIDER))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(24)),
        )
        .show(ctx, |ui| {
            ui.set_width(460.0);

            ui.vertical(|ui| match state {
                UpdateState::Available(info) => {
                    ui.label(
                        RichText::new("A new version of EmuSync is available!")
                            .size(15.0)
                            .strong()
                            .color(ON_BACKGROUND),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "Version: {} (Current: {})",
                            info.version, current_version
                        ))
                        .size(12.0)
                        .color(PRIMARY),
                    );

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    ui.label(
                        RichText::new("Release Notes:")
                            .size(12.0)
                            .strong()
                            .color(ON_SURFACE),
                    );
                    ui.add_space(4.0);

                    ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                        let body = if info.release_notes.trim().is_empty() {
                            "No release notes provided."
                        } else {
                            &info.release_notes
                        };
                        ui.label(RichText::new(body).size(11.0).color(ON_SURFACE_DIM));
                    });

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let info_clone = info.clone();
                            if ui
                                .add(
                                    Button::new(
                                        RichText::new("Download and Update")
                                            .size(12.0)
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .fill(PRIMARY)
                                    .min_size(Vec2::new(160.0, 28.0)),
                                )
                                .clicked()
                            {
                                on_download(info_clone);
                            }

                            if ui
                                .add(
                                    Button::new(
                                        RichText::new("Later").size(12.0).color(ON_SURFACE),
                                    )
                                    .min_size(Vec2::new(70.0, 28.0)),
                                )
                                .clicked()
                            {
                                on_close();
                            }
                        });
                    });
                }
                UpdateState::Downloading { progress, info } => {
                    ui.label(
                        RichText::new("Downloading Update...")
                            .size(15.0)
                            .strong()
                            .color(ON_BACKGROUND),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("Downloading version {}", info.version))
                            .size(12.0)
                            .color(ON_SURFACE_DIM),
                    );

                    ui.add_space(16.0);
                    ui.add(ProgressBar::new(*progress).show_percentage().animate(true));

                    ui.add_space(16.0);
                    ui.label(
                        RichText::new("Please wait while the update is safely downloaded.")
                            .size(11.0)
                            .color(ON_SURFACE_DIM),
                    );
                }
                UpdateState::ReadyToRestart {
                    downloaded_path,
                    is_steam_game_mode,
                } => {
                    ui.label(
                        RichText::new("Update Ready to Install!")
                            .size(15.0)
                            .strong()
                            .color(SUCCESS),
                    );
                    ui.add_space(4.0);

                    if *is_steam_game_mode {
                        ui.label(
                            RichText::new(
                                "Update applied. SteamOS will launch the new version when closed.",
                            )
                            .size(12.0)
                            .color(ON_SURFACE),
                        );
                    } else {
                        ui.label(
                            RichText::new(
                                "Click restart to apply the update and relaunch EmuSync.",
                            )
                            .size(12.0)
                            .color(ON_SURFACE),
                        );
                    }

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let path = PathBuf::from(downloaded_path);
                            if ui
                                .add(
                                    Button::new(
                                        RichText::new("Restart Now")
                                            .size(12.0)
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .fill(SUCCESS)
                                    .min_size(Vec2::new(120.0, 28.0)),
                                )
                                .clicked()
                            {
                                on_restart(path);
                                on_close();
                            }

                            if ui
                                .add(
                                    Button::new(
                                        RichText::new("Later").size(12.0).color(ON_SURFACE),
                                    )
                                    .min_size(Vec2::new(70.0, 28.0)),
                                )
                                .clicked()
                            {
                                on_close();
                            }
                        });
                    });
                }
                UpdateState::Error(err) => {
                    ui.label(
                        RichText::new("Update Check Failed")
                            .size(15.0)
                            .strong()
                            .color(ERROR),
                    );
                    ui.add_space(8.0);
                    ui.label(RichText::new(err).size(12.0).color(ON_SURFACE));

                    ui.add_space(16.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                Button::new(RichText::new("Close").size(12.0).color(ON_SURFACE))
                                    .min_size(Vec2::new(70.0, 28.0)),
                            )
                            .clicked()
                        {
                            on_close();
                        }
                    });
                }
                UpdateState::Checking => {
                    ui.label(
                        RichText::new("Checking for updates...")
                            .size(14.0)
                            .color(PRIMARY),
                    );
                }
                UpdateState::Idle => {
                    ui.label(
                        RichText::new("You are on the latest version of EmuSync.")
                            .size(13.0)
                            .color(ON_SURFACE),
                    );
                    ui.add_space(12.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                Button::new(RichText::new("Close").size(12.0).color(ON_SURFACE))
                                    .min_size(Vec2::new(70.0, 28.0)),
                            )
                            .clicked()
                        {
                            on_close();
                        }
                    });
                }
            });
        });
}
