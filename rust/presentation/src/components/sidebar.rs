//! Sidebar listing library folders and entries, laid out like Kotlin's `Sidebar`.

use crate::{
    actions::UiAction,
    components::{
        icons::{glyph_button, paint_glyph},
        sidebar_order::{
            SidebarInteraction, SidebarRow, compute_sidebar_rows, move_sidebar_entry,
            move_top_level_unit,
        },
    },
    state::AppUiState,
    theme::{
        CARD_HOVER, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SURFACE, SURFACE_SELECTED,
        SURFACE_VARIANT, bold, glyph,
    },
};
use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Margin, Pos2, Rect, RichText, ScrollArea, Sense,
    Shadow, Ui, pos2, text::LayoutJob, vec2,
};
use emusync_domain::library::GameEntry;

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
    let entries: Vec<GameEntry> = state
        .config
        .as_ref()
        .map(|c| c.entries.clone())
        .unwrap_or_default();

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
                    RichText::new("LIBRARY")
                        .size(11.0)
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
                    .on_hover_text("Add entry")
                    .clicked()
                    {
                        on_open_add();
                    }
                });
            });
        });

    let rows = compute_sidebar_rows(
        &entries,
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
                        if interaction.dragging_key() == Some(row.key().as_str()) {
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
                            Some(to) => move_top_level_unit(&entries, *unit_index, to),
                            None => continue,
                        }
                    }
                    SidebarRow::Entry { .. } => move_sidebar_entry(&entries, row, direction),
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
                        interaction.drag_folder(&entries, row, delta, DRAG_STEP)
                    }
                    SidebarRow::Entry { .. } => {
                        interaction.drag_entry(&entries, row, delta, DRAG_STEP)
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

fn render_row(ui: &mut Ui, row: &SidebarRow, rect: Rect, ctx: &RowContext<'_>) -> Vec<RowEvent> {
    let mut events = Vec::new();
    let key = row.key();
    let rect = match row {
        SidebarRow::Entry {
            is_nested: true, ..
        } => Rect::from_min_max(pos2(rect.min.x + NESTED_INDENT, rect.min.y), rect.max),
        _ => rect,
    };

    // Registered first so the handle and buttons registered later sit on top of it.
    let row_response = ui
        .interact(
            rect,
            ui.make_persistent_id(("sidebar_row", &key)),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    let hovered = ui.rect_contains_pointer(rect);

    if ctx.dragging {
        ui.painter().add(
            Shadow {
                offset: [0, 3],
                blur: 16,
                spread: 0,
                color: Color32::from_black_alpha(140),
            }
            .as_shape(rect, CornerRadius::same(ROW_RADIUS)),
        );
    }

    match row {
        SidebarRow::FolderHeader {
            group_name,
            count,
            is_collapsed,
            has_selected_child,
            unit_index,
            total_units,
            ..
        } => {
            let fill = if ctx.dragging {
                SURFACE_SELECTED.gamma_multiply(0.9)
            } else if *has_selected_child && *is_collapsed {
                SURFACE_SELECTED.gamma_multiply(0.5)
            } else if hovered {
                CARD_HOVER
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(rect, ROW_RADIUS, fill);

            let mut left = rect.left() + 8.0;
            drag_handle(
                ui,
                &key,
                rect,
                &mut left,
                hovered,
                ctx.dragging,
                &mut events,
            );
            left += 4.0;
            let folder_color = if *has_selected_child {
                PRIMARY
            } else {
                PRIMARY.gamma_multiply(0.85)
            };
            paint_glyph(
                ui,
                pos2(left + 9.0, rect.center().y),
                glyph::FOLDER_OPEN,
                18.0,
                folder_color,
            );
            left += 18.0 + 8.0;

            let mut right = rect.right() - 8.0;
            let chevron =
                Rect::from_center_size(pos2(right - 12.0, rect.center().y), vec2(24.0, 24.0));
            right -= 24.0 + 4.0;
            let chevron_glyph = if *is_collapsed {
                glyph::CHEVRON_RIGHT
            } else {
                glyph::CHEVRON_DOWN
            };
            if glyph_button(
                ui,
                ui.make_persistent_id(("sidebar_collapse", &key)),
                chevron,
                chevron_glyph,
                18.0,
                ON_SURFACE_DIM,
            )
            .on_hover_text(if *is_collapsed { "Expand" } else { "Collapse" })
            .clicked()
            {
                events.push(RowEvent::ToggleCollapse);
            }

            if hovered && !ctx.dragging {
                reorder_buttons(
                    ui,
                    &key,
                    rect,
                    &mut right,
                    *unit_index > 0,
                    *unit_index + 1 < *total_units,
                    &mut events,
                );
                right -= 4.0;
            }

            // Count badge.
            let badge_text = ui.painter().layout_no_wrap(
                count.to_string(),
                FontId::proportional(11.0),
                ON_SURFACE_DIM,
            );
            let badge = Rect::from_min_max(
                pos2(right - badge_text.size().x - 12.0, rect.center().y - 9.0),
                pos2(right, rect.center().y + 9.0),
            );
            ui.painter().rect_filled(badge, 10, SURFACE_VARIANT);
            ui.painter().galley(
                badge.center() - badge_text.size() / 2.0,
                badge_text,
                ON_SURFACE_DIM,
            );
            right = badge.left() - 6.0;

            let name_color = if *has_selected_child {
                PRIMARY
            } else {
                ON_BACKGROUND
            };
            paint_truncated(
                ui,
                pos2(left, rect.center().y),
                group_name,
                bold(13.0),
                name_color,
                right - left,
            );

            if row_response.clicked() && !ctx.dragging {
                events.push(RowEvent::ActivateFolder);
            }
        }
        SidebarRow::Entry {
            entry,
            is_nested,
            sibling_index,
            total_siblings,
            unit_index,
            total_units,
            ..
        } => {
            let is_selected = ctx
                .state
                .selected_entry
                .as_ref()
                .is_some_and(|s| s.common().name == entry.common().name);
            let fill = if ctx.dragging {
                SURFACE_SELECTED.gamma_multiply(0.9)
            } else if is_selected {
                SURFACE_SELECTED
            } else if hovered {
                CARD_HOVER
            } else {
                SURFACE
            };
            let text_color = if is_selected {
                PRIMARY
            } else if hovered {
                ON_BACKGROUND
            } else {
                ON_SURFACE
            };
            ui.painter().rect_filled(rect, ROW_RADIUS, fill);

            let mut left = rect.left() + 8.0;
            drag_handle(
                ui,
                &key,
                rect,
                &mut left,
                hovered,
                ctx.dragging,
                &mut events,
            );
            left += 4.0;
            let icon = match entry {
                GameEntry::Emulator(_) => glyph::CONTROLLER,
                GameEntry::Native(_) => glyph::GAMEPAD,
            };
            paint_glyph(
                ui,
                pos2(left + 10.0, rect.center().y),
                icon,
                20.0,
                text_color,
            );
            left += 20.0 + 8.0;

            let mut right = rect.right() - 8.0;
            if is_selected {
                let bar = Rect::from_min_max(
                    pos2(right - 3.0, rect.center().y - 12.0),
                    pos2(right, rect.center().y + 12.0),
                );
                ui.painter().rect_filled(bar, 2, PRIMARY);
                right -= 3.0;
            }
            if ctx.state.steam_available {
                right -= 4.0;
                let registered = ctx.state.is_steam_registered(entry);
                let steam =
                    Rect::from_center_size(pos2(right - 12.0, rect.center().y), vec2(24.0, 24.0));
                right -= 24.0;
                let color = if registered {
                    STEAM_REGISTERED
                } else {
                    ON_SURFACE_DIM.gamma_multiply(0.4)
                };
                if glyph_button(
                    ui,
                    ui.make_persistent_id(("sidebar_steam", &key)),
                    steam,
                    glyph::WIDGETS,
                    16.0,
                    color,
                )
                .on_hover_text(if registered {
                    "Unregister from Steam"
                } else {
                    "Register in Steam"
                })
                .clicked()
                {
                    events.push(RowEvent::ToggleSteam(entry.clone()));
                }
            }
            if hovered && !ctx.dragging {
                let (position, count) = if *is_nested {
                    (*sibling_index, *total_siblings)
                } else {
                    (*unit_index, *total_units)
                };
                reorder_buttons(
                    ui,
                    &key,
                    rect,
                    &mut right,
                    position > 0,
                    position + 1 < count,
                    &mut events,
                );
            }

            let width = right - left - 4.0;
            let title_font = if is_selected {
                bold(13.0)
            } else {
                FontId::proportional(13.0)
            };
            let subtitle = match entry {
                GameEntry::Emulator(emulator) => emulator
                    .extensions
                    .iter()
                    .map(|ext| format!(".{ext}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                GameEntry::Native(_) => "Native".to_string(),
            };
            paint_truncated(
                ui,
                pos2(left, rect.center().y - 8.0),
                &entry.common().name,
                title_font,
                text_color,
                width,
            );
            paint_truncated(
                ui,
                pos2(left, rect.center().y + 9.0),
                &subtitle,
                FontId::proportional(11.0),
                ON_SURFACE_DIM,
                width,
            );

            if row_response.clicked() && !ctx.dragging {
                events.push(RowEvent::Select(entry.clone()));
            }
        }
    }
    events
}

fn drag_handle(
    ui: &mut Ui,
    key: &str,
    rect: Rect,
    left: &mut f32,
    row_hovered: bool,
    dragging: bool,
    events: &mut Vec<RowEvent>,
) {
    let handle = Rect::from_min_size(pos2(*left, rect.center().y - 12.0), vec2(24.0, 24.0));
    *left += 24.0;
    let response = ui.interact(
        handle,
        ui.make_persistent_id(("sidebar_drag", key)),
        Sense::drag(),
    );
    let cursor = if response.dragged() {
        CursorIcon::Grabbing
    } else {
        CursorIcon::Grab
    };
    let response = response.on_hover_cursor(cursor);
    if response.drag_started() {
        events.push(RowEvent::DragStart);
    }
    if response.dragged() {
        let delta = response.drag_delta().y;
        if delta != 0.0 {
            events.push(RowEvent::Drag(delta));
        }
    }
    if response.drag_stopped() {
        events.push(RowEvent::DragEnd);
    }
    let color = if dragging {
        PRIMARY
    } else {
        ON_SURFACE_DIM.gamma_multiply(if row_hovered { 0.8 } else { 0.35 })
    };
    paint_glyph(ui, handle.center(), glyph::DRAG, 16.0, color);
}

fn reorder_buttons(
    ui: &mut Ui,
    key: &str,
    rect: Rect,
    right: &mut f32,
    can_move_up: bool,
    can_move_down: bool,
    events: &mut Vec<RowEvent>,
) {
    // Laid out right to left: down sits closest to the edge, as in Kotlin's Row.
    for (enabled, direction, glyph, label) in [
        (can_move_down, 1, glyph::CHEVRON_DOWN, "Move down"),
        (can_move_up, -1, glyph::CHEVRON_UP, "Move up"),
    ] {
        if !enabled {
            continue;
        }
        let button = Rect::from_center_size(pos2(*right - 10.0, rect.center().y), vec2(20.0, 20.0));
        *right -= 20.0;
        if glyph_button(
            ui,
            ui.make_persistent_id(("sidebar_move", key, direction)),
            button,
            glyph,
            14.0,
            ON_SURFACE_DIM,
        )
        .on_hover_text(label)
        .clicked()
        {
            events.push(RowEvent::Move(direction));
        }
    }
}

/// Paints single-line text vertically centered on `left_center`, truncated with "…".
fn paint_truncated(
    ui: &Ui,
    left_center: Pos2,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) {
    if max_width <= 0.0 {
        return;
    }
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width);
    let galley = ui.painter().layout_job(job);
    let pos = pos2(left_center.x, left_center.y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, color);
}
