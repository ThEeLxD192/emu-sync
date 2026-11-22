//! Toolbar above the game grid: the category title and its actions. When the actions do
//! not fit beside a readable title, the buttons show only their icon (their text becomes
//! the tooltip), and then the game count is left out.

use crate::{
    actions::UiAction,
    components::{
        icons::{glyph_button, paint_glyph},
        pill::{LABEL_SIZE, Pill, PillForm, text_width},
    },
    i18n::{Texts, fill, tr},
    model::CloudSyncStatus,
    state::AppUiState,
    theme::{
        DIVIDER, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, PRIMARY_CONTAINER, SUCCESS,
        SURFACE_VARIANT, WARNING, bold, glyph,
    },
};
use egui::{
    Align, Color32, FontId, Frame, Label, Layout, Margin, RichText, Sense, Spinner, Stroke, Ui,
    vec2,
};
use emusync_domain::library::GameEntry;

/// Width the title keeps before the actions start to shrink.
const MIN_TITLE_WIDTH: f32 = 96.0;
const ROW_HEIGHT: f32 = 40.0;
const TOOLBAR_ICON_SIZE: f32 = 36.0;
const SMALL_GAP: f32 = 8.0;
const TITLE_GAP: f32 = 12.0;

/// Renders the category title with its game count, edit and delete buttons, sync chip and,
/// for an emulator, the button that opens it.
pub(crate) fn render_category_toolbar(
    ui: &mut Ui,
    state: &AppUiState,
    selected: &GameEntry,
    on_action: &mut impl FnMut(UiAction),
    on_edit_category: &mut impl FnMut(GameEntry),
    on_delete_category: &mut impl FnMut(GameEntry),
) {
    let t = tr(ui.ctx());
    let name = &selected.common().name;
    let count = state.game_items.len();
    let games = if count == 1 {
        t.game_count_one.to_string()
    } else {
        fill(t.game_count_other, &[("count", &count)])
    };
    let actions = ToolbarActions {
        count: Pill {
            text: &games,
            text_color: PRIMARY,
            font: FontId::proportional(12.0),
            fill: PRIMARY_CONTAINER,
            padding: vec2(12.0, 4.0),
            corner_radius: 12,
            interactive: false,
            ..Pill::default()
        },
        sync: SyncChip::for_status(t, state.sync_status_for_selected()),
        open_emulator: matches!(selected, GameEntry::Emulator(_)).then(|| open_emulator_pill(t)),
    };

    // Fixed row height so every item is centered on the same line.
    let row = vec2(ui.available_width(), ROW_HEIGHT);
    ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_height(ROW_HEIGHT);
        ui.spacing_mut().item_spacing.x = 0.0;
        let room = ui.available_width() - MIN_TITLE_WIDTH;
        let fit = ToolbarFit::ROOMIEST_FIRST
            .into_iter()
            .find(|fit| actions.width(ui, *fit) <= room)
            .unwrap_or(ToolbarFit::IconsWithoutCount);
        let form = fit.pill_form();

        // Actions are laid out first (from the right) so the title only takes what is left.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if fit.shows_count() {
                actions.count.show(ui);
                ui.add_space(SMALL_GAP);
            }

            if toolbar_icon(ui, "category_delete", glyph::DELETE, t.delete_category).clicked() {
                on_delete_category(selected.clone());
            }
            if toolbar_icon(ui, "category_edit", glyph::PENCIL, t.edit_category).clicked() {
                on_edit_category(selected.clone());
            }
            ui.add_space(SMALL_GAP);

            match actions.sync.show(ui, t, form) {
                Some(SyncChipAction::Sync) => on_action(UiAction::SyncEntry(selected.clone())),
                Some(SyncChipAction::Check) => {
                    on_action(UiAction::CheckSyncEntry(selected.clone()))
                }
                None => {}
            }

            if let (Some(open), GameEntry::Emulator(emulator)) = (&actions.open_emulator, selected)
            {
                ui.add_space(SMALL_GAP);
                if open.show_as(ui, form, OPEN_EMULATOR_ICON).clicked() {
                    on_action(UiAction::OpenEmulator(emulator.clone()));
                }
            }

            ui.add_space(TITLE_GAP);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(
                    Label::new(RichText::new(name).font(bold(20.0)).color(ON_BACKGROUND))
                        .truncate(),
                )
                .on_hover_text(name);
            });
        });
    });
}

/// The open-emulator button always has this icon; it only completes the call.
const OPEN_EMULATOR_ICON: (&str, Color32) = (glyph::COG, PRIMARY);

fn open_emulator_pill(t: &'static Texts) -> Pill<'static> {
    Pill {
        icon: Some(OPEN_EMULATOR_ICON),
        text: t.open_emulator,
        text_color: PRIMARY,
        font: FontId::proportional(13.0),
        fill: Color32::TRANSPARENT,
        stroke: Stroke::new(1.0, DIVIDER),
        padding: vec2(16.0, 8.0),
        min_height: 36.0,
        corner_radius: 18,
        ..Pill::default()
    }
}

/// The toolbar's actions, built once to measure and to show them.
struct ToolbarActions<'a> {
    count: Pill<'a>,
    sync: SyncChip<'static>,
    open_emulator: Option<Pill<'static>>,
}

impl ToolbarActions<'_> {
    /// Width of the actions laid out as `fit` says, including the gap before the title.
    fn width(&self, ui: &Ui, fit: ToolbarFit) -> f32 {
        let form = fit.pill_form();
        let count = if fit.shows_count() {
            self.count.size(ui).x + SMALL_GAP
        } else {
            0.0
        };
        let open_emulator = self.open_emulator.as_ref().map_or(0.0, |open| {
            SMALL_GAP + open.width_as(ui, form, OPEN_EMULATOR_ICON)
        });
        count
            + 2.0 * TOOLBAR_ICON_SIZE
            + SMALL_GAP
            + self.sync.width(ui, form)
            + open_emulator
            + TITLE_GAP
    }
}

/// How the toolbar's actions make room for the title.
#[derive(Clone, Copy, Debug)]
enum ToolbarFit {
    /// Every button with its text.
    Whole,
    /// Buttons with only their icon.
    Icons,
    /// Buttons with only their icon, and no game count.
    IconsWithoutCount,
}

impl ToolbarFit {
    const ROOMIEST_FIRST: [Self; 3] = [Self::Whole, Self::Icons, Self::IconsWithoutCount];

    fn pill_form(self) -> PillForm {
        match self {
            Self::Whole => PillForm::Whole,
            Self::Icons | Self::IconsWithoutCount => PillForm::IconOnly,
        }
    }

    fn shows_count(self) -> bool {
        !matches!(self, Self::IconsWithoutCount)
    }
}

fn toolbar_icon(ui: &mut Ui, id: &str, icon: &str, label: &str) -> egui::Response {
    let (rect, _) =
        ui.allocate_exact_size(vec2(TOOLBAR_ICON_SIZE, TOOLBAR_ICON_SIZE), Sense::hover());
    let id = ui.make_persistent_id(id);
    glyph_button(ui, id, rect, icon, 20.0, ON_SURFACE_DIM).on_hover_text(label)
}

enum SyncChipAction {
    Sync,
    Check,
}

/// The cloud status chip of the selected category, with Kotlin's per-state presentation.
enum SyncChip<'a> {
    /// A button that syncs the category.
    Sync(Pill<'a>),
    /// The category is in sync; its refresh button checks again.
    InSync,
    /// A check or a sync is running; the text says which.
    Busy(&'a str),
    /// Drive is not configured: no chip.
    Hidden,
}

/// Inner margin of the status chips, their refresh button and their icon.
const STATUS_MARGIN: Margin = Margin::symmetric(10, 6);
const REFRESH_SIZE: f32 = 18.0;
const REFRESH_GAP: f32 = 4.0;
const STATUS_ICON_SIZE: f32 = 16.0;
const STATUS_ICON_GAP: f32 = 6.0;
const SPINNER_SIZE: f32 = 14.0;

impl SyncChip<'static> {
    fn for_status(t: &'static Texts, status: CloudSyncStatus) -> Self {
        let button = |icon, color, text, fill, stroke| {
            Self::Sync(Pill {
                icon: Some((icon, color)),
                text,
                text_color: color,
                fill,
                stroke,
                ..Pill::default()
            })
        };
        match status {
            CloudSyncStatus::InSync => Self::InSync,
            CloudSyncStatus::OutOfSync => button(
                glyph::CLOUD_DOWNLOAD,
                PRIMARY,
                t.out_of_sync_sync_all,
                PRIMARY_CONTAINER,
                Stroke::NONE,
            ),
            CloudSyncStatus::Conflict => button(
                glyph::ALERT,
                WARNING,
                t.conflict_sync,
                WARNING.gamma_multiply(0.2),
                Stroke::NONE,
            ),
            CloudSyncStatus::Checking => Self::Busy(t.checking),
            CloudSyncStatus::Syncing => Self::Busy(t.syncing),
            CloudSyncStatus::Idle | CloudSyncStatus::Error => button(
                glyph::CLOUD_SYNC,
                PRIMARY,
                t.sync_all,
                Color32::TRANSPARENT,
                Stroke::new(1.0, DIVIDER),
            ),
            CloudSyncStatus::NotConfigured => Self::Hidden,
        }
    }
}

/// The status chips keep their refresh button, status icon and spinner in every form;
/// only their label goes to the tooltip when the toolbar is short of room.
impl SyncChip<'_> {
    fn width(&self, ui: &Ui, form: PillForm) -> f32 {
        let margins = STATUS_MARGIN.sum().x;
        // The label and its gap to the icon; the label goes to the tooltip when icon-only.
        let label_width = |text| match form {
            PillForm::Whole => text_width(ui, status_label(text)) + STATUS_ICON_GAP,
            PillForm::IconOnly => 0.0,
        };
        match self {
            Self::Sync(button) => button.width_as(ui, form, (glyph::CLOUD_SYNC, PRIMARY)),
            Self::InSync => {
                margins
                    + REFRESH_SIZE
                    + REFRESH_GAP
                    + label_width(tr(ui.ctx()).in_sync)
                    + STATUS_ICON_SIZE
            }
            Self::Busy(text) => margins + label_width(text) + SPINNER_SIZE,
            Self::Hidden => 0.0,
        }
    }

    fn show(&self, ui: &mut Ui, t: &Texts, form: PillForm) -> Option<SyncChipAction> {
        let label = |ui: &mut Ui, text| {
            if form == PillForm::Whole {
                ui.label(status_label(text));
                ui.add_space(STATUS_ICON_GAP);
            }
        };
        let label_as_tooltip = |chip: egui::Response, text| {
            if form == PillForm::IconOnly {
                chip.on_hover_text(text);
            }
        };
        match self {
            Self::Sync(button) => button
                .show_as(ui, form, (glyph::CLOUD_SYNC, PRIMARY))
                .clicked()
                .then_some(SyncChipAction::Sync),
            Self::InSync => {
                let mut check = false;
                // Right to left: refresh, label, then the status icon.
                let chip = status_frame(ui, |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(REFRESH_SIZE, REFRESH_SIZE), Sense::hover());
                    let id = ui.make_persistent_id("sync_check");
                    check = glyph_button(ui, id, rect, glyph::REFRESH, 13.0, ON_SURFACE_DIM)
                        .on_hover_text(t.check_sync)
                        .clicked();
                    ui.add_space(REFRESH_GAP);
                    label(ui, t.in_sync);
                    status_icon(ui, glyph::CLOUD_CHECK, SUCCESS);
                });
                label_as_tooltip(chip, t.in_sync);
                check.then_some(SyncChipAction::Check)
            }
            Self::Busy(text) => {
                // Right to left: label, then the spinner.
                let chip = status_frame(ui, |ui| {
                    label(ui, text);
                    ui.add(Spinner::new().size(SPINNER_SIZE).color(PRIMARY));
                });
                label_as_tooltip(chip, text);
                None
            }
            Self::Hidden => None,
        }
    }
}

fn status_label(text: &str) -> RichText {
    RichText::new(text).size(LABEL_SIZE).color(ON_SURFACE)
}

/// Status chip in the right-to-left toolbar. `add_contents` adds items from right to
/// left: a left-to-right child would take the whole remaining width, stretching the
/// chip and leaving no room for the category title.
fn status_frame(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) -> egui::Response {
    Frame::new()
        .fill(SURFACE_VARIANT)
        .corner_radius(8)
        .inner_margin(STATUS_MARGIN)
        .show(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                add_contents(ui);
            });
        })
        .response
}

/// The status icon at the left end of a right-to-left chip.
fn status_icon(ui: &mut Ui, icon: &str, color: Color32) {
    let (rect, _) =
        ui.allocate_exact_size(vec2(STATUS_ICON_SIZE, STATUS_ICON_SIZE), Sense::hover());
    paint_glyph(ui, rect.center(), icon, STATUS_ICON_SIZE, color);
}
