//! Confirmation before deleting a category (Rust-only: Kotlin deletes on the first click,
//! which is easy to trigger by accident on a touch screen).

use crate::{
    components::pill::Pill,
    theme::{DIVIDER, ERROR, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, SURFACE, bold},
};
use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Modal, RichText, Stroke, vec2,
};

/// Outcome of one frame of the delete confirmation dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmDeleteOutcome {
    Pending,
    Confirmed,
    Cancelled,
}

/// Renders the delete confirmation; Escape or a click outside cancels.
pub fn render_confirm_delete_dialog(
    ctx: &egui::Context,
    entry_name: &str,
    steam_registered: bool,
) -> ConfirmDeleteOutcome {
    let mut outcome = ConfirmDeleteOutcome::Pending;
    let modal = Modal::new(Id::unique("confirm_delete_modal"))
        .backdrop_color(Color32::from_black_alpha(160))
        .frame(
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, DIVIDER))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(24)),
        )
        .show(ctx, |ui| {
            let t = crate::i18n::tr(ui.ctx());
            ui.set_width(380.0);
            ui.label(
                RichText::new(crate::i18n::fill(
                    t.delete_entry_title,
                    &[("name", &entry_name)],
                ))
                .font(bold(15.0))
                .color(ON_BACKGROUND),
            );
            ui.add_space(8.0);
            let mut detail = String::from(t.delete_entry_detail);
            if steam_registered {
                detail.push_str(t.delete_entry_steam);
            }
            ui.label(RichText::new(detail).size(12.0).color(ON_SURFACE));
            ui.add_space(4.0);
            ui.label(
                RichText::new(t.delete_entry_kept)
                    .size(12.0)
                    .color(ON_SURFACE_DIM),
            );
            ui.add_space(20.0);

            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 36.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    let delete = Pill {
                        text: t.delete,
                        text_color: Color32::WHITE,
                        font: bold(13.0),
                        fill: ERROR,
                        padding: vec2(16.0, 8.0),
                        min_height: 36.0,
                        corner_radius: 18,
                        ..Pill::default()
                    };
                    if delete.show(ui).clicked() {
                        outcome = ConfirmDeleteOutcome::Confirmed;
                    }
                    ui.add_space(8.0);
                    let cancel = Pill {
                        text: t.cancel,
                        text_color: ON_SURFACE,
                        font: FontId::proportional(13.0),
                        fill: Color32::TRANSPARENT,
                        stroke: Stroke::new(1.0, DIVIDER),
                        padding: vec2(16.0, 8.0),
                        min_height: 36.0,
                        corner_radius: 18,
                        ..Pill::default()
                    };
                    if cancel.show(ui).clicked() {
                        outcome = ConfirmDeleteOutcome::Cancelled;
                    }
                },
            );
        });
    if outcome == ConfirmDeleteOutcome::Pending && modal.should_close() {
        outcome = ConfirmDeleteOutcome::Cancelled;
    }
    outcome
}
