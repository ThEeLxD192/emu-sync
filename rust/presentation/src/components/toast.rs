//! Toast notification overlay for transient messages.

use crate::theme::{DIVIDER, ON_BACKGROUND, ON_SURFACE, SURFACE};
use egui::{
    Align, Align2, Button, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Ui, Vec2, Window,
};

/// Renders a floating notification toast at the bottom center of the window.
pub fn render_toast(ui: &mut Ui, message: &str, on_dismiss: &mut impl FnMut()) {
    Window::new("Notification")
        .title_bar(false)
        .resizable(false)
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
                ui.label(RichText::new(message).size(13.0).color(ON_BACKGROUND));
                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            Button::new(RichText::new("x").size(11.0).color(ON_SURFACE))
                                .min_size(Vec2::new(20.0, 20.0)),
                        )
                        .clicked()
                    {
                        on_dismiss();
                    }
                });
            });
        });
}
