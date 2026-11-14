//! Folder and entry rows of the sidebar.

use super::{
    NESTED_INDENT, ROW_RADIUS, RowContext, RowEvent, STEAM_REGISTERED,
    controls::{drag_handle, paint_truncated, reorder_buttons},
};
use crate::{
    components::{
        icons::{glyph_button, paint_glyph},
        sidebar_order::SidebarRow,
    },
    state::AppUiState,
    theme::{
        CARD_HOVER, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SURFACE, SURFACE_SELECTED,
        SURFACE_VARIANT, bold, glyph,
    },
};
use egui::{Color32, CornerRadius, CursorIcon, FontId, Rect, Sense, Shadow, Ui, pos2, vec2};
use emusync_domain::library::GameEntry;

/// What every row variant needs once its frame is laid out and hit-tested.
struct RowFrame<'a> {
    rect: Rect,
    key: &'a str,
    hovered: bool,
    clicked: bool,
    dragging: bool,
    state: &'a AppUiState,
}

pub(super) fn render_row(
    ui: &mut Ui,
    row: &SidebarRow,
    rect: Rect,
    ctx: &RowContext<'_>,
) -> Vec<RowEvent> {
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

    let frame = RowFrame {
        rect,
        key: &key,
        hovered,
        clicked: row_response.clicked(),
        dragging: ctx.dragging,
        state: ctx.state,
    };

    match row {
        SidebarRow::FolderHeader { .. } => folder_row(ui, &frame, row, &mut events),
        SidebarRow::Entry { .. } => entry_row(ui, &frame, row, &mut events),
    }
    events
}

fn folder_row(ui: &mut Ui, frame: &RowFrame<'_>, row: &SidebarRow, events: &mut Vec<RowEvent>) {
    let SidebarRow::FolderHeader {
        group_name,
        count,
        is_collapsed,
        has_selected_child,
        unit_index,
        total_units,
        ..
    } = row
    else {
        return;
    };
    let RowFrame {
        rect, key, hovered, ..
    } = *frame;
    let fill = if frame.dragging {
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
    drag_handle(ui, key, rect, &mut left, hovered, frame.dragging, events);
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
    let chevron = Rect::from_center_size(pos2(right - 12.0, rect.center().y), vec2(24.0, 24.0));
    right -= 24.0 + 4.0;
    let chevron_glyph = if *is_collapsed {
        glyph::CHEVRON_RIGHT
    } else {
        glyph::CHEVRON_DOWN
    };
    if glyph_button(
        ui,
        ui.make_persistent_id(("sidebar_collapse", key)),
        chevron,
        chevron_glyph,
        18.0,
        ON_SURFACE_DIM,
    )
    .on_hover_text(if *is_collapsed {
        crate::i18n::tr(ui.ctx()).expand
    } else {
        crate::i18n::tr(ui.ctx()).collapse
    })
    .clicked()
    {
        events.push(RowEvent::ToggleCollapse);
    }

    if hovered && !frame.dragging {
        reorder_buttons(
            ui,
            key,
            rect,
            &mut right,
            *unit_index > 0,
            *unit_index + 1 < *total_units,
            events,
        );
        right -= 4.0;
    }

    // Count badge.
    let badge_text = ui.painter().layout_no_wrap(
        count.to_string(),
        FontId::proportional(12.0),
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

    if frame.clicked && !frame.dragging {
        events.push(RowEvent::ActivateFolder);
    }
}

fn entry_row(ui: &mut Ui, frame: &RowFrame<'_>, row: &SidebarRow, events: &mut Vec<RowEvent>) {
    let SidebarRow::Entry {
        entry,
        is_nested,
        sibling_index,
        total_siblings,
        unit_index,
        total_units,
        ..
    } = row
    else {
        return;
    };
    let RowFrame {
        rect, key, hovered, ..
    } = *frame;
    let is_selected = frame
        .state
        .selected_entry
        .as_ref()
        .is_some_and(|s| s.common().name == entry.common().name);
    let fill = if frame.dragging {
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
    drag_handle(ui, key, rect, &mut left, hovered, frame.dragging, events);
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
    if frame.state.steam_available {
        right -= 4.0;
        let registered = frame.state.is_steam_registered(entry);
        let steam = Rect::from_center_size(pos2(right - 12.0, rect.center().y), vec2(24.0, 24.0));
        right -= 24.0;
        let color = if registered {
            STEAM_REGISTERED
        } else {
            ON_SURFACE_DIM.gamma_multiply(0.4)
        };
        if glyph_button(
            ui,
            ui.make_persistent_id(("sidebar_steam", key)),
            steam,
            glyph::WIDGETS,
            16.0,
            color,
        )
        .on_hover_text(if registered {
            crate::i18n::tr(ui.ctx()).unregister_steam
        } else {
            crate::i18n::tr(ui.ctx()).register_steam
        })
        .clicked()
        {
            events.push(RowEvent::ToggleSteam(entry.clone()));
        }
    }
    if hovered && !frame.dragging {
        let (position, count) = if *is_nested {
            (*sibling_index, *total_siblings)
        } else {
            (*unit_index, *total_units)
        };
        reorder_buttons(
            ui,
            key,
            rect,
            &mut right,
            position > 0,
            position + 1 < count,
            events,
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
        GameEntry::Native(_) => crate::i18n::tr(ui.ctx()).native.to_string(),
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
        FontId::proportional(12.0),
        ON_SURFACE_DIM,
        width,
    );

    if frame.clicked && !frame.dragging {
        events.push(RowEvent::Select(entry.clone()));
    }
}
