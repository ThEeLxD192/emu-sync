//! Sidebar listing library folders and entries, laid out like Kotlin's `Sidebar`.

mod controls;
mod row;

use crate::{
    actions::UiAction,
    components::{
        icons::glyph_button,
        sidebar_order::{
            SidebarInteraction, SidebarRow, compute_sidebar_rows, move_sidebar_entry,
            move_top_level_unit,
        },
    },
    state::AppUiState,
    theme::{ON_SURFACE_DIM, glyph},
};
use egui::{Color32, Margin, RichText, ScrollArea, Sense, Ui, vec2};
use emusync_domain::library::GameEntry;
use row::render_row;

const ENTRY_HEIGHT: f32 = 46.0;
const FOLDER_HEIGHT: f32 = 36.0;
const ROW_GAP: f32 = 2.0;
/// Distance the pointer must travel to move a dragged row by one position.
const DRAG_STEP: f32 = ENTRY_HEIGHT + ROW_GAP;
const NESTED_INDENT: f32 = 12.0;
const ROW_RADIUS: u8 = 8;
const STEAM_REGISTERED: Color32 = Color32::from_rgb(0x66, 0xBB, 0x6A);

/// Something the user did on a row, applied after the list is laid out.
enum RowEvent {
    Select(GameEntry),
    ActivateFolder,
    ToggleCollapse,
    Move(isize),
    ToggleSteam(GameEntry),
    DragStart,
    Drag(f32),
    DragEnd,
}

/// Renders the sidebar: the LIBRARY header with the add button and the folder/entry list.
pub fn render_sidebar(
    ui: &mut Ui,
    state: &AppUiState,
    interaction: &mut SidebarInteraction,
    on_action: &mut impl FnMut(UiAction),
    on_open_add: &mut impl FnMut(),
) {
    let entries: &[GameEntry] = state.config.as_ref().map_or(&[], |c| c.entries.as_slice());

    // A drag whose row vanished (or a missed release) must not stay active.
    if interaction.dragging_key().is_some() && !ui.input(|i| i.pointer.any_down()) {
        interaction.end_drag();
    }

    egui::Frame::new()
        .inner_margin(Margin {
            left: 16,
            right: 12,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            let row = vec2(ui.available_width(), 28.0);
            let centered = egui::Layout::left_to_right(egui::Align::Center);
            ui.allocate_ui_with_layout(row, centered, |ui| {
                ui.set_min_height(28.0);
                ui.label(
                    RichText::new(crate::i18n::tr(ui.ctx()).library)
                        .size(12.0)
                        .color(ON_SURFACE_DIM)
                        .extra_letter_spacing(1.5),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                    if glyph_button(
                        ui,
                        ui.make_persistent_id("sidebar_add"),
                        rect,
                        glyph::PLUS,
                        18.0,
                        ON_SURFACE_DIM,
                    )
                    .on_hover_text(crate::i18n::tr(ui.ctx()).add_category)
                    .clicked()
                    {
                        on_open_add();
                    }
                });
            });
        });

    let rows = compute_sidebar_rows(
        entries,
        interaction.collapsed_groups(),
        state.selected_entry.as_ref(),
    );
    let mut events: Vec<(usize, RowEvent)> = Vec::new();

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin::symmetric(8, 0))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = ROW_GAP;
                    let mut dragged = None;
                    for (index, row) in rows.iter().enumerate() {
                        let height = match row {
                            SidebarRow::FolderHeader { .. } => FOLDER_HEIGHT,
                            SidebarRow::Entry { .. } => ENTRY_HEIGHT,
                        };
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), height),
                            Sense::hover(),
                        );
                        let key = row.key();
                        if interaction.dragging_key() == Some(key.as_str()) {
                            // Painted last so it floats above its neighbours.
                            dragged = Some((index, rect));
                            continue;
                        }
                        let ctx = RowContext {
                            state,
                            dragging: false,
                        };
                        for event in render_row(ui, row, rect, &ctx) {
                            events.push((index, event));
                        }
                    }
                    if let Some((index, rect)) = dragged {
                        let rect = rect.translate(vec2(0.0, interaction.drag_offset_y()));
                        let ctx = RowContext {
                            state,
                            dragging: true,
                        };
                        for event in render_row(ui, &rows[index], rect, &ctx) {
                            events.push((index, event));
                        }
                    }
                });
        });

    for (index, event) in events {
        let row = &rows[index];
        match event {
            RowEvent::Select(entry) => on_action(UiAction::SelectEntry(Some(entry))),
            RowEvent::ActivateFolder => {
                if let Some(entry) = interaction.activate_folder(row) {
                    on_action(UiAction::SelectEntry(Some(entry)));
                }
            }
            RowEvent::ToggleCollapse => {
                if let SidebarRow::FolderHeader {
                    group_name,
                    is_collapsed,
                    ..
                } = row
                {
                    interaction.toggle_group(group_name, *is_collapsed);
                }
            }
            RowEvent::Move(direction) => {
                let reordered = match row {
                    SidebarRow::FolderHeader { unit_index, .. } => {
                        match unit_index.checked_add_signed(direction) {
                            Some(to) => move_top_level_unit(entries, *unit_index, to),
                            None => continue,
                        }
                    }
                    SidebarRow::Entry { .. } => move_sidebar_entry(entries, row, direction),
                };
                if reordered != entries {
                    on_action(UiAction::ReorderEntries(reordered));
                }
            }
            RowEvent::ToggleSteam(entry) => on_action(UiAction::ToggleSteam(entry)),
            RowEvent::DragStart => interaction.start_drag(row.key()),
            RowEvent::Drag(delta) => {
                let reordered = match row {
                    SidebarRow::FolderHeader { .. } => {
                        interaction.drag_folder(entries, row, delta, DRAG_STEP)
                    }
                    SidebarRow::Entry { .. } => {
                        interaction.drag_entry(entries, row, delta, DRAG_STEP)
                    }
                };
                if let Some(reordered) = reordered {
                    on_action(UiAction::ReorderEntries(reordered));
                }
            }
            RowEvent::DragEnd => interaction.end_drag(),
        }
    }
}

struct RowContext<'a> {
    state: &'a AppUiState,
    dragging: bool,
}
