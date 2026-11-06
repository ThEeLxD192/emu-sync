//! Game grid component displaying scanned games and cards.

use crate::{
    actions::UiAction,
    cover_cache::CoverCache,
    model::GameItem,
    state::AppUiState,
    theme::{
        DIVIDER, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, PRIMARY_CONTAINER, SURFACE,
        SURFACE_VARIANT,
    },
};
use egui::{
    Align, Button, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke, Ui,
    Vec2,
};
use emusync_domain::library::GameEntry;

/// Renders the main content area with category controls and the game card grid.
pub fn render_game_grid(
    ui: &mut Ui,
    state: &AppUiState,
    cover_cache: &mut CoverCache,
    on_action: &mut impl FnMut(UiAction),
    on_edit_game: &mut impl FnMut(GameItem),
    on_edit_category: &mut impl FnMut(GameEntry),
    on_delete_category: &mut impl FnMut(GameEntry),
) {
    let Some(selected) = &state.selected_entry else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Select a category from the sidebar or click \"+ Add\" to begin.")
                    .size(14.0)
                    .color(ON_SURFACE_DIM),
            );
        });
        return;
    };

    let selected_name = selected.common().name.clone();

    // Category Header Toolbar
    Frame::new()
        .fill(SURFACE)
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&selected_name)
                        .size(18.0)
                        .strong()
                        .color(ON_BACKGROUND),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Games count chip
                    let count = state.game_items.len();
                    let count_label = format!("{count} game{}", if count == 1 { "" } else { "s" });
                    Frame::new()
                        .fill(PRIMARY_CONTAINER)
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(10, 4))
                        .show(ui, |ui| {
                            ui.label(RichText::new(count_label).size(11.0).color(PRIMARY));
                        });

                    ui.add_space(8.0);

                    // Delete category
                    if ui
                        .add(
                            Button::new(RichText::new("Delete").size(11.0).color(ON_SURFACE_DIM))
                                .min_size(Vec2::new(54.0, 24.0)),
                        )
                        .clicked()
                    {
                        on_delete_category(selected.clone());
                    }

                    // Edit category
                    if ui
                        .add(
                            Button::new(RichText::new("Edit").size(11.0).color(ON_SURFACE))
                                .min_size(Vec2::new(48.0, 24.0)),
                        )
                        .clicked()
                    {
                        on_edit_category(selected.clone());
                    }

                    ui.add_space(8.0);

                    // Sync buttons
                    let sync_status = state.sync_status_for_selected();
                    let status_color = sync_status.color();

                    if ui
                        .add(
                            Button::new(
                                RichText::new(format!("Sync: {}", sync_status.label()))
                                    .size(11.0)
                                    .color(status_color),
                            )
                            .min_size(Vec2::new(90.0, 24.0)),
                        )
                        .clicked()
                    {
                        on_action(UiAction::SyncEntry(selected.clone()));
                    }

                    if ui
                        .add(
                            Button::new(RichText::new("Check").size(11.0).color(ON_SURFACE))
                                .min_size(Vec2::new(48.0, 24.0)),
                        )
                        .clicked()
                    {
                        on_action(UiAction::CheckSyncEntry(selected.clone()));
                    }

                    // Open Emulator button (if emulator)
                    if let GameEntry::Emulator(emu) = selected {
                        ui.add_space(8.0);
                        if ui
                            .add(
                                Button::new(
                                    RichText::new("Open Emulator").size(11.0).color(PRIMARY),
                                )
                                .min_size(Vec2::new(95.0, 24.0)),
                            )
                            .clicked()
                        {
                            on_action(UiAction::OpenEmulator(emu.clone()));
                        }
                    }
                });
            });
        });

    ui.add_space(8.0);

    // Main game grid / loading / empty state
    if state.is_loading {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("Loading games...").size(14.0).color(PRIMARY));
        });
        return;
    }

    if state.game_items.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No games found").size(15.0).color(ON_SURFACE));
                ui.add_space(4.0);
                if let GameEntry::Emulator(emu) = selected {
                    ui.label(
                        RichText::new(format!("Place ROMs in: {}", emu.roms_directory))
                            .size(12.0)
                            .color(ON_SURFACE_DIM),
                    );
                }
            });
        });
        return;
    }

    ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for item in &state.game_items {
                render_game_card(ui, item, cover_cache, on_action, on_edit_game);
            }
        });
    });
}

fn render_game_card(
    ui: &mut Ui,
    item: &GameItem,
    cover_cache: &mut CoverCache,
    on_action: &mut impl FnMut(UiAction),
    on_edit_game: &mut impl FnMut(GameItem),
) {
    let card_width = 170.0;
    let card_height = 215.0;

    Frame::new()
        .fill(SURFACE_VARIANT)
        .stroke(Stroke::new(1.0, DIVIDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(card_width);
            ui.set_height(card_height);

            ui.vertical(|ui| {
                // Cover preview area
                let cover_path = item.effective_cover_path();
                let cover_tex = cover_path
                    .as_deref()
                    .and_then(|p| cover_cache.get_or_load(ui.ctx(), p));

                let image_height = 135.0;

                if let Some(texture) = cover_tex {
                    ui.add(egui::Image::new(texture).max_size(Vec2::new(card_width, image_height)));
                } else {
                    Frame::new()
                        .fill(SURFACE)
                        .corner_radius(CornerRadius::same(6))
                        .show(ui, |ui| {
                            ui.set_width(card_width);
                            ui.set_height(image_height);
                            ui.centered_and_justified(|ui| {
                                ui.label(RichText::new("🎮").size(32.0).color(ON_SURFACE_DIM));
                            });
                        });
                }

                ui.add_space(6.0);

                // Effective game title
                let title = item.effective_title();
                ui.label(
                    RichText::new(&title)
                        .size(12.0)
                        .strong()
                        .color(ON_BACKGROUND),
                );

                ui.add_space(4.0);

                // Action buttons: Play and Edit
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            Button::new(
                                RichText::new("▶ Play")
                                    .size(11.0)
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(PRIMARY)
                            .min_size(Vec2::new(72.0, 22.0)),
                        )
                        .clicked()
                    {
                        on_action(UiAction::LaunchGame(item.clone()));
                    }

                    if ui
                        .add(
                            Button::new(RichText::new("⚙ Edit").size(11.0).color(ON_SURFACE))
                                .min_size(Vec2::new(54.0, 22.0)),
                        )
                        .clicked()
                    {
                        on_edit_game(item.clone());
                    }
                });
            });
        });
}
