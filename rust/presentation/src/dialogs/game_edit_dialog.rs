//! Dialog for editing individual game overrides and resolving save setup requests.

use crate::{
    forms::GameEditFormState,
    model::{PathTarget, PickMode},
    theme::{DIVIDER, ERROR, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SURFACE, WARNING},
};
use egui::{
    Align2, Button, Checkbox, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke,
    TextEdit, Vec2, Window,
};
use emusync_domain::library::{SaveTarget, SaveTargetType};

/// Renders the Game Overrides / Save Setup dialog.
pub fn render_game_edit_dialog(
    ctx: &egui::Context,
    form: &mut GameEditFormState,
    notice: Option<&str>,
    cloud_files: &[String],
    on_save: &mut impl FnMut(Option<String>, Vec<SaveTarget>, Option<String>),
    on_pick_path: &mut impl FnMut(PathTarget, PickMode),
    on_close: &mut impl FnMut(),
) {
    let title = if notice.is_some() || !cloud_files.is_empty() {
        "Save Setup Required"
    } else {
        "Edit Game Overrides"
    };

    Window::new(title)
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, DIVIDER))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20)),
        )
        .show(ctx, |ui| {
            ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                ui.vertical(|ui| {
                    // Notice banner if present
                    if let Some(msg) = notice {
                        Frame::new()
                            .fill(SURFACE)
                            .stroke(Stroke::new(1.0, WARNING))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::same(10))
                            .show(ui, |ui| {
                                ui.label(RichText::new(msg).size(12.0).color(WARNING));
                            });
                        ui.add_space(10.0);
                    }

                    // Cloud restore destination if cloud files are present
                    if !cloud_files.is_empty() {
                        ui.label(
                            RichText::new("Restore Saves From Cloud:")
                                .size(13.0)
                                .strong()
                                .color(ON_BACKGROUND),
                        );
                        ui.label(
                            RichText::new(format!(
                                "Found {} cloud save file(s). Select the local destination folder to restore into.",
                                cloud_files.len()
                            ))
                            .size(11.0)
                            .color(ON_SURFACE_DIM),
                        );

                        ui.add_space(6.0);

                        ui.horizontal(|ui| {
                            ui.add(
                                TextEdit::singleline(&mut form.restore_destination_folder)
                                    .hint_text("Local restore folder path")
                                    .desired_width(400.0),
                            );
                            if ui.button("Browse").clicked() {
                                on_pick_path(PathTarget::RestoreDestination, PickMode::Directory);
                            }
                        });

                        ui.add_space(4.0);

                        ui.add(Checkbox::new(
                            &mut form.restore_whole_folder,
                            "Restore entire folder instead of individual files",
                        ));

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);
                    }

                    // Title override
                    ui.label(RichText::new("Display Title Override:").size(12.0).strong().color(ON_BACKGROUND));
                    ui.add(
                        TextEdit::singleline(&mut form.custom_title)
                            .hint_text("Custom title for this game in the library")
                            .desired_width(f32::INFINITY),
                    );

                    ui.add_space(8.0);

                    // Cover Art override
                    ui.label(RichText::new("Cover Art Override (Optional):").size(12.0).color(ON_SURFACE_DIM));
                    ui.horizontal(|ui| {
                        ui.add(
                            TextEdit::singleline(&mut form.cover_path)
                                .hint_text("Path to custom image file (.png, .jpg, .webp)")
                                .desired_width(420.0),
                        );
                        if ui.button("Browse").clicked() {
                            on_pick_path(PathTarget::CoverPath, PickMode::File);
                        }
                    });

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Save Targets section
                    ui.label(
                        RichText::new("Game-Specific Save Locations:")
                            .size(13.0)
                            .strong()
                            .color(ON_BACKGROUND),
                    );
                    ui.label(
                        RichText::new("Override the category default save targets for this specific game.")
                            .size(11.0)
                            .color(ON_SURFACE_DIM),
                    );

                    ui.add_space(8.0);

                    let mut rows_to_remove = Vec::new();

                    for (idx, row) in form.saves.rows.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.add(
                                TextEdit::singleline(&mut row.path)
                                    .hint_text("Save file or folder path")
                                    .desired_width(240.0),
                            );

                            let mode = match row.target_type {
                                SaveTargetType::File | SaveTargetType::Unknown => PickMode::File,
                                SaveTargetType::Directory => PickMode::Directory,
                            };
                            if ui.button("...").clicked() {
                                on_pick_path(PathTarget::SavePath(idx), mode);
                            }

                            let type_label = match row.target_type {
                                SaveTargetType::File => "File",
                                SaveTargetType::Directory => "Folder",
                                SaveTargetType::Unknown => "Unknown",
                            };
                            if ui.button(type_label).clicked() {
                                row.target_type = match row.target_type {
                                    SaveTargetType::File => SaveTargetType::Directory,
                                    SaveTargetType::Directory | SaveTargetType::Unknown => SaveTargetType::File,
                                };
                            }

                            ui.add(
                                TextEdit::singleline(&mut row.cloud_path)
                                    .hint_text("Cloud path")
                                    .desired_width(110.0),
                            );

                            if ui.button("✕").clicked() {
                                rows_to_remove.push(idx);
                            }
                        });
                        ui.add_space(4.0);
                    }

                    for idx in rows_to_remove.into_iter().rev() {
                        form.saves.remove_row(idx);
                    }

                    if ui.button("+ Add Save Location").clicked() {
                        form.saves.add_row();
                    }

                    if let Some(err) = &form.error_message {
                        ui.add_space(8.0);
                        ui.label(RichText::new(err).size(12.0).color(ERROR));
                    }

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(12.0);

                    // Action buttons
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    Button::new(RichText::new("Save Settings").size(12.0).strong().color(Color32::WHITE))
                                        .fill(PRIMARY)
                                        .min_size(Vec2::new(110.0, 28.0)),
                                )
                                .clicked()
                                && let Ok((title, targets, cover)) = form.build_result(cloud_files)
                            {
                                on_save(title, targets, cover);
                                on_close();
                            }

                            if ui
                                .add(
                                    Button::new(RichText::new("Cancel").size(12.0).color(ON_SURFACE))
                                        .min_size(Vec2::new(75.0, 28.0)),
                                )
                                .clicked()
                            {
                                on_close();
                            }
                        });
                    });
                });
            });
        });
}
