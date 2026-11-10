//! Rounded chip/button with an optional leading Nerd Font icon, shared by the header
//! and the category toolbar.

use crate::theme::{ON_SURFACE, SURFACE_VARIANT};
use egui::{Color32, CursorIcon, FontId, Response, Sense, Stroke, StrokeKind, Ui, Vec2, vec2};

pub(crate) const LABEL_SIZE: f32 = 11.0;
const ICON_SIZE: f32 = 16.0;
const CHIP_PADDING: Vec2 = Vec2::new(10.0, 6.0);

/// A rounded clickable chip with an optional leading Nerd Font icon.
pub(crate) struct Pill<'a> {
    pub(crate) icon: Option<(&'a str, Color32)>,
    pub(crate) text: &'a str,
    pub(crate) text_color: Color32,
    pub(crate) font: FontId,
    pub(crate) fill: Color32,
    pub(crate) stroke: Stroke,
    pub(crate) padding: Vec2,
    pub(crate) min_height: f32,
    pub(crate) corner_radius: u8,
    /// Clickable chips react to the pointer; status chips only display.
    pub(crate) interactive: bool,
}

impl Default for Pill<'_> {
    fn default() -> Self {
        Self {
            icon: None,
            text: "",
            text_color: ON_SURFACE,
            font: FontId::proportional(LABEL_SIZE),
            fill: SURFACE_VARIANT,
            stroke: Stroke::NONE,
            padding: CHIP_PADDING,
            min_height: 0.0,
            corner_radius: 8,
            interactive: true,
        }
    }
}

impl Pill<'_> {
    pub(crate) fn show(&self, ui: &mut Ui) -> Response {
        pill(ui, self)
    }
}

pub(crate) fn pill(ui: &mut Ui, spec: &Pill<'_>) -> Response {
    const ICON_GAP: f32 = 6.0;
    let painter = ui.painter();
    let icon = spec.icon.map(|(glyph, color)| {
        painter.layout_no_wrap(glyph.to_owned(), FontId::proportional(ICON_SIZE), color)
    });
    let text = painter.layout_no_wrap(spec.text.to_owned(), spec.font.clone(), spec.text_color);

    // Nerd Font icons paint wider than their advance, so size them by their painted bounds.
    let icon_box = icon.as_ref().map(|g| g.mesh_bounds.width().max(ICON_SIZE));
    let icon_width = icon_box.map_or(0.0, |w| w + ICON_GAP);
    let content_height = icon.as_ref().map_or(0.0, |g| g.size().y).max(text.size().y);
    let size = vec2(
        icon_width + text.size().x + 2.0 * spec.padding.x,
        (content_height + 2.0 * spec.padding.y).max(spec.min_height),
    );
    let sense = if spec.interactive {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect(
            rect,
            spec.corner_radius,
            spec.fill,
            spec.stroke,
            StrokeKind::Inside,
        );
        let overlay = if !spec.interactive {
            0
        } else if response.is_pointer_button_down_on() {
            24
        } else if response.hovered() {
            12
        } else {
            0
        };
        if overlay > 0 {
            painter.rect_filled(rect, spec.corner_radius, Color32::from_white_alpha(overlay));
        }

        let mut x = rect.left() + spec.padding.x;
        if let (Some(icon), Some(box_width)) = (icon, icon_box) {
            // Center the painted glyph (not its advance box) inside the icon slot.
            let bounds = icon.mesh_bounds;
            let origin = egui::pos2(
                x + box_width / 2.0 - bounds.center().x,
                rect.center().y - bounds.center().y,
            );
            painter.galley(origin, icon, spec.text_color);
            x += box_width + ICON_GAP;
        }
        let y = rect.center().y - text.size().y / 2.0;
        painter.galley(egui::pos2(x, y), text, spec.text_color);
    }

    if spec.interactive {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}
