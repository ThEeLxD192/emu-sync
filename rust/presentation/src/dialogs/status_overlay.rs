//! Status overlays for sync, gameplay, conflicts and errors, laid out like Kotlin's
//! `StatusOverlay`.
//!
//! Only the error notice closes on Escape or a click outside (Kotlin's
//! `Dialog(onDismissRequest = onDismiss)`); every other overlay mirrors a running
//! operation and stays until its buttons or the coordinator change the status.

use crate::{
    components::{
        icons::{paint_glyph, paint_glyph_rotated},
        pill::Pill,
    },
    dialogs::form_fields::primary_button,
    model::AppStatus,
    theme::{
        DIVIDER, ERROR, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SECONDARY, SUCCESS,
        SURFACE, WARNING, bold, glyph,
    },
};
use egui::{
    Align, Color32, FontId, Frame, Id, Label, Layout, Margin, Modal, Rect, RichText, ScrollArea,
    Sense, Stroke, Ui, vec2,
};
use emusync_domain::sync::Decision;

/// Callbacks the status overlay reports to the application.
pub struct StatusOverlayEvents<'a> {
    pub on_resolve_conflict: &'a mut dyn FnMut(Decision),
    pub on_retry_post_sync: &'a mut dyn FnMut(),
    pub on_dismiss: &'a mut dyn FnMut(),
}

/// Renders the overlay for `status`. `conflict_resolved` shows the conflict as syncing
/// right after a choice, as Kotlin does, so the other option cannot be picked.
pub fn render_status_overlay(
    ctx: &egui::Context,
    status: &AppStatus,
    conflict_resolved: bool,
    events: StatusOverlayEvents<'_>,
) {
    match status {
        AppStatus::Idle => {}
        AppStatus::Syncing { message } => {
            status_dialog(ctx, glyph::CLOUD_SYNC, message, None, true, PRIMARY)
        }
        AppStatus::Playing => status_dialog(
            ctx,
            glyph::CONTROLLER,
            "Playing...",
            Some("The game is running. This window will update when you close it."),
            false,
            SUCCESS,
        ),
        AppStatus::Loading => status_dialog(
            ctx,
            glyph::HOURGLASS,
            "Loading...",
            None,
            true,
            ON_SURFACE_DIM,
        ),
        AppStatus::Conflict { .. } if conflict_resolved => {
            status_dialog(ctx, glyph::CLOUD_SYNC, "Syncing...", None, true, PRIMARY)
        }
        AppStatus::Conflict {
            local_date,
            cloud_date,
            game_name,
        } => conflict_dialog(
            ctx,
            game_name,
            local_date,
            cloud_date,
            events.on_resolve_conflict,
        ),
        AppStatus::PostSyncFailed {
            game_name,
            reason,
            completed_save_count,
            pending_save_count,
        } => post_sync_failed_dialog(
            ctx,
            &post_sync_failed_message(
                game_name,
                reason,
                *completed_save_count,
                *pending_save_count,
            ),
            events.on_retry_post_sync,
            events.on_dismiss,
        ),
        AppStatus::Error { message } => error_dialog(ctx, message, events.on_dismiss),
    }
}

/// Body of the post-sync failure notice (Kotlin's `PostSyncFailedDialog`).
pub fn post_sync_failed_message(
    game_name: &str,
    reason: &str,
    completed_save_count: usize,
    pending_save_count: usize,
) -> String {
    let progress = if completed_save_count == 0 {
        "Google Drive was not updated.".to_string()
    } else {
        format!(
            "{completed_save_count} save(s) synced successfully; {pending_save_count} save(s) still need to be uploaded."
        )
    };
    format!("{game_name} closed normally. {progress}\n\n{reason}")
}

/// Stable id of a conflict option card ("Local Save" / "Cloud Save").
pub fn conflict_card_id(title: &str) -> Id {
    Id::unique(("conflict_card", title))
}

fn overlay(id: &str, border: Color32) -> Modal {
    Modal::new(Id::unique(id))
        .backdrop_color(Color32::from_black_alpha(160))
        .frame(
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, border))
                .corner_radius(20)
                .inner_margin(Margin::same(28)),
        )
}

fn status_dialog(
    ctx: &egui::Context,
    icon: &str,
    title: &str,
    subtitle: Option<&str>,
    spinning: bool,
    color: Color32,
) {
    overlay("status_dialog", DIVIDER).show(ctx, |ui| {
        ui.set_min_width(280.0);
        ui.set_max_width(380.0);
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 32.0, color.gamma_multiply(0.15));
            if spinning {
                // One turn every 1.2 s, as Kotlin's infinite rotation.
                let time = ui.input(|i| i.time) as f32;
                let angle = time / 1.2 * std::f32::consts::TAU;
                paint_glyph_rotated(ui, rect.center(), icon, 32.0, color, angle);
                ui.ctx().request_repaint();
            } else {
                paint_glyph(ui, rect.center(), icon, 32.0, color);
            }
            ui.add_space(20.0);
            ui.label(RichText::new(title).font(bold(15.0)).color(ON_BACKGROUND));
            if let Some(subtitle) = subtitle {
                ui.add_space(8.0);
                ui.label(RichText::new(subtitle).size(12.0).color(ON_SURFACE_DIM));
            }
        });
    });
}

fn alert_icon(ui: &mut Ui, icon: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
    paint_glyph(ui, rect.center(), icon, 48.0, color);
    ui.add_space(16.0);
}

fn conflict_dialog(
    ctx: &egui::Context,
    game_name: &str,
    local_date: &str,
    cloud_date: &str,
    on_resolve: &mut dyn FnMut(Decision),
) {
    overlay("conflict_dialog", WARNING.gamma_multiply(0.5)).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.vertical_centered(|ui| {
            alert_icon(ui, glyph::ALERT, WARNING);
            ui.label(RichText::new("Save Conflict").font(bold(20.0)).color(WARNING));
            ui.add_space(4.0);
            ui.label(RichText::new(game_name).size(13.0).color(ON_SURFACE_DIM));
            ui.add_space(12.0);
        });
        ui.label(
            RichText::new(
                "Both copies changed, or their shared history could not be verified. Choose which progress to keep. Restoring from the cloud preserves a local recovery copy.",
            )
            .size(12.0)
            .color(ON_SURFACE_DIM),
        );
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let width = (ui.available_width() - 12.0) / 2.0;
            if conflict_card(ui, width, "Local Save", local_date, glyph::COMPUTER, PRIMARY) {
                on_resolve(Decision::UploadLocal);
            }
            if conflict_card(ui, width, "Cloud Save", cloud_date, glyph::CLOUD, SECONDARY) {
                on_resolve(Decision::DownloadCloud);
            }
        });
    });
}

fn conflict_card(
    ui: &mut Ui,
    width: f32,
    title: &str,
    date: &str,
    icon: &str,
    color: Color32,
) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 112.0), Sense::hover());
    let response = ui
        .interact(rect, conflict_card_id(title), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let fill = if response.hovered() {
        color.gamma_multiply(0.16)
    } else {
        color.gamma_multiply(0.08)
    };
    ui.painter().rect(
        rect,
        12,
        fill,
        Stroke::new(1.0, color.gamma_multiply(0.4)),
        egui::StrokeKind::Inside,
    );
    let top = rect.top() + 12.0;
    paint_glyph(
        ui,
        egui::pos2(rect.center().x, top + 14.0),
        icon,
        28.0,
        color,
    );
    let text_area = Rect::from_min_max(egui::pos2(rect.left() + 8.0, top + 36.0), rect.max);
    let mut text_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_area)
            .layout(Layout::top_down(Align::Center)),
    );
    // Not selectable: selectable labels sense clicks and would swallow the card's.
    text_ui.add(Label::new(RichText::new(title).font(bold(13.0)).color(color)).selectable(false));
    text_ui.add_space(4.0);
    text_ui.add(Label::new(RichText::new(date).size(11.0).color(ON_SURFACE_DIM)).selectable(false));
    response.clicked()
}

fn post_sync_failed_dialog(
    ctx: &egui::Context,
    message: &str,
    on_retry: &mut dyn FnMut(),
    on_dismiss: &mut dyn FnMut(),
) {
    overlay("post_sync_failed_dialog", ERROR.gamma_multiply(0.5)).show(ctx, |ui| {
        ui.set_min_width(320.0);
        ui.set_max_width(460.0);
        ui.vertical_centered(|ui| {
            alert_icon(ui, glyph::ALERT, ERROR);
            ui.label(
                RichText::new("Save kept locally")
                    .font(bold(15.0))
                    .color(ERROR),
            );
            ui.add_space(8.0);
            ui.label(RichText::new(message).size(13.0).color(ON_SURFACE));
            ui.add_space(20.0);
        });
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 40.0),
            Layout::right_to_left(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if primary_button(ui, "Retry upload").clicked() {
                    on_retry();
                }
                let keep_local = Pill {
                    text: "Keep local",
                    text_color: PRIMARY,
                    font: FontId::proportional(13.0),
                    fill: Color32::TRANSPARENT,
                    stroke: Stroke::new(1.0, DIVIDER),
                    padding: vec2(20.0, 0.0),
                    min_height: 40.0,
                    corner_radius: 20,
                    ..Pill::default()
                };
                if keep_local.show(ui).clicked() {
                    on_dismiss();
                }
            },
        );
    });
}

fn error_dialog(ctx: &egui::Context, message: &str, on_dismiss: &mut dyn FnMut()) {
    let modal = overlay("error_dialog", ERROR.gamma_multiply(0.5)).show(ctx, |ui| {
        ui.set_min_width(300.0);
        ui.set_max_width(420.0);
        ui.vertical_centered(|ui| {
            alert_icon(ui, glyph::ERROR_OUTLINE, ERROR);
            ui.label(RichText::new("Sync Notice").font(bold(15.0)).color(ERROR));
            ui.add_space(8.0);
        });
        ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            ui.label(RichText::new(message).size(13.0).color(ON_SURFACE));
        });
        ui.add_space(20.0);
        ui.vertical_centered(|ui| primary_button(ui, "Close").clicked())
            .inner
    });
    if modal.inner || modal.should_close() {
        on_dismiss();
    }
}
