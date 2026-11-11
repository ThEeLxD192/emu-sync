//! Edit game dialog (title, cover art and save targets, or cloud restore destination),
//! laid out like Kotlin's `EditGameDialog`.

use crate::{
    components::icons::paint_glyph,
    cover_cache::CoverCache,
    dialogs::form_fields::{
        field_hint, path_field, primary_button, save_target_rows, text_button, text_field,
        tonal_button,
    },
    forms::{GameEditFormState, GameEditResult},
    model::{GameItem, PathTarget, PickMode},
    theme::{
        DIVIDER, ERROR, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SUCCESS, SURFACE,
        SURFACE_VARIANT, bold, glyph,
    },
};
use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Modal, Rect, RichText,
    ScrollArea, Sense, Stroke, Ui, pos2, vec2,
};
use emusync_domain::library::{GameEntry, clean_game_title};
use std::path::Path;

/// Callbacks the game dialog reports to the application.
pub struct GameEditDialogEvents<'a> {
    pub on_save: &'a mut dyn FnMut(GameEditResult),
    pub on_pick_path: &'a mut dyn FnMut(PathTarget, PickMode),
    pub on_save_path_edited: &'a mut dyn FnMut(usize, String),
    pub on_close: &'a mut dyn FnMut(),
}

/// Renders the Edit Game modal dialog.
pub fn render_game_edit_dialog(
    ctx: &egui::Context,
    form: &mut GameEditFormState,
    game: &GameItem,
    notice: Option<&str>,
    cloud_files: &[String],
    cover_cache: &mut CoverCache,
    events: GameEditDialogEvents<'_>,
) {
    let max_height = (ctx.content_rect().height() - 160.0).clamp(240.0, 640.0);
    let modal = Modal::new(Id::unique("game_edit_dialog_modal"))
        .backdrop_color(Color32::from_black_alpha(160))
        .frame(
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, DIVIDER))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::same(24)),
        )
        .show(ctx, |ui| {
            ui.set_width(560.0);
            ScrollArea::vertical()
                .max_height(max_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        RichText::new("Edit Game")
                            .font(bold(22.0))
                            .color(ON_SURFACE),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(game.effective_title())
                            .font(bold(15.0))
                            .color(PRIMARY),
                    );
                    ui.label(
                        RichText::new(
                            "Configure custom title, cover art, and specific save files/folders.",
                        )
                        .size(12.0)
                        .color(ON_SURFACE_DIM),
                    );
                    ui.add_space(18.0);

                    if let Some(notice) = notice {
                        notice_banner(ui, notice);
                    }
                    title_fields(ui, form, game);
                    cover_art(ui, form, game, cover_cache, events.on_pick_path);
                    save_fields(
                        ui,
                        form,
                        game,
                        cloud_files,
                        events.on_pick_path,
                        events.on_save_path_edited,
                    );
                    if let Some(message) = &form.error_message {
                        ui.add_space(8.0);
                        ui.label(RichText::new(message).size(12.0).color(ERROR));
                    }
                });

            ui.add_space(12.0);
            let can_save = form.can_save(cloud_files);
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 40.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let save = ui.add_enabled_ui(can_save, |ui| primary_button(ui, "Save"));
                    if save.inner.clicked()
                        && let Ok(result) = form.build_result(cloud_files)
                    {
                        (events.on_save)(result);
                        (events.on_close)();
                    }
                    if text_button(ui, "Cancel", PRIMARY).clicked() {
                        (events.on_close)();
                    }
                },
            );
        });
    if modal.should_close() {
        (events.on_close)();
    }
}

fn notice_banner(ui: &mut Ui, notice: &str) {
    Frame::new()
        .fill(PRIMARY.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, PRIMARY.gamma_multiply(0.4)))
        .corner_radius(10)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                paint_glyph(ui, rect.center(), glyph::CLOUD_DOWNLOAD, 24.0, PRIMARY);
                ui.add_space(10.0);
                ui.add(
                    egui::Label::new(RichText::new(notice).size(12.0).color(ON_BACKGROUND)).wrap(),
                );
            });
        });
    ui.add_space(16.0);
}

fn section_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(bold(13.0)).color(ON_SURFACE));
}

fn title_fields(ui: &mut Ui, form: &mut GameEditFormState, game: &GameItem) {
    let raw = game.raw_name();
    let clean = clean_game_title(&raw);
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 32.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            section_title(ui, "Game Title");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if form.custom_title != raw && small_text_button(ui, "Raw Name", ON_SURFACE_DIM) {
                    form.custom_title = raw.clone();
                }
                if form.custom_title != clean && small_text_button(ui, "Auto-Clean", PRIMARY) {
                    form.custom_title = clean.clone();
                }
            });
        },
    );
    ui.add_space(4.0);
    text_field(
        ui,
        "game_title",
        &mut form.custom_title,
        &clean,
        ui.available_width(),
        false,
    );
    if let Some(rom) = game.rom_file_name() {
        field_hint(ui, &format!("ROM File: {rom}"), ON_SURFACE_DIM);
    }
    ui.add_space(18.0);
}

fn small_text_button(ui: &mut Ui, text: &str, color: Color32) -> bool {
    crate::components::pill::Pill {
        text,
        text_color: color,
        font: FontId::proportional(11.0),
        fill: Color32::TRANSPARENT,
        padding: vec2(8.0, 4.0),
        corner_radius: 14,
        ..Default::default()
    }
    .show(ui)
    .clicked()
}

fn cover_art(
    ui: &mut Ui,
    form: &mut GameEditFormState,
    game: &GameItem,
    cover_cache: &mut CoverCache,
    on_pick_path: &mut dyn FnMut(PathTarget, PickMode),
) {
    let is_custom = !form.cover_path.trim().is_empty();
    // The scanned cover already prefers the saved custom one, as Kotlin's preview does.
    let detected = game
        .cover_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string);
    let preview_path = if is_custom {
        // Only an existing file is previewed, so half-typed paths never reach the cache.
        Some(form.cover_path.trim().to_string()).filter(|path| Path::new(path).is_file())
    } else {
        detected.clone()
    };
    let is_auto = !is_custom && detected.is_some();

    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 24.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            section_title(ui, "Cover Art");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (text, color) = if is_custom {
                    ("Custom Cover", PRIMARY)
                } else if is_auto {
                    ("Auto-detected", SUCCESS)
                } else {
                    ("No Cover", ON_SURFACE_DIM)
                };
                Frame::new()
                    .fill(color.gamma_multiply(0.15))
                    .corner_radius(6)
                    .inner_margin(Margin::symmetric(8, 2))
                    .show(ui, |ui| {
                        ui.label(RichText::new(text).size(11.0).color(color));
                    });
            });
        },
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new("Individual cover image for this game, or empty for automatic detection.")
            .size(12.0)
            .color(ON_SURFACE_DIM),
    );
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        let (preview, _) = ui.allocate_exact_size(vec2(136.0, 180.0), Sense::hover());
        paint_cover_preview(ui, preview, preview_path.as_deref(), cover_cache);

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(
                RichText::new(if is_custom {
                    "Custom Image File:"
                } else {
                    "Detected Image File:"
                })
                .size(12.0)
                .color(ON_SURFACE_DIM),
            );
            let placeholder = detected
                .as_deref()
                .and_then(|p| Path::new(p).file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Auto-detecting...".to_string());
            text_field(
                ui,
                "game_cover",
                &mut form.cover_path,
                &placeholder,
                ui.available_width(),
                false,
            );
            ui.label(
                RichText::new(if is_custom {
                    "Using your specific custom image for this game."
                } else if is_auto {
                    "Found automatically next to ROM or emulator covers."
                } else {
                    "Select an image file to display as box art."
                })
                .size(12.0)
                .color(ON_SURFACE_DIM.gamma_multiply(0.75)),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let label = if is_custom {
                    "Change Image..."
                } else {
                    "Select Image..."
                };
                if tonal_button(ui, label).clicked() {
                    on_pick_path(PathTarget::CoverPath, PickMode::File);
                }
                if is_custom && text_button(ui, "Reset to Auto", PRIMARY).clicked() {
                    form.cover_path.clear();
                }
            });
        });
    });
    ui.add_space(22.0);
}

fn paint_cover_preview(ui: &mut Ui, rect: Rect, path: Option<&str>, cover_cache: &mut CoverCache) {
    ui.painter().rect(
        rect,
        10,
        SURFACE_VARIANT,
        Stroke::new(1.0, DIVIDER),
        egui::StrokeKind::Inside,
    );
    let texture = path.and_then(|p| cover_cache.get_or_load(ui.ctx(), p));
    if let Some(texture) = texture {
        let area = rect.shrink(4.0);
        let size = texture.size_vec2();
        let scale = (area.width() / size.x).min(area.height() / size.y.max(1.0));
        ui.painter().image(
            texture.id(),
            Rect::from_center_size(area.center(), size * scale),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        let dim = ON_SURFACE_DIM.gamma_multiply(0.35);
        paint_glyph(ui, rect.center() - vec2(0.0, 12.0), glyph::IMAGE, 42.0, dim);
        ui.painter().text(
            rect.center() + vec2(0.0, 24.0),
            egui::Align2::CENTER_CENTER,
            "No Cover Found",
            FontId::proportional(11.0),
            ON_SURFACE_DIM.gamma_multiply(0.6),
        );
    }
}

fn save_fields(
    ui: &mut Ui,
    form: &mut GameEditFormState,
    game: &GameItem,
    cloud_files: &[String],
    on_pick_path: &mut dyn FnMut(PathTarget, PickMode),
    on_save_path_edited: &mut dyn FnMut(usize, String),
) {
    let restoring = !cloud_files.is_empty();
    section_title(
        ui,
        if restoring {
            "Cloud Save Destination"
        } else {
            "Save Files/Directories Paths"
        },
    );
    ui.add_space(4.0);
    let description = if restoring {
        "Choose one folder. The cloud files below will be restored as individual files; other files in this folder will not be synced."
    } else if matches!(game.entry, GameEntry::Emulator(_)) {
        "Only paths specific to this game. Emulator-wide paths are configured in the system settings."
    } else {
        "Specific save files or directories synchronized for this game."
    };
    ui.add(egui::Label::new(RichText::new(description).size(12.0).color(ON_SURFACE_DIM)).wrap());
    ui.add_space(10.0);

    if restoring {
        restore_fields(ui, form, cloud_files, on_pick_path);
    } else {
        save_target_rows(
            ui,
            "game_save",
            &mut form.saves,
            "/home/user/saves/game.sav",
            on_pick_path,
            on_save_path_edited,
        );
    }
    ui.add_space(20.0);
}

fn restore_fields(
    ui: &mut Ui,
    form: &mut GameEditFormState,
    cloud_files: &[String],
    on_pick_path: &mut dyn FnMut(PathTarget, PickMode),
) {
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.radio_value(
        &mut form.restore_whole_folder,
        false,
        RichText::new("Only the cloud files listed below").color(ON_SURFACE),
    );
    ui.radio_value(
        &mut form.restore_whole_folder,
        true,
        RichText::new("The entire folder").color(ON_SURFACE),
    );
    ui.spacing_mut().item_spacing.y = 0.0;
    let explanation = if form.restore_whole_folder {
        "Every file in this folder will be synchronized. Use a dedicated folder if it contains saves from other games."
    } else {
        "Only the cloud files listed below will be synchronized; other files in the folder will be left alone."
    };
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.add(
            egui::Label::new(RichText::new(explanation).size(12.0).color(ON_SURFACE_DIM)).wrap(),
        );
    });
    ui.add_space(8.0);

    let (_, picked) = path_field(
        ui,
        "game_restore_destination",
        &mut form.restore_destination_folder,
        "Choose a destination folder with the Folder button",
        &[("Folder", PickMode::Directory)],
        false,
    );
    if let Some(mode) = picked {
        on_pick_path(PathTarget::RestoreDestination, mode);
    }
    if form.selected_restore_paths(cloud_files).is_empty() {
        ui.add_space(6.0);
        ui.label(
            RichText::new("Select a destination folder before saving.")
                .size(12.0)
                .color(ERROR),
        );
    }
    ui.add_space(10.0);
    for name in cloud_files {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("• {name}"))
                    .size(12.0)
                    .color(ON_SURFACE_DIM),
            );
        });
        ui.add_space(3.0);
    }
}
