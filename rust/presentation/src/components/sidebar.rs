//! Sidebar component listing categories and entries.

use crate::{
    actions::UiAction,
    components::icons::{paint_arrow_down, paint_arrow_up},
    state::AppUiState,
    theme::{
        CARD_HOVER, DIVIDER, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SUCCESS, SURFACE,
        SURFACE_SELECTED, SURFACE_VARIANT,
    },
};
use egui::{
    Align, Align2, Button, Color32, CornerRadius, FontId, Frame, Label, Layout, Margin, Pos2, Rect,
    RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui, Vec2,
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

                    let row_size = Vec2::new(ui.available_width(), 44.0);
                    let (row_rect, row_response) = ui.allocate_exact_size(row_size, Sense::click());

                    let pointer_pos = ui.input(|i| i.pointer.hover_pos());
                    let is_row_hovered = pointer_pos.is_some_and(|pos| row_rect.contains(pos));

                    let fill = if is_selected {
                        SURFACE_SELECTED
                    } else if is_row_hovered {
                        CARD_HOVER
                    } else {
                        SURFACE_VARIANT
                    };

                    let stroke = if is_selected {
                        Stroke::new(1.5, PRIMARY)
                    } else {
                        Stroke::new(1.0, DIVIDER)
                    };

                    if ui.is_rect_visible(row_rect) {
                        ui.painter()
                            .rect_filled(row_rect, CornerRadius::same(8), fill);
                        ui.painter().rect_stroke(
                            row_rect,
                            CornerRadius::same(8),
                            stroke,
                            StrokeKind::Inside,
                        );

                        let mut right_x = row_rect.max.x - 8.0;
                        let mut move_down_clicked = false;
                        let mut move_up_clicked = false;
                        let mut steam_clicked = false;

                        // Reorder buttons (always visible when there are multiple categories)
                        if total_entries > 1 {
                            let btn_size = 20.0;

                            // Down button
                            right_x -= btn_size;
                            let down_rect = Rect::from_min_size(
                                Pos2::new(right_x, row_rect.center().y - btn_size * 0.5),
                                Vec2::splat(btn_size),
                            );
                            let can_move_down = index + 1 < total_entries;
                            let down_id =
                                ui.make_persistent_id(format!("mv_dn_{index}_{entry_name}"));
                            let down_resp = ui.interact(
                                down_rect,
                                down_id,
                                if can_move_down {
                                    Sense::click()
                                } else {
                                    Sense::hover()
                                },
                            );
                            if down_resp.hovered() && can_move_down {
                                ui.painter().rect_filled(
                                    down_rect,
                                    CornerRadius::same(4),
                                    ui.visuals().widgets.hovered.bg_fill,
                                );
                            }
                            let down_color = if can_move_down {
                                if down_resp.hovered() {
                                    PRIMARY
                                } else {
                                    ON_SURFACE
                                }
                            } else {
                                ON_SURFACE_DIM.gamma_multiply(0.35)
                            };
                            paint_arrow_down(ui, down_rect, down_color);
                            if down_resp.clicked() {
                                move_down_clicked = true;
                            }

                            right_x -= 3.0;

                            // Up button
                            right_x -= btn_size;
                            let up_rect = Rect::from_min_size(
                                Pos2::new(right_x, row_rect.center().y - btn_size * 0.5),
                                Vec2::splat(btn_size),
                            );
                            let can_move_up = index > 0;
                            let up_id =
                                ui.make_persistent_id(format!("mv_up_{index}_{entry_name}"));
                            let up_resp = ui.interact(
                                up_rect,
                                up_id,
                                if can_move_up {
                                    Sense::click()
                                } else {
                                    Sense::hover()
                                },
                            );
                            if up_resp.hovered() && can_move_up {
                                ui.painter().rect_filled(
                                    up_rect,
                                    CornerRadius::same(4),
                                    ui.visuals().widgets.hovered.bg_fill,
                                );
                            }
                            let up_color = if can_move_up {
                                if up_resp.hovered() {
                                    PRIMARY
                                } else {
                                    ON_SURFACE
                                }
                            } else {
                                ON_SURFACE_DIM.gamma_multiply(0.35)
                            };
                            paint_arrow_up(ui, up_rect, up_color);
                            if up_resp.clicked() {
                                move_up_clicked = true;
                            }
                        }

                        // Steam toggle button
                        if state.steam_available {
                            let steam_w = 44.0;
                            let steam_h = 20.0;
                            right_x -= steam_w + 4.0;
                            let steam_rect = Rect::from_min_size(
                                Pos2::new(right_x, row_rect.center().y - steam_h * 0.5),
                                Vec2::new(steam_w, steam_h),
                            );
                            let is_steam = state.is_steam_registered(entry);
                            let steam_id =
                                ui.make_persistent_id(format!("stm_{index}_{entry_name}"));
                            let steam_resp = ui.interact(steam_rect, steam_id, Sense::click());
                            let (steam_text, steam_color) = if is_steam {
                                ("Steam", SUCCESS)
                            } else {
                                ("+Steam", ON_SURFACE_DIM)
                            };
                            let steam_bg = if steam_resp.hovered() {
                                ui.visuals().widgets.hovered.bg_fill
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter()
                                .rect_filled(steam_rect, CornerRadius::same(4), steam_bg);
                            ui.painter().rect_stroke(
                                steam_rect,
                                CornerRadius::same(4),
                                Stroke::new(1.0, steam_color.gamma_multiply(0.5)),
                                StrokeKind::Inside,
                            );
                            ui.painter().text(
                                steam_rect.center(),
                                Align2::CENTER_CENTER,
                                steam_text,
                                FontId::proportional(10.0),
                                steam_color,
                            );
                            if steam_resp.clicked() {
                                steam_clicked = true;
                            }
                        }

                        // Category Title & Subtitle bounded strictly to the left of all buttons
                        let text_min_x = row_rect.min.x + 10.0;
                        let text_max_x = (right_x - 6.0).max(text_min_x + 20.0);
                        let text_rect = Rect::from_min_max(
                            Pos2::new(text_min_x, row_rect.min.y + 4.0),
                            Pos2::new(text_max_x, row_rect.max.y - 4.0),
                        );

                        let mut text_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
                        text_ui.vertical(|ui| {
                            let title_color = if is_selected {
                                PRIMARY
                            } else if is_row_hovered {
                                ON_BACKGROUND
                            } else {
                                ON_SURFACE
                            };
                            ui.add(
                                Label::new(
                                    RichText::new(entry_name)
                                        .size(13.0)
                                        .strong()
                                        .color(title_color),
                                )
                                .truncate(),
                            );
                            let subtitle = match entry {
                                GameEntry::Emulator(emu) => {
                                    emu.common.group.as_deref().unwrap_or("Emulator System")
                                }
                                GameEntry::Native(_) => "Native PC Game",
                            };
                            ui.add(
                                Label::new(
                                    RichText::new(subtitle).size(10.0).color(ON_SURFACE_DIM),
                                )
                                .truncate(),
                            );
                        });

                        // Actions dispatch
                        if move_down_clicked {
                            let mut new_entries = entries.clone();
                            new_entries.swap(index, index + 1);
                            on_action(UiAction::ReorderEntries(new_entries));
                        } else if move_up_clicked {
                            let mut new_entries = entries.clone();
                            new_entries.swap(index, index - 1);
                            on_action(UiAction::ReorderEntries(new_entries));
                        } else if steam_clicked {
                            on_action(UiAction::ToggleSteam(entry.clone()));
                        } else if row_response.clicked() {
                            on_action(UiAction::SelectEntry(Some(entry.clone())));
                        }
                    }

                    ui.add_space(6.0);
                }
            });
        });
}
