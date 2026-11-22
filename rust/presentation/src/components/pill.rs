//! Rounded chip/button with an optional leading Nerd Font icon, shared by the header
//! and the category toolbar.

use crate::theme::{ON_SURFACE, SURFACE_VARIANT};
use egui::{
    Color32, CursorIcon, FontId, Galley, Response, RichText, Sense, Stroke, StrokeKind, TextStyle,
    TextWrapMode, Ui, Vec2, WidgetText, vec2,
};
use std::sync::Arc;

pub(crate) const LABEL_SIZE: f32 = 11.0;
const ICON_SIZE: f32 = 16.0;
const CHIP_PADDING: Vec2 = Vec2::new(10.0, 6.0);
/// Space between the icon and the text.
const ICON_GAP: f32 = 6.0;

/// A rounded clickable chip with an optional leading Nerd Font icon.
#[derive(Clone)]
pub(crate) struct Pill<'a> {
    pub(crate) icon: Option<(&'a str, Color32)>,
    pub(crate) text: &'a str,
    pub(crate) text_color: Color32,
    pub(crate) font: FontId,
    pub(crate) fill: Color32,
    pub(crate) stroke: Stroke,
    pub(crate) padding: Vec2,
    pub(crate) min_height: f32,
    /// Content is centered when the pill is wider than it.
    pub(crate) min_width: f32,
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
            min_width: 0.0,
            corner_radius: 8,
            interactive: true,
        }
    }
}

impl<'a> Pill<'a> {
    pub(crate) fn show(&self, ui: &mut Ui) -> Response {
        pill(ui, self)
    }

    /// The size the pill takes, without laying it out.
    pub(crate) fn size(&self, ui: &Ui) -> Vec2 {
        PillLayout::new(ui, self).size
    }

    /// The same pill with only its icon, or `fallback_icon` when it has none.
    fn icon_only(&self, fallback_icon: (&'a str, Color32)) -> Self {
        Self {
            icon: Some(self.icon.unwrap_or(fallback_icon)),
            text: "",
            ..self.clone()
        }
    }

    /// Shows the pill in `form`; an icon-only pill keeps its text as its tooltip.
    /// `fallback_icon` stands in for a pill that has no icon of its own.
    pub(crate) fn show_as(
        &self,
        ui: &mut Ui,
        form: PillForm,
        fallback_icon: (&'a str, Color32),
    ) -> Response {
        match form {
            PillForm::Whole => self.show(ui),
            PillForm::IconOnly => self
                .icon_only(fallback_icon)
                .show(ui)
                .on_hover_text(self.text),
        }
    }

    /// The width `show_as` takes for the same `form`.
    pub(crate) fn width_as(
        &self,
        ui: &Ui,
        form: PillForm,
        fallback_icon: (&'a str, Color32),
    ) -> f32 {
        match form {
            PillForm::Whole => self.size(ui).x,
            PillForm::IconOnly => self.icon_only(fallback_icon).size(ui).x,
        }
    }
}

/// How a pill shows when its row is short of room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PillForm {
    /// Icon and text.
    Whole,
    /// Only the icon; the text becomes the tooltip.
    IconOnly,
}

/// The icon and text of a pill and where they fit: what `pill` paints and what
/// `Pill::size` reports, from one computation.
struct PillLayout {
    icon: Option<Arc<Galley>>,
    icon_box: Option<f32>,
    text: Arc<Galley>,
    content_width: f32,
    size: Vec2,
}

impl PillLayout {
    fn new(ui: &Ui, spec: &Pill<'_>) -> Self {
        let painter = ui.painter();
        let icon = spec.icon.map(|(glyph, color)| {
            painter.layout_no_wrap(glyph.to_owned(), FontId::proportional(ICON_SIZE), color)
        });
        let text = painter.layout_no_wrap(spec.text.to_owned(), spec.font.clone(), spec.text_color);

        // Nerd Font icons paint wider than their advance, so size them by their painted bounds.
        let icon_box = icon.as_ref().map(|g| g.mesh_bounds.width().max(ICON_SIZE));
        // An icon-only pill has no text to keep apart from its icon.
        let icon_gap = if spec.text.is_empty() { 0.0 } else { ICON_GAP };
        let icon_width = icon_box.map_or(0.0, |w| w + icon_gap);
        let content_height = icon.as_ref().map_or(0.0, |g| g.size().y).max(text.size().y);
        let content_width = icon_width + text.size().x;
        let size = vec2(
            (content_width + 2.0 * spec.padding.x).max(spec.min_width),
            (content_height + 2.0 * spec.padding.y).max(spec.min_height),
        );
        Self {
            icon,
            icon_box,
            text,
            content_width,
            size,
        }
    }
}

pub(crate) fn pill(ui: &mut Ui, spec: &Pill<'_>) -> Response {
    let PillLayout {
        icon,
        icon_box,
        text,
        content_width,
        size,
    } = PillLayout::new(ui, spec);
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

        let mut x = rect.center().x - content_width / 2.0;
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

/// Width of a one-line label with `text`, as `Ui::label` lays it out.
pub(crate) fn text_width(ui: &Ui, text: RichText) -> f32 {
    WidgetText::from(text)
        .into_galley(
            ui,
            Some(TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Body,
        )
        .size()
        .x
}
