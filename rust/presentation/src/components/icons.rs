//! Vector-drawn crisp icons immune to missing system font glyphs.

use egui::{Color32, CornerRadius, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2};

use crate::theme::{PRIMARY, PRIMARY_CONTAINER};

/// Paints a right-pointing play triangle centered in `rect`.
pub fn paint_play_triangle(ui: &Ui, rect: Rect, color: Color32, scale: f32) {
    let painter = ui.painter();
    let center = rect.center();
    let size = (rect.height().min(rect.width()) * 0.45 * scale).max(4.0);
    let half_w = size * 0.866;
    let points = vec![
        Pos2::new(center.x - half_w * 0.5, center.y - size * 0.6),
        Pos2::new(center.x + half_w * 0.6, center.y),
        Pos2::new(center.x - half_w * 0.5, center.y + size * 0.6),
    ];
    painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
}

/// Paints a clean chevron pointing up centered in `rect`.
pub fn paint_arrow_up(ui: &Ui, rect: Rect, color: Color32) {
    let painter = ui.painter();
    let center = rect.center();
    let w = 4.0;
    let h = 3.0;
    let points = vec![
        Pos2::new(center.x - w, center.y + h),
        Pos2::new(center.x, center.y - h),
        Pos2::new(center.x + w, center.y + h),
    ];
    painter.add(Shape::line(points, Stroke::new(1.8, color)));
}

/// Paints a clean chevron pointing down centered in `rect`.
pub fn paint_arrow_down(ui: &Ui, rect: Rect, color: Color32) {
    let painter = ui.painter();
    let center = rect.center();
    let w = 4.0;
    let h = 3.0;
    let points = vec![
        Pos2::new(center.x - w, center.y - h),
        Pos2::new(center.x, center.y + h),
        Pos2::new(center.x + w, center.y - h),
    ];
    painter.add(Shape::line(points, Stroke::new(1.8, color)));
}

/// Interactive button for moving items up.
pub fn button_move_up(ui: &mut Ui, enabled: bool) -> Response {
    let size = Vec2::new(20.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        if response.hovered() && enabled {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), visuals.bg_fill);
        }
        let color = if enabled {
            visuals.fg_stroke.color
        } else {
            ui.visuals()
                .widgets
                .noninteractive
                .fg_stroke
                .color
                .gamma_multiply(0.4)
        };
        paint_arrow_up(ui, rect, color);
    }
    response
}

/// Interactive button for moving items down.
pub fn button_move_down(ui: &mut Ui, enabled: bool) -> Response {
    let size = Vec2::new(20.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        if response.hovered() && enabled {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), visuals.bg_fill);
        }
        let color = if enabled {
            visuals.fg_stroke.color
        } else {
            ui.visuals()
                .widgets
                .noninteractive
                .fg_stroke
                .color
                .gamma_multiply(0.4)
        };
        paint_arrow_down(ui, rect, color);
    }
    response
}

/// Paints a stylized edit pencil centered in `rect`.
pub fn paint_pencil_icon(ui: &Ui, rect: Rect, color: Color32) {
    let painter = ui.painter();
    let center = rect.center();
    let s = 4.0;
    let p1 = Pos2::new(center.x - s, center.y + s);
    let p2 = Pos2::new(center.x + s * 0.7, center.y - s * 0.7);
    painter.add(Shape::line(vec![p1, p2], Stroke::new(1.8, color)));
    // tip
    painter.add(Shape::line(
        vec![
            Pos2::new(center.x - s - 1.0, center.y + s),
            Pos2::new(center.x - s, center.y + s + 1.0),
        ],
        Stroke::new(1.5, color),
    ));
}

/// Renders the sleek game cover fallback placeholder matching Compose Desktop.
pub fn render_cover_placeholder(ui: &mut Ui, available_rect: Rect) {
    let painter = ui.painter();
    let center = available_rect.center();
    let box_size = Vec2::new(56.0, 56.0);
    let box_rect = Rect::from_center_size(center, box_size);

    painter.rect_filled(box_rect, CornerRadius::same(14), PRIMARY_CONTAINER);
    paint_play_triangle(ui, box_rect, PRIMARY, 1.2);
}

/// Paints a Nerd Font glyph centered on its painted bounds (glyph advances are unreliable).
pub fn paint_glyph(ui: &Ui, center: Pos2, glyph: &str, size: f32, color: Color32) {
    let galley =
        ui.painter()
            .layout_no_wrap(glyph.to_owned(), egui::FontId::proportional(size), color);
    let origin = center - galley.mesh_bounds.center().to_vec2();
    ui.painter().galley(origin, galley, color);
}

/// A round icon button with a hover highlight, drawn inside `rect`.
pub fn glyph_button(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    glyph: &str,
    size: f32,
    color: Color32,
) -> Response {
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same((rect.height() / 2.0) as u8),
            Color32::from_white_alpha(14),
        );
    }
    paint_glyph(ui, rect.center(), glyph, size, color);
    response
}
