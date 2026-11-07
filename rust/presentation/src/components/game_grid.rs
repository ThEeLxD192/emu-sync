//! Game grid component displaying scanned games and cards.

use crate::{
    actions::UiAction,
    components::icons::{paint_pencil_icon, paint_play_triangle, render_cover_placeholder},
    cover_cache::CoverCache,
    model::GameItem,
    state::AppUiState,
    theme::{
        CARD_HOVER, DIVIDER, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, PRIMARY_CONTAINER,
        SURFACE, SURFACE_VARIANT,
    },
};
use egui::{
    Align, Button, Color32, CornerRadius, Frame, Label, Layout, Margin, Pos2, Rect, RichText,
    ScrollArea, Sense, Stroke, StrokeKind, Ui, Vec2,
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

                    // Sync button & Check sync
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
    let card_width = 168.0;
    let card_height = 205.0;

    let mut play_clicked = false;
    let mut edit_clicked = false;

    let is_enabled = ui.is_enabled();
    let (card_rect, card_response) = ui.allocate_exact_size(
        Vec2::new(card_width, card_height),
        if is_enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );

    let pointer_pos = ui.input(|i| i.pointer.hover_pos());
    let is_hovered = is_enabled && pointer_pos.is_some_and(|pos| card_rect.contains(pos));

    if is_hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if is_enabled && card_response.clicked() {
        play_clicked = true;
    }

    if ui.is_rect_visible(card_rect) {
        let fill = if is_hovered {
            CARD_HOVER
        } else {
            SURFACE_VARIANT
        };
        let stroke = if is_hovered {
            Stroke::new(1.5, PRIMARY)
        } else {
            Stroke::new(1.0, DIVIDER)
        };

        ui.painter()
            .rect_filled(card_rect, CornerRadius::same(12), fill);
        ui.painter().rect_stroke(
            card_rect,
            CornerRadius::same(12),
            stroke,
            StrokeKind::Inside,
        );

        let inner_rect = card_rect.shrink(8.0);

        // 1. Cover Art area (top 140px)
        let cover_height = 140.0;
        let cover_rect =
            Rect::from_min_size(inner_rect.min, Vec2::new(inner_rect.width(), cover_height));

        let cover_path = item.effective_cover_path();
        let cover_tex = cover_path
            .as_deref()
            .and_then(|p| cover_cache.get_or_load(ui.ctx(), p));

        if let Some(texture) = cover_tex {
            // Paint the image fitted inside cover_rect
            let tex_size = texture.size_vec2();
            let aspect = tex_size.x / tex_size.y.max(1.0);
            let target_w = (cover_height * aspect).min(cover_rect.width());
            let target_h = (target_w / aspect).min(cover_height);
            let image_rect =
                Rect::from_center_size(cover_rect.center(), Vec2::new(target_w, target_h));

            ui.painter().image(
                texture.id(),
                image_rect,
                Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );

            // Play overlay on hover
            if is_hovered {
                ui.painter().rect_filled(
                    cover_rect,
                    CornerRadius::same(8),
                    Color32::from_black_alpha(115),
                );
                paint_play_triangle(ui, cover_rect, Color32::WHITE, 1.4);
            }
        } else {
            // Fallback: rounded container with play icon matching Compose Desktop
            render_cover_placeholder(ui, cover_rect);
        }

        // 2. Title area below cover
        let title_rect = Rect::from_min_size(
            Pos2::new(inner_rect.min.x, inner_rect.min.y + cover_height + 6.0),
            Vec2::new(inner_rect.width(), inner_rect.height() - cover_height - 6.0),
        );

        let title = item.effective_title();
        let mut title_ui = ui.new_child(egui::UiBuilder::new().max_rect(title_rect));
        title_ui.vertical_centered(|ui| {
            ui.add(
                Label::new(
                    RichText::new(&title)
                        .size(12.0)
                        .strong()
                        .color(if is_hovered {
                            ON_BACKGROUND
                        } else {
                            ON_SURFACE
                        }),
                )
                .truncate(),
            );
        });

        // 3. Top-right Edit button shown on hover
        if is_hovered {
            let edit_btn_size = 24.0;
            let edit_rect = Rect::from_min_size(
                Pos2::new(card_rect.max.x - edit_btn_size - 6.0, card_rect.min.y + 6.0),
                Vec2::new(edit_btn_size, edit_btn_size),
            );

            let edit_id = ui.make_persistent_id(format!("edit_btn_{}", item.name));
            let edit_resp = ui.interact(
                edit_rect,
                edit_id,
                if is_enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );

            let bg_color = if edit_resp.hovered() && is_enabled {
                Color32::from_black_alpha(210)
            } else {
                Color32::from_black_alpha(150)
            };
            ui.painter()
                .circle_filled(edit_rect.center(), edit_btn_size * 0.5, bg_color);
            paint_pencil_icon(ui, edit_rect, Color32::WHITE);

            if is_enabled && edit_resp.clicked() {
                edit_clicked = true;
                play_clicked = false;
            }
        }
    }

    if is_enabled {
        if edit_clicked {
            on_edit_game(item.clone());
        } else if play_clicked {
            on_action(UiAction::LaunchGame(item.clone()));
        }
    }
}
