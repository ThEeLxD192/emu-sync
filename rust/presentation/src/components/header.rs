//! Top application header bar, laid out like the Kotlin `AppHeader`.

use crate::{
    actions::UiAction,
    state::AppUiState,
    theme::{
        DIVIDER, ERROR, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, SUCCESS, SURFACE_VARIANT, bold, glyph,
    },
};
use egui::{
    Align, Color32, CursorIcon, FontId, Layout, Response, RichText, Sense, Stroke, StrokeKind, Ui,
    Vec2, vec2,
};
use emusync_application::UpdateState;

const LABEL_SIZE: f32 = 11.0;
const ICON_SIZE: f32 = 16.0;
const CHIP_PADDING: Vec2 = Vec2::new(10.0, 6.0);

/// Renders the header: branding on the left; Drive status, update chip, version and
/// the safe-close action on the right.
pub fn render_header(
    ui: &mut Ui,
    state: &AppUiState,
    on_action: &mut impl FnMut(UiAction),
    on_show_drive: &mut impl FnMut(),
    on_show_update: &mut impl FnMut(),
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(
            RichText::new("EmuSync")
                .font(bold(22.0))
                .color(PRIMARY)
                .extra_letter_spacing(0.5),
        );

        // Right-to-left: items are added from the window edge inwards.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if pill(
                ui,
                &Pill {
                    text: "Safe Close",
                    text_color: Color32::WHITE,
                    font: FontId::proportional(14.0),
                    fill: ERROR,
                    padding: vec2(16.0, 8.0),
                    min_height: 40.0,
                    corner_radius: 20,
                    ..Pill::default()
                },
            )
            .clicked()
            {
                on_action(UiAction::Exit);
            }

            ui.add_space(16.0);
            ui.label(
                RichText::new(format!("v{}", state.current_version))
                    .size(LABEL_SIZE)
                    .color(ON_SURFACE_DIM),
            );

            ui.add_space(12.0);
            render_update_slot(ui, &state.update_state, on_action, on_show_update);

            ui.add_space(12.0);
            render_drive_status(ui, state, on_action, on_show_drive);
        });
    });
}

/// Action for the "Connect Drive" button, matching Kotlin: connect directly with stored
/// client credentials, or `None` to open the Drive setup dialog when either is blank.
pub fn connect_drive_action(state: &AppUiState) -> Option<UiAction> {
    let credentials = state.config.as_ref()?.credentials.as_ref()?;
    if credentials.client_id.trim().is_empty() || credentials.client_secret.trim().is_empty() {
        return None;
    }
    Some(UiAction::ConnectDrive {
        client_id: credentials.client_id.clone(),
        client_secret: credentials.client_secret.clone(),
    })
}

fn render_drive_status(
    ui: &mut Ui,
    state: &AppUiState,
    on_action: &mut impl FnMut(UiAction),
    on_show_drive: &mut impl FnMut(),
) {
    if state.is_drive_connected() {
        let chip = Pill {
            icon: Some((glyph::CLOUD, SUCCESS)),
            text: "Drive Linked",
            fill: SURFACE_VARIANT,
            ..Pill::default()
        };
        if pill(ui, &chip).clicked() {
            on_show_drive();
        }
        return;
    }

    let button = Pill {
        icon: Some((glyph::CLOUD_SYNC, PRIMARY)),
        text: "Connect Drive",
        text_color: PRIMARY,
        fill: Color32::TRANSPARENT,
        stroke: Stroke::new(1.0, DIVIDER),
        padding: vec2(12.0, 6.0),
        corner_radius: 16,
        ..Pill::default()
    };
    if pill(ui, &button).clicked() {
        match connect_drive_action(state) {
            Some(action) => on_action(action),
            None => on_show_drive(),
        }
    }
}

fn render_update_slot(
    ui: &mut Ui,
    update_state: &UpdateState,
    on_action: &mut impl FnMut(UiAction),
    on_show_update: &mut impl FnMut(),
) {
    let chip = match update_state {
        UpdateState::Available(info) => Pill {
            icon: Some((glyph::UPDATE, Color32::WHITE)),
            text: &format!("Update v{}", info.version),
            text_color: Color32::WHITE,
            font: bold(LABEL_SIZE),
            fill: PRIMARY,
            ..Pill::default()
        }
        .show(ui),
        UpdateState::ReadyToRestart { .. } => Pill {
            icon: Some((glyph::RESTART, Color32::WHITE)),
            text: "Restart EmuSync",
            text_color: Color32::WHITE,
            font: bold(LABEL_SIZE),
            fill: SUCCESS,
            ..Pill::default()
        }
        .show(ui),
        // Rust-only extras: Kotlin shows nothing in these states.
        UpdateState::Downloading { progress, .. } => Pill {
            icon: Some((glyph::DOWNLOAD, PRIMARY)),
            text: &format!("Downloading {}%", (progress * 100.0) as u32),
            text_color: PRIMARY,
            fill: SURFACE_VARIANT,
            ..Pill::default()
        }
        .show(ui),
        UpdateState::Checking => {
            ui.label(
                RichText::new("Checking updates…")
                    .size(LABEL_SIZE)
                    .color(ON_SURFACE_DIM),
            );
            return;
        }
        UpdateState::Idle | UpdateState::Error(_) => {
            let check = Pill {
                text: "Check Updates",
                text_color: ON_SURFACE_DIM,
                fill: Color32::TRANSPARENT,
                stroke: Stroke::new(1.0, DIVIDER),
                ..Pill::default()
            };
            if pill(ui, &check).clicked() {
                on_action(UiAction::CheckForUpdates { manual: true });
            }
            return;
        }
    };
    if chip.clicked() {
        on_show_update();
    }
}

/// A rounded clickable chip with an optional leading Nerd Font icon.
struct Pill<'a> {
    icon: Option<(&'a str, Color32)>,
    text: &'a str,
    text_color: Color32,
    font: FontId,
    fill: Color32,
    stroke: Stroke,
    padding: Vec2,
    min_height: f32,
    corner_radius: u8,
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
        }
    }
}

impl Pill<'_> {
    fn show(&self, ui: &mut Ui) -> Response {
        pill(ui, self)
    }
}

fn pill(ui: &mut Ui, spec: &Pill<'_>) -> Response {
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
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect(
            rect,
            spec.corner_radius,
            spec.fill,
            spec.stroke,
            StrokeKind::Inside,
        );
        let overlay = if response.is_pointer_button_down_on() {
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

    response.on_hover_cursor(CursorIcon::PointingHand)
}
