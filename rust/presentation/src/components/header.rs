//! Top application header bar, laid out like the Kotlin `AppHeader`.

use crate::{
    actions::UiAction,
    components::pill::{LABEL_SIZE, Pill, PillForm, pill, text_width},
    i18n::{self, Language, Texts, fill, tr},
    state::AppUiState,
    theme::{DIVIDER, ERROR, ON_SURFACE_DIM, PRIMARY, SUCCESS, SURFACE_VARIANT, bold, glyph},
};
use egui::{Align, Color32, FontId, Layout, Popup, RichText, Stroke, Ui, vec2};
use emusync_application::UpdateState;

/// Space kept between the brand and the items on the right.
const BRAND_GAP: f32 = 16.0;
/// Space between the items on the right, and before the version label.
const ITEM_GAP: f32 = 12.0;
const VERSION_GAP: f32 = 16.0;

/// Renders the header: branding on the left; Drive status, update chip, version and
/// the safe-close action on the right. When they do not fit beside the brand, the
/// buttons show only their icon (their text becomes the tooltip) and the version moves
/// to the brand's tooltip.
pub fn render_header(
    ui: &mut Ui,
    state: &AppUiState,
    on_action: &mut impl FnMut(UiAction),
    on_show_drive: &mut impl FnMut(),
    on_show_update: &mut impl FnMut(),
) {
    let t = tr(ui.ctx());
    let current_language = i18n::language(ui.ctx());
    let language_code = current_language.code().to_uppercase();
    let version = format!("v{}", state.current_version);
    let update_text = update_text(t, &state.update_state);
    let items = HeaderItems {
        close: safe_close_pill(t),
        language: language_pill(&language_code),
        version: &version,
        update: update_item(t, &state.update_state, &update_text),
        drive: drive_pill(t, state.is_drive_connected()),
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let brand = ui.label(
            RichText::new("EmuSync")
                .font(bold(22.0))
                .color(PRIMARY)
                .extra_letter_spacing(0.5),
        );
        let form = if items.width(ui, PillForm::Whole) + BRAND_GAP > ui.available_width() {
            PillForm::IconOnly
        } else {
            PillForm::Whole
        };
        if form == PillForm::IconOnly {
            brand.on_hover_text(&version);
        }

        // Right-to-left: items are added from the window edge inwards.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if items.close.show_as(ui, form, CLOSE_ICON).clicked() {
                on_action(UiAction::Exit);
            }

            ui.add_space(ITEM_GAP);
            render_language_selector(ui, &items.language, current_language, on_action);

            if form == PillForm::Whole {
                ui.add_space(VERSION_GAP);
                ui.label(version_text(&version));
            }

            ui.add_space(ITEM_GAP);
            items.update.show(ui, form, on_action, on_show_update);

            ui.add_space(ITEM_GAP);
            if items.drive.show_as(ui, form, DRIVE_ICON).clicked() {
                if state.is_drive_connected() {
                    on_show_drive();
                } else {
                    match connect_drive_action(state) {
                        Some(action) => on_action(action),
                        None => on_show_drive(),
                    }
                }
            }
        });
    });
}

/// Icons for the buttons that have none, shown when the header is short of room.
const CLOSE_ICON: (&str, Color32) = (glyph::POWER, Color32::WHITE);
const CHECK_UPDATES_ICON: (&str, Color32) = (glyph::UPDATE, ON_SURFACE_DIM);
/// The Drive buttons always have their own icon; this only completes the call.
const DRIVE_ICON: (&str, Color32) = (glyph::CLOUD, ON_SURFACE_DIM);

/// The items on the right of the header, built once to measure and to show them.
struct HeaderItems<'a> {
    close: Pill<'a>,
    language: Pill<'a>,
    version: &'a str,
    update: UpdateItem<'a>,
    drive: Pill<'a>,
}

impl HeaderItems<'_> {
    /// Width of the items on the right with their buttons in `form`.
    fn width(&self, ui: &Ui, form: PillForm) -> f32 {
        let version = match form {
            PillForm::Whole => VERSION_GAP + text_width(ui, version_text(self.version)),
            PillForm::IconOnly => 0.0,
        };
        self.close.width_as(ui, form, CLOSE_ICON)
            + ITEM_GAP
            + self.language.size(ui).x
            + version
            + ITEM_GAP
            + self.update.width(ui, form)
            + ITEM_GAP
            + self.drive.width_as(ui, form, DRIVE_ICON)
    }
}

fn version_text(version: &str) -> RichText {
    RichText::new(version)
        .size(LABEL_SIZE)
        .color(ON_SURFACE_DIM)
}

fn safe_close_pill(t: &'static Texts) -> Pill<'static> {
    Pill {
        text: t.safe_close,
        text_color: Color32::WHITE,
        font: FontId::proportional(14.0),
        fill: ERROR,
        padding: vec2(16.0, 8.0),
        min_height: 40.0,
        corner_radius: 20,
        ..Pill::default()
    }
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

fn drive_pill(t: &'static Texts, is_connected: bool) -> Pill<'static> {
    if is_connected {
        return Pill {
            icon: Some((glyph::CLOUD, SUCCESS)),
            text: t.drive_linked,
            fill: SURFACE_VARIANT,
            ..Pill::default()
        };
    }
    Pill {
        icon: Some((glyph::CLOUD_SYNC, PRIMARY)),
        text: t.connect_drive,
        text_color: PRIMARY,
        fill: Color32::TRANSPARENT,
        stroke: Stroke::new(1.0, DIVIDER),
        padding: vec2(12.0, 6.0),
        corner_radius: 16,
        ..Pill::default()
    }
}

/// The text of the update chip when it shows a value; empty otherwise.
fn update_text(t: &Texts, update_state: &UpdateState) -> String {
    match update_state {
        UpdateState::Available(info) => fill(t.update_to, &[("version", &info.version)]),
        UpdateState::Downloading { progress, .. } => fill(
            t.downloading_percent,
            &[(
                "percent",
                &crate::dialogs::update_dialog::progress_percent(*progress),
            )],
        ),
        _ => String::new(),
    }
}

/// What the update slot shows, and what a click on it does.
enum UpdateItem<'a> {
    /// A chip that opens the update dialog.
    OpenDialog(Pill<'a>),
    /// A button that checks for updates now.
    CheckNow(Pill<'a>),
    /// A check is running.
    Checking(&'a str),
}

fn update_item<'a>(t: &'static Texts, update_state: &UpdateState, text: &'a str) -> UpdateItem<'a> {
    match update_state {
        UpdateState::Available(_) => UpdateItem::OpenDialog(Pill {
            icon: Some((glyph::UPDATE, Color32::WHITE)),
            text,
            text_color: Color32::WHITE,
            font: bold(LABEL_SIZE),
            fill: PRIMARY,
            ..Pill::default()
        }),
        UpdateState::ReadyToRestart { .. } => UpdateItem::OpenDialog(Pill {
            icon: Some((glyph::RESTART, Color32::WHITE)),
            text: t.restart_app,
            text_color: Color32::WHITE,
            font: bold(LABEL_SIZE),
            fill: SUCCESS,
            ..Pill::default()
        }),
        // Rust-only extras: Kotlin shows nothing in these states.
        UpdateState::Downloading { .. } => UpdateItem::OpenDialog(Pill {
            icon: Some((glyph::DOWNLOAD, PRIMARY)),
            text,
            text_color: PRIMARY,
            fill: SURFACE_VARIANT,
            ..Pill::default()
        }),
        UpdateState::Checking => UpdateItem::Checking(t.checking_updates),
        UpdateState::Idle
        | UpdateState::CheckFailed(_)
        | UpdateState::DownloadFailed(_)
        | UpdateState::RestartFailed { .. } => UpdateItem::CheckNow(Pill {
            text: t.check_updates,
            text_color: ON_SURFACE_DIM,
            fill: Color32::TRANSPARENT,
            stroke: Stroke::new(1.0, DIVIDER),
            ..Pill::default()
        }),
    }
}

/// The icon-only form of the "checking for updates" label, with the text as its tooltip.
fn checking_icon(text: &str) -> Pill<'_> {
    Pill {
        icon: Some((glyph::HOURGLASS, ON_SURFACE_DIM)),
        text,
        fill: Color32::TRANSPARENT,
        interactive: false,
        ..Pill::default()
    }
}

impl UpdateItem<'_> {
    fn width(&self, ui: &Ui, form: PillForm) -> f32 {
        match (self, form) {
            (Self::OpenDialog(chip) | Self::CheckNow(chip), _) => {
                chip.width_as(ui, form, CHECK_UPDATES_ICON)
            }
            (Self::Checking(text), PillForm::IconOnly) => {
                checking_icon(text).width_as(ui, form, CHECK_UPDATES_ICON)
            }
            (Self::Checking(text), PillForm::Whole) => text_width(ui, checking_text(text)),
        }
    }

    fn show(
        &self,
        ui: &mut Ui,
        form: PillForm,
        on_action: &mut impl FnMut(UiAction),
        on_show_update: &mut impl FnMut(),
    ) {
        match (self, form) {
            (Self::OpenDialog(chip), _) => {
                if chip.show_as(ui, form, CHECK_UPDATES_ICON).clicked() {
                    on_show_update();
                }
            }
            (Self::CheckNow(button), _) => {
                if button.show_as(ui, form, CHECK_UPDATES_ICON).clicked() {
                    on_action(UiAction::CheckForUpdates { manual: true });
                }
            }
            (Self::Checking(text), PillForm::IconOnly) => {
                checking_icon(text).show_as(ui, form, CHECK_UPDATES_ICON);
            }
            (Self::Checking(text), PillForm::Whole) => {
                ui.label(checking_text(text));
            }
        }
    }
}

fn checking_text(text: &str) -> RichText {
    RichText::new(text).size(LABEL_SIZE).color(ON_SURFACE_DIM)
}

/// The language chip: shows the current language.
fn language_pill(code: &str) -> Pill<'_> {
    Pill {
        icon: Some((glyph::TRANSLATE, ON_SURFACE_DIM)),
        text: code,
        text_color: ON_SURFACE_DIM,
        fill: Color32::TRANSPARENT,
        stroke: Stroke::new(1.0, DIVIDER),
        ..Pill::default()
    }
}

/// Shows the language chip, which opens the list to choose another language.
fn render_language_selector(
    ui: &mut Ui,
    chip: &Pill<'_>,
    current: Language,
    on_action: &mut impl FnMut(UiAction),
) {
    let response = pill(ui, chip).on_hover_text(tr(ui.ctx()).language);
    Popup::menu(&response).show(|ui| {
        for language in Language::ALL {
            if ui
                .selectable_label(language == current, language.native_name())
                .clicked()
                && language != current
            {
                on_action(UiAction::SetLanguage(language));
            }
        }
    });
}
