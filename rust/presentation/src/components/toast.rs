//! Toast notification overlay for transient messages.

use crate::theme::{DIVIDER, ON_BACKGROUND, ON_SURFACE, SURFACE};
use egui::{
    Align2, Button, CornerRadius, Frame, Label, Margin, RichText, Stroke, Ui, Vec2, Window,
};

/// Widest the message may grow before it wraps onto another line.
const MESSAGE_MAX_WIDTH: f32 = 420.0;
/// Minimum side of the dismiss button; also the row height the message is
/// centered against, so a single-line message lines up with the button.
const DISMISS_SIZE: f32 = 28.0;

/// Renders a floating notification toast at the bottom center of the window.
///
/// The dismiss button sits right after the message instead of being pinned
/// to the window edge: the window sizes itself from its content, so a
/// right-aligned button would land on top of the text.
pub fn render_toast(ui: &mut Ui, message: &str, on_dismiss: &mut impl FnMut()) {
    Window::new("Notification")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -30.0))
        .frame(
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, DIVIDER))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(16, 10)),
        )
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(DISMISS_SIZE);
                ui.scope(|ui| {
                    ui.set_max_width(MESSAGE_MAX_WIDTH);
                    ui.add(
                        Label::new(RichText::new(message).size(13.0).color(ON_BACKGROUND)).wrap(),
                    );
                });
                ui.add_space(12.0);
                if ui
                    .add(
                        Button::new(RichText::new("x").size(12.0).color(ON_SURFACE))
                            .min_size(Vec2::splat(DISMISS_SIZE)),
                    )
                    .clicked()
                {
                    on_dismiss();
                }
            });
        });
}
