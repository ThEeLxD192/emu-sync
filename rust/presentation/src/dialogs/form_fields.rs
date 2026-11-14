//! Form building blocks shared by the entry and game dialogs, styled like Kotlin's
//! `FormField`, `PathField`, `CloudPathField` and Material filter chips.

use crate::forms::save_targets::{cloud_path_is_valid, excluded_folder_is_valid};
use crate::{
    components::{icons::glyph_button, pill::Pill},
    forms::SaveTargetFormState,
    model::{PathTarget, PickMode},
    theme::{
        DIVIDER, ERROR, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, PRIMARY_CONTAINER,
        SURFACE_SELECTED, SURFACE_VARIANT, glyph,
    },
};
use egui::{Color32, FontId, Frame, Margin, Response, RichText, Stroke, TextEdit, Ui, vec2};
use emusync_domain::library::SaveTargetType;

const FIELD_HEIGHT: f32 = 40.0;
const FIELD_RADIUS: u8 = 10;

/// Small dim label shown above a field.
pub(crate) fn field_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(ON_SURFACE_DIM));
    ui.add_space(4.0);
}

/// Helper text shown below a field, wrapped to the field's width: a hint can be longer
/// than one line, more so in some languages.
pub(crate) fn field_hint(ui: &mut Ui, text: &str, color: Color32) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(text).size(12.0).color(color)).wrap());
    });
}

/// Single-line text field: blue border when focused, red when invalid.
pub(crate) fn text_field(
    ui: &mut Ui,
    id_salt: impl egui::AsIdSalt,
    value: &mut String,
    placeholder: &str,
    width: f32,
    invalid: bool,
) -> Response {
    let id = ui.make_persistent_id(id_salt);
    let focused = ui.memory(|m| m.has_focus(id));
    let border = if invalid {
        ERROR
    } else if focused {
        PRIMARY
    } else {
        DIVIDER
    };
    let frame = Frame::new()
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(FIELD_RADIUS)
        .inner_margin(Margin::symmetric(12, 10));
    ui.add(
        TextEdit::singleline(value)
            .id(id)
            .frame(frame)
            .hint_text(RichText::new(placeholder).color(ON_SURFACE_DIM.gamma_multiply(0.5)))
            .text_color(if focused { ON_BACKGROUND } else { ON_SURFACE })
            .font(FontId::proportional(13.0))
            .desired_width(width)
            .min_size(vec2(width, FIELD_HEIGHT)),
    )
}

/// Labelled full-width text field.
pub(crate) fn labelled_field(
    ui: &mut Ui,
    label: &str,
    id_salt: impl egui::AsIdSalt,
    value: &mut String,
    placeholder: &str,
) -> Response {
    field_label(ui, label);
    text_field(ui, id_salt, value, placeholder, ui.available_width(), false)
}

/// A filled secondary button ("Browse", "File", "Folder").
pub(crate) fn tonal_button(ui: &mut Ui, text: &str) -> Response {
    Pill {
        text,
        text_color: ON_SURFACE,
        font: FontId::proportional(13.0),
        fill: SURFACE_SELECTED,
        padding: vec2(16.0, 0.0),
        min_height: FIELD_HEIGHT,
        corner_radius: FIELD_RADIUS,
        ..Pill::default()
    }
    .show(ui)
}

/// Text-only primary button ("Cancel", "+ Add Another Path", type toggles).
pub(crate) fn text_button(ui: &mut Ui, text: &str, color: Color32) -> Response {
    Pill {
        text,
        text_color: color,
        font: FontId::proportional(13.0),
        fill: Color32::TRANSPARENT,
        padding: vec2(12.0, 0.0),
        min_height: FIELD_HEIGHT,
        corner_radius: 20,
        ..Pill::default()
    }
    .show(ui)
}

/// Filled primary button ("Add", "Save").
pub(crate) fn primary_button(ui: &mut Ui, text: &str) -> Response {
    Pill {
        text,
        text_color: Color32::WHITE,
        font: FontId::proportional(13.0),
        fill: PRIMARY,
        padding: vec2(24.0, 0.0),
        min_height: FIELD_HEIGHT,
        corner_radius: 20,
        ..Pill::default()
    }
    .show(ui)
}

/// Material filter chip with an optional leading icon.
pub(crate) fn filter_chip(
    ui: &mut Ui,
    text: &str,
    icon: Option<&str>,
    selected: bool,
    min_width: f32,
) -> Response {
    let color = if selected { PRIMARY } else { ON_SURFACE };
    Pill {
        icon: icon.map(|glyph| (glyph, color)),
        text,
        text_color: color,
        font: FontId::proportional(13.0),
        fill: if selected {
            PRIMARY_CONTAINER
        } else {
            SURFACE_VARIANT
        },
        stroke: if selected {
            Stroke::NONE
        } else {
            Stroke::new(1.0, DIVIDER)
        },
        padding: vec2(12.0, 0.0),
        min_height: 32.0,
        min_width,
        corner_radius: 8,
        ..Pill::default()
    }
    .show(ui)
}

/// A path text field followed by picker buttons; returns the field response and the
/// picker that was clicked, if any.
pub(crate) fn path_field(
    ui: &mut Ui,
    id_salt: impl egui::AsIdSalt,
    value: &mut String,
    placeholder: &str,
    pickers: &[(&str, PickMode)],
    invalid: bool,
) -> (Response, Option<PickMode>) {
    ui.horizontal(|ui| {
        let mut picked = None;
        ui.spacing_mut().item_spacing.x = 8.0;
        // Reserve the buttons' real width (text + padding + spacing) for them.
        let button_width: f32 = pickers
            .iter()
            .map(|(label, _)| {
                let text = ui.painter().layout_no_wrap(
                    (*label).to_owned(),
                    FontId::proportional(13.0),
                    ON_SURFACE,
                );
                text.size().x + 32.0 + 8.0
            })
            .sum();
        let width = (ui.available_width() - button_width).max(120.0);
        let field = text_field(ui, id_salt, value, placeholder, width, invalid);
        for (label, mode) in pickers {
            if tonal_button(ui, label).clicked() {
                picked = Some(*mode);
            }
        }
        (field, picked)
    })
    .inner
}

/// Optional cloud path with live validation (Kotlin's `CloudPathField`).
pub(crate) fn cloud_path_field(
    ui: &mut Ui,
    id_salt: impl egui::AsIdSalt,
    value: &mut String,
    kind: &SaveTargetType,
    automatic: Option<&str>,
) -> Response {
    let t = crate::i18n::tr(ui.ctx());
    let invalid = !cloud_path_is_valid(value);
    let placeholder = if *kind == SaveTargetType::Directory {
        t.cloud_prefix_placeholder
    } else {
        t.cloud_name_placeholder
    };
    field_label(ui, t.cloud_path_label);
    let response = text_field(
        ui,
        id_salt,
        value,
        placeholder,
        ui.available_width(),
        invalid,
    );
    if invalid {
        field_hint(ui, t.cloud_path_invalid, ERROR);
    } else if let Some(automatic) = automatic {
        field_hint(
            ui,
            &crate::i18n::fill(t.cloud_path_automatic, &[("automatic", &automatic)]),
            ON_SURFACE_DIM,
        );
    } else {
        field_hint(ui, t.cloud_path_same, ON_SURFACE_DIM);
    }
    response
}

/// Label for a save target kind's toggle button.
pub fn save_kind_label(kind: &SaveTargetType, t: &crate::i18n::Texts) -> &'static str {
    match kind {
        SaveTargetType::File => t.file,
        SaveTargetType::Directory => t.folder,
        SaveTargetType::Unknown => t.choose_type,
    }
}

/// Trash icon button used to remove a row.
pub(crate) fn remove_button(ui: &mut Ui, id_salt: impl egui::AsIdSalt) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(FIELD_HEIGHT, FIELD_HEIGHT), egui::Sense::hover());
    let id = ui.make_persistent_id(("remove_row", id_salt));
    glyph_button(ui, id, rect, glyph::DELETE, 20.0, ERROR)
        .on_hover_text(crate::i18n::tr(ui.ctx()).remove_path)
}

/// Editable save target rows (path with File/Folder pickers, kind toggle, remove button
/// and cloud path) followed by "+ Add Another Path", shared by both dialogs.
pub(crate) fn save_target_rows(
    ui: &mut Ui,
    id_prefix: &str,
    saves: &mut SaveTargetFormState,
    placeholder: &str,
    on_pick_path: &mut dyn FnMut(PathTarget, PickMode),
    on_save_path_edited: &mut dyn FnMut(usize, String),
) {
    let t = crate::i18n::tr(ui.ctx());
    let removable = saves.rows.len() > 1;
    let automatic = saves.automatic_cloud_paths();
    let mut remove = None;
    for index in 0..saves.rows.len() {
        let mut toggle = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let kind_label = save_kind_label(&saves.rows[index].target_type, t);
            let tail = if removable { FIELD_HEIGHT + 4.0 } else { 0.0 };
            let toggle_width = text_width(ui, kind_label) + 24.0 + 4.0;
            ui.scope(|ui| {
                ui.set_width(ui.available_width() - toggle_width - tail);
                let (field, picked) = path_field(
                    ui,
                    (id_prefix, "path", index),
                    &mut saves.rows[index].path,
                    placeholder,
                    &[(t.file, PickMode::File), (t.folder, PickMode::Directory)],
                    false,
                );
                if field.changed() {
                    on_save_path_edited(index, saves.rows[index].path.clone());
                }
                if let Some(mode) = picked {
                    on_pick_path(PathTarget::SavePath(index), mode);
                }
            });
            if text_button(ui, kind_label, PRIMARY).clicked() {
                toggle = true;
            }
            if removable && remove_button(ui, (id_prefix, "remove", index)).clicked() {
                remove = Some(index);
            }
        });
        if toggle {
            saves.toggle_type(index);
        }
        ui.add_space(8.0);
        let row = &mut saves.rows[index];
        cloud_path_field(
            ui,
            (id_prefix, "cloud", index),
            &mut row.cloud_path,
            &row.target_type,
            automatic.get(index).cloned().flatten().as_deref(),
        );
        if saves.rows[index].accepts_excluded_folders() {
            ui.add_space(8.0);
            excluded_folder_rows(ui, id_prefix, saves, index, on_pick_path);
        }
        ui.add_space(12.0);
    }
    if let Some(index) = remove {
        saves.remove_row(index);
    }

    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), FIELD_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            if text_button(ui, t.add_another_path, PRIMARY).clicked() {
                saves.add_row();
            }
        },
    );
}

/// The folders inside save folder `index` that are never synced: one picker row per
/// folder with a remove button, followed by "+ Add Excluded Folder".
fn excluded_folder_rows(
    ui: &mut Ui,
    id_prefix: &str,
    saves: &mut SaveTargetFormState,
    index: usize,
    on_pick_path: &mut dyn FnMut(PathTarget, PickMode),
) {
    let t = crate::i18n::tr(ui.ctx());
    field_label(ui, t.excluded_folders_label);
    let mut remove = None;
    for folder in 0..saves.rows[index].excluded_folders.len() {
        let value = &mut saves.rows[index].excluded_folders[folder];
        let invalid = !excluded_folder_is_valid(value);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.scope(|ui| {
                ui.set_width(ui.available_width() - FIELD_HEIGHT - 4.0);
                let (_, picked) = path_field(
                    ui,
                    (id_prefix, "excluded", index, folder),
                    value,
                    t.excluded_folder_placeholder,
                    &[(t.folder, PickMode::Directory)],
                    invalid,
                );
                if picked.is_some() {
                    on_pick_path(
                        PathTarget::ExcludedFolder {
                            save: index,
                            folder,
                        },
                        PickMode::Directory,
                    );
                }
            });
            if remove_button(ui, (id_prefix, "remove_excluded", index, folder)).clicked() {
                remove = Some(folder);
            }
        });
        if invalid {
            field_hint(ui, t.excluded_folder_outside, ERROR);
        }
        ui.add_space(8.0);
    }
    if let Some(folder) = remove {
        saves.remove_excluded_folder(index, folder);
    }
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), FIELD_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            if text_button(ui, t.add_excluded_folder, PRIMARY).clicked() {
                saves.add_excluded_folder(index);
            }
        },
    );
    field_hint(ui, t.excluded_folders_hint, ON_SURFACE_DIM);
}

fn text_width(ui: &Ui, text: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(13.0), Color32::WHITE)
        .size()
        .x
}

/// Masked text field with an eye button that reveals the value while toggled.
pub(crate) fn secret_field(
    ui: &mut Ui,
    id_salt: impl egui::AsIdSalt,
    value: &mut String,
    placeholder: &str,
    revealed: &mut bool,
) -> Response {
    let id = ui.make_persistent_id(id_salt);
    let focused = ui.memory(|m| m.has_focus(id));
    let frame = Frame::new()
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, if focused { PRIMARY } else { DIVIDER }))
        .corner_radius(FIELD_RADIUS)
        .inner_margin(Margin::symmetric(12, 10));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let width = (ui.available_width() - FIELD_HEIGHT - 4.0).max(120.0);
        let field = ui.add(
            TextEdit::singleline(value)
                .id(id)
                .frame(frame)
                .password(!*revealed)
                .hint_text(RichText::new(placeholder).color(ON_SURFACE_DIM.gamma_multiply(0.5)))
                .text_color(if focused { ON_BACKGROUND } else { ON_SURFACE })
                .font(FontId::proportional(13.0))
                .desired_width(width)
                .min_size(vec2(width, FIELD_HEIGHT)),
        );
        let (rect, _) =
            ui.allocate_exact_size(vec2(FIELD_HEIGHT, FIELD_HEIGHT), egui::Sense::hover());
        let (icon, tip) = if *revealed {
            (glyph::EYE_OFF, crate::i18n::tr(ui.ctx()).hide)
        } else {
            (glyph::EYE, crate::i18n::tr(ui.ctx()).show)
        };
        let eye = ui.make_persistent_id(("reveal", id));
        if glyph_button(ui, eye, rect, icon, 20.0, ON_SURFACE_DIM)
            .on_hover_text(tip)
            .clicked()
        {
            *revealed = !*revealed;
        }
        field
    })
    .inner
}
