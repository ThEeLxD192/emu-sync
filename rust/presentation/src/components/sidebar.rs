//! Sidebar component listing categories and entries.

use crate::{
    actions::UiAction,
    state::AppUiState,
    theme::{
        DIVIDER, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SURFACE, SURFACE_SELECTED,
        SURFACE_VARIANT,
    },
};
use egui::{
    Align, Button, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke, Ui, Vec2,
};
use emusync_domain::library::GameEntry;

/// Renders the left sidebar containing categories, group organization, and reordering.
pub fn render_sidebar(
    ui: &mut Ui,
    state: &AppUiState,
    on_action: &mut impl FnMut(UiAction),
    on_open_add: &mut impl FnMut(),
) {
    let entries: Vec<GameEntry> = state
        .config
        .as_ref()
        .map(|c| c.entries.clone())
        .unwrap_or_default();

    Frame::new()
        .fill(SURFACE)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            // Header with LIBRARY label and Add button
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("LIBRARY")
                        .size(11.0)
                        .strong()
                        .color(ON_SURFACE_DIM),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            Button::new(RichText::new("+ Add").size(11.0).color(PRIMARY))
                                .min_size(Vec2::new(48.0, 22.0)),
                        )
                        .clicked()
                    {
                        on_open_add();
                    }
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // Scrollable list of categories
            ScrollArea::vertical().show(ui, |ui| {
                if entries.is_empty() {
                    ui.add_space(20.0);
                    ui.label(
                        RichText::new("No categories added yet.")
                            .size(12.0)
                            .color(ON_SURFACE_DIM),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Click \"+ Add\" above to add an emulator or PC game.")
                            .size(11.0)
                            .color(ON_SURFACE_DIM),
                    );
                    return;
                }

                let total_entries = entries.len();

                for (index, entry) in entries.iter().enumerate() {
                    let entry_name = &entry.common().name;
                    let is_selected = state
                        .selected_entry
                        .as_ref()
                        .is_some_and(|s| &s.common().name == entry_name);

                    let row_fill = if is_selected {
                        SURFACE_SELECTED
                    } else {
                        SURFACE_VARIANT
                    };

                    let stroke = if is_selected {
                        Stroke::new(1.0, PRIMARY)
                    } else {
                        Stroke::new(1.0, DIVIDER)
                    };

                    Frame::new()
                        .fill(row_fill)
                        .stroke(stroke)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Category title and selection
                                let title_text =
                                    RichText::new(entry_name).size(13.0).color(if is_selected {
                                        ON_BACKGROUND
                                    } else {
                                        ON_SURFACE
                                    });

                                if ui.selectable_label(is_selected, title_text).clicked() {
                                    on_action(UiAction::SelectEntry(Some(entry.clone())));
                                }

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    // Move down button
                                    let can_move_down = index + 1 < total_entries;
                                    if ui
                                        .add_enabled(
                                            can_move_down,
                                            Button::new(
                                                RichText::new("▼").size(10.0).color(ON_SURFACE_DIM),
                                            )
                                            .min_size(Vec2::new(18.0, 18.0)),
                                        )
                                        .clicked()
                                    {
                                        let mut new_entries = entries.clone();
                                        new_entries.swap(index, index + 1);
                                        on_action(UiAction::ReorderEntries(new_entries));
                                    }

                                    // Move up button
                                    let can_move_up = index > 0;
                                    if ui
                                        .add_enabled(
                                            can_move_up,
                                            Button::new(
                                                RichText::new("▲").size(10.0).color(ON_SURFACE_DIM),
                                            )
                                            .min_size(Vec2::new(18.0, 18.0)),
                                        )
                                        .clicked()
                                    {
                                        let mut new_entries = entries.clone();
                                        new_entries.swap(index, index - 1);
                                        on_action(UiAction::ReorderEntries(new_entries));
                                    }

                                    // Steam toggle
                                    if state.steam_available {
                                        let is_steam = state.is_steam_registered(entry);
                                        let steam_label =
                                            if is_steam { "Steam ✓" } else { "+Steam" };
                                        let steam_color =
                                            if is_steam { PRIMARY } else { ON_SURFACE_DIM };

                                        if ui
                                            .add(
                                                Button::new(
                                                    RichText::new(steam_label)
                                                        .size(10.0)
                                                        .color(steam_color),
                                                )
                                                .min_size(Vec2::new(42.0, 18.0)),
                                            )
                                            .clicked()
                                        {
                                            on_action(UiAction::ToggleSteam(entry.clone()));
                                        }
                                    }
                                });
                            });
                        });

                    ui.add_space(4.0);
                }
            });
        });
}
