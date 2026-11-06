//! Dialog for adding and editing emulator and native game categories.

use crate::{
    forms::EntryFormState,
    model::{EntryType, PathTarget, PickMode},
    theme::{DIVIDER, ERROR, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SURFACE},
};
use egui::{
    Align2, Button, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, TextEdit,
    Vec2, Window,
};
use emusync_domain::library::{GameEntry, SaveTargetType};

/// Renders the Add / Edit category modal dialog.
pub fn render_entry_dialog(
    ctx: &egui::Context,
    form: &mut EntryFormState,
    on_save: &mut impl FnMut(GameEntry),
    on_pick_path: &mut impl FnMut(PathTarget, PickMode),
    on_close: &mut impl FnMut(),
) {
    let is_editing = form.initial_entry.is_some();
    let title = if is_editing {
        "Edit Category"
    } else {
        "Add Category"
    };

    Window::new(title)
        .collapsible(false)
        .resizable(true)
        .default_width(540.0)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, DIVIDER))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(20)),
        )
        .show(ctx, |ui| {
            ScrollArea::vertical().max_height(580.0).show(ui, |ui| {
                ui.vertical(|ui| {
                    // Type selector
                    ui.horizontal(|ui| {
                        let is_emu = form.entry_type == EntryType::Emulator;
                        if ui
                            .selectable_label(is_emu, RichText::new("Emulator System").strong())
                            .clicked()
                            && !is_emu
                        {
                            form.entry_type = EntryType::Emulator;
                            form.apply_type_defaults();
                        }

                        let is_nat = form.entry_type == EntryType::Native;
                        if ui
                            .selectable_label(is_nat, RichText::new("Native PC Game").strong())
                            .clicked()
                            && !is_nat
                        {
                            form.entry_type = EntryType::Native;
                            form.apply_type_defaults();
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Name and Group
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Category Name:").size(12.0).strong().color(ON_BACKGROUND));
                            ui.add(TextEdit::singleline(&mut form.name).desired_width(240.0));
                        });

                        ui.vertical(|ui| {
                            ui.label(RichText::new("Group (Optional):").size(12.0).color(ON_SURFACE_DIM));
                            ui.add(TextEdit::singleline(&mut form.group).desired_width(240.0));
                        });
                    });

                    ui.add_space(8.0);

                    // Executable Path
                    ui.label(RichText::new("Executable Path:").size(12.0).strong().color(ON_BACKGROUND));
                    ui.horizontal(|ui| {
                        ui.add(TextEdit::singleline(&mut form.executable_path).desired_width(430.0));
                        if ui.button("Browse").clicked() {
                            on_pick_path(PathTarget::Executable, PickMode::File);
                        }
                    });

                    ui.add_space(8.0);

                    // Arguments
                    ui.label(RichText::new("Arguments:").size(12.0).color(ON_BACKGROUND));
                    ui.add(
                        TextEdit::singleline(&mut form.arguments)
                            .hint_text(if form.entry_type == EntryType::Emulator { "{ROM}" } else { "" })
                            .desired_width(f32::INFINITY),
                    );

                    ui.add_space(8.0);

                    // Specific fields
                    match form.entry_type {
                        EntryType::Emulator => {
                            ui.label(RichText::new("ROMs Directory:").size(12.0).strong().color(ON_BACKGROUND));
                            ui.horizontal(|ui| {
                                ui.add(TextEdit::singleline(&mut form.roms_directory).desired_width(430.0));
                                if ui.button("Browse").clicked() {
                                    on_pick_path(PathTarget::RomsDirectory, PickMode::Directory);
                                }
                            });

                            ui.add_space(8.0);

                            ui.label(RichText::new("File Extensions (comma-separated):").size(12.0).strong().color(ON_BACKGROUND));
                            ui.add(
                                TextEdit::singleline(&mut form.extensions)
                                    .hint_text("e.g. iso, chd, bin, cue")
                                    .desired_width(f32::INFINITY),
                            );

                            ui.add_space(8.0);

                            ui.label(RichText::new("Fullscreen Arguments (Optional):").size(12.0).color(ON_SURFACE_DIM));
                            ui.add(
                                TextEdit::singleline(&mut form.fullscreen_args)
                                    .hint_text("e.g. -f or --fullscreen")
                                    .desired_width(f32::INFINITY),
                            );
                        }
                        EntryType::Native => {
                            ui.label(RichText::new("Wait For Process Name (Optional):").size(12.0).color(ON_SURFACE_DIM));
                            ui.add(
                                TextEdit::singleline(&mut form.wait_for_process)
                                    .hint_text("Process to monitor if launcher exits early")
                                    .desired_width(f32::INFINITY),
                            );

                            ui.add_space(8.0);

                            ui.label(RichText::new("Cover Art Path (Optional):").size(12.0).color(ON_SURFACE_DIM));
                            ui.horizontal(|ui| {
                                ui.add(TextEdit::singleline(&mut form.cover_path).desired_width(430.0));
                                if ui.button("Browse").clicked() {
                                    on_pick_path(PathTarget::CoverPath, PickMode::File);
                                }
                            });
                        }
                    }

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Save Targets section
                    ui.label(
                        RichText::new("Save Target Locations:")
                            .size(13.0)
                            .strong()
                            .color(ON_BACKGROUND),
                    );
                    ui.label(
                        RichText::new("Define where save files or directories are stored locally and in the cloud.")
                            .size(11.0)
                            .color(ON_SURFACE_DIM),
                    );

                    ui.add_space(8.0);

                    let mut rows_to_remove = Vec::new();

                    for (idx, row) in form.saves.rows.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            // Path input
                            ui.add(
                                TextEdit::singleline(&mut row.path)
                                    .hint_text("Local save file or directory path")
                                    .desired_width(260.0),
                            );

                            // Browse button
                            let mode = match row.target_type {
                                SaveTargetType::File | SaveTargetType::Unknown => PickMode::File,
                                SaveTargetType::Directory => PickMode::Directory,
                            };
                            if ui.button("...").clicked() {
                                on_pick_path(PathTarget::SavePath(idx), mode);
                            }

                            // Type toggle
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

                            // Cloud path
                            ui.add(
                                TextEdit::singleline(&mut row.cloud_path)
                                    .hint_text("Cloud folder/file name")
                                    .desired_width(120.0),
                            );

                            // Delete button
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

                    // Error banner
                    if let Some(err) = &form.error_message {
                        ui.add_space(8.0);
                        ui.label(RichText::new(err).size(12.0).color(ERROR));
                    }

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(12.0);

                    // Form action buttons
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    Button::new(RichText::new("Save Category").size(12.0).strong().color(Color32::WHITE))
                                        .fill(PRIMARY)
                                        .min_size(Vec2::new(120.0, 28.0)),
                                )
                                .clicked()
                                && let Ok(entry) = form.build_entry()
                            {
                                on_save(entry);
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
