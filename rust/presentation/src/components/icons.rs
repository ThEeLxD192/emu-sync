//! Icon helpers drawing Nerd Font glyphs from the bundled application font.

use egui::{Color32, CornerRadius, Pos2, Rect, Response, Sense, Ui};

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
