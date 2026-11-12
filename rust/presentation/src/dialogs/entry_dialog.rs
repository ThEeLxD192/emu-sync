//! Add / edit entry dialog, laid out like Kotlin's `AddEntryDialog`.

use crate::{
    dialogs::form_fields::{
        field_hint, field_label, filter_chip, labelled_field, path_field, primary_button,
        save_target_rows, text_button,
    },
    forms::EntryFormState,
    model::{EntryType, PathTarget, PickMode},
    theme::{DIVIDER, ERROR, ON_BACKGROUND, ON_SURFACE_DIM, PRIMARY, SURFACE, bold, glyph},
};
use egui::{
    Align, Color32, CornerRadius, Frame, Id, Layout, Margin, Modal, RichText, ScrollArea, Stroke,
    Ui, vec2,
};
use emusync_domain::library::GameEntry;

/// Callbacks the entry dialog reports to the application.
pub struct EntryDialogEvents<'a> {
    pub on_save: &'a mut dyn FnMut(GameEntry),
    pub on_pick_path: &'a mut dyn FnMut(PathTarget, PickMode),
    /// A save path was edited by hand; its real kind should be detected.
    pub on_save_path_edited: &'a mut dyn FnMut(usize, String),
    pub on_close: &'a mut dyn FnMut(),
}

/// Renders the Add / Edit entry modal dialog.
pub fn render_entry_dialog(
    ctx: &egui::Context,
    form: &mut EntryFormState,
    existing_groups: &[String],
    detect_fullscreen: fn(&str) -> String,
    events: EntryDialogEvents<'_>,
) {
    let is_editing = form.initial_entry.is_some();
    let max_height = (ctx.content_rect().height() - 160.0).max(240.0);

    let modal = Modal::new(Id::unique("entry_dialog_modal"))
        .backdrop_color(Color32::from_black_alpha(160))
        .frame(
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, DIVIDER))
                .corner_radius(CornerRadius::same(20))
                .inner_margin(Margin::same(24)),
        )
        .show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(
                RichText::new(if is_editing {
                    "Edit Entry"
                } else {
                    "Add New Entry"
                })
                .font(bold(20.0))
                .color(ON_BACKGROUND),
            );
            ui.add_space(20.0);

            ScrollArea::vertical()
                .max_height(max_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    type_selector(ui, form);
                    identity_fields(ui, form, existing_groups);
                    launch_fields(ui, form, events.on_pick_path);
                    match form.entry_type {
                        EntryType::Emulator => {
                            emulator_fields(ui, form, detect_fullscreen, events.on_pick_path)
                        }
                        EntryType::Native => native_fields(ui, form, events.on_pick_path),
                    }
                    save_fields(ui, form, events.on_pick_path, events.on_save_path_edited);
                    if let Some(message) = &form.error_message {
                        ui.add_space(8.0);
                        ui.label(RichText::new(message).size(12.0).color(ERROR));
                    }
                });

            ui.add_space(20.0);
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 40.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if primary_button(ui, if is_editing { "Save" } else { "Add" }).clicked()
                        && let Ok(entry) = form.build_entry()
                    {
                        (events.on_save)(entry);
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

fn type_selector(ui: &mut Ui, form: &mut EntryFormState) {
    field_label(ui, "Type");
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let half = (ui.available_width() - 8.0) / 2.0;
        for (kind, label, icon) in [
            (EntryType::Emulator, "Emulator", glyph::CONTROLLER),
            (EntryType::Native, "Native", glyph::GAMEPAD),
        ] {
            let selected = form.entry_type == kind;
            if filter_chip(ui, label, Some(icon), selected, half).clicked() && !selected {
                form.entry_type = kind;
                form.apply_type_defaults();
            }
        }
    });
    ui.add_space(16.0);
}

fn identity_fields(ui: &mut Ui, form: &mut EntryFormState, existing_groups: &[String]) {
    let emulator = form.entry_type == EntryType::Emulator;
    labelled_field(
        ui,
        "Name",
        "entry_name",
        &mut form.name,
        if emulator {
            "e.g. Yuzu or Eden"
        } else {
            "e.g. Spelunky Classic"
        },
    );
    ui.add_space(12.0);

    labelled_field(
        ui,
        "Folder / Platform Group (Optional)",
        "entry_group",
        &mut form.group,
        "e.g. Switch, PlayStation (shares cloud saves folder)",
    );
    if !existing_groups.is_empty() {
        ui.add_space(4.0);
        ui.label(
            RichText::new("Assign to existing folder:")
                .size(12.0)
                .color(ON_SURFACE_DIM),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for group in existing_groups {
                if filter_chip(ui, group, None, form.group_is(group), 0.0).clicked() {
                    form.toggle_group(group);
                }
            }
        });
    }
    let preview_color = if form.group.trim().is_empty() {
        ON_SURFACE_DIM
    } else {
        PRIMARY
    };
    field_hint(
        ui,
        &format!(
            "☁️ Google Drive saves folder: \"{}\"",
            form.cloud_folder_preview()
        ),
        preview_color,
    );
    ui.add_space(12.0);
}

fn launch_fields(
    ui: &mut Ui,
    form: &mut EntryFormState,
    on_pick_path: &mut dyn FnMut(PathTarget, PickMode),
) {
    let emulator = form.entry_type == EntryType::Emulator;
    field_label(
        ui,
        if emulator {
            "Emulator Executable Path"
        } else {
            "Executable Path"
        },
    );
    let (_, picked) = path_field(
        ui,
        "entry_executable",
        &mut form.executable_path,
        if emulator {
            "/usr/bin/mgba-qt"
        } else {
            "/opt/spelunky/spelunky"
        },
        &[("Browse", PickMode::File)],
        false,
    );
    if let Some(mode) = picked {
        on_pick_path(PathTarget::Executable, mode);
    }
    ui.add_space(12.0);

    labelled_field(
        ui,
        "Arguments (quote values that contain spaces)",
        "entry_arguments",
        &mut form.arguments,
        if emulator {
            "{ROM} --fullscreen"
        } else {
            "--fullscreen --no-intro"
        },
    );
    field_hint(
        ui,
        "Example: --profile \"My Profile\" keeps My Profile as one argument.",
        ON_SURFACE_DIM,
    );
    if emulator {
        field_hint(
            ui,
            "Use {ROM} to insert the requested game's path.",
            ON_SURFACE_DIM,
        );
    }
    ui.add_space(12.0);
}

fn emulator_fields(
    ui: &mut Ui,
    form: &mut EntryFormState,
    detect_fullscreen: fn(&str) -> String,
    on_pick_path: &mut dyn FnMut(PathTarget, PickMode),
) {
    field_label(ui, "ROMs Directory");
    let (_, picked) = path_field(
        ui,
        "entry_roms",
        &mut form.roms_directory,
        "/home/user/roms/gba",
        &[("Browse", PickMode::Directory)],
        false,
    );
    if let Some(mode) = picked {
        on_pick_path(PathTarget::RomsDirectory, mode);
    }
    ui.add_space(12.0);

    labelled_field(
        ui,
        "File Extensions (comma-separated)",
        "entry_extensions",
        &mut form.extensions,
        "gba, gbc",
    );
    ui.add_space(12.0);

    let detected = detect_fullscreen(&form.executable_path);
    let placeholder = if detected.is_empty() {
        "e.g. -bigpicture -fullscreen"
    } else {
        &detected
    };
    labelled_field(
        ui,
        "Fullscreen / Big Picture Flags (auto-detected if empty)",
        "entry_fullscreen",
        &mut form.fullscreen_args,
        placeholder,
    );
    field_hint(
        ui,
        &format!(
            "Forces the emulator to bypass its Qt GUI. Auto-detected: \"{}\"",
            if detected.is_empty() {
                "(none)"
            } else {
                &detected
            }
        ),
        ON_SURFACE_DIM,
    );
    ui.add_space(12.0);
}

fn native_fields(
    ui: &mut Ui,
    form: &mut EntryFormState,
    on_pick_path: &mut dyn FnMut(PathTarget, PickMode),
) {
    labelled_field(
        ui,
        "Wait for Process Name (Optional)",
        "entry_wait_process",
        &mut form.wait_for_process,
        "Spelunky.exe",
    );
    field_hint(
        ui,
        "Useful for Steam games: EmuSync will wait for this process to close.",
        ON_SURFACE_DIM,
    );
    ui.add_space(12.0);

    field_label(ui, "Cover Image (Optional)");
    let (_, picked) = path_field(
        ui,
        "entry_cover",
        &mut form.cover_path,
        "/path/to/game_cover.png",
        &[("Browse", PickMode::File)],
        false,
    );
    if let Some(mode) = picked {
        on_pick_path(PathTarget::CoverPath, mode);
    }
    ui.add_space(12.0);
}

fn save_fields(
    ui: &mut Ui,
    form: &mut EntryFormState,
    on_pick_path: &mut dyn FnMut(PathTarget, PickMode),
    on_save_path_edited: &mut dyn FnMut(usize, String),
) {
    let placeholder = if form.entry_type == EntryType::Emulator {
        "/home/user/emulator/memcards/Mcd001.ps2"
    } else {
        "/home/user/.local/share/save.dat"
    };
    field_label(ui, "Save Files/Directories Paths");
    save_target_rows(
        ui,
        "entry_save",
        &mut form.saves,
        placeholder,
        on_pick_path,
        on_save_path_edited,
    );
}
