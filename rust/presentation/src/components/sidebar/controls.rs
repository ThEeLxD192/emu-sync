//! Drag handle, reorder buttons and truncated text shared by sidebar rows.

use super::RowEvent;
use crate::{
    components::icons::{glyph_button, paint_glyph},
    theme::{ON_SURFACE_DIM, PRIMARY, glyph},
};
use egui::{Color32, CursorIcon, FontId, Pos2, Rect, Sense, Ui, pos2, text::LayoutJob, vec2};

pub(super) fn drag_handle(
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

pub(super) fn reorder_buttons(
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
pub(super) fn paint_truncated(
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
