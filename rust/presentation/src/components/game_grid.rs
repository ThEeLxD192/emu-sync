//! Category toolbar and adaptive game grid, laid out like Kotlin's `GameGrid`.

use crate::{
    actions::UiAction,
    components::{
        icons::{glyph_button, paint_glyph},
        pill::{LABEL_SIZE, Pill},
    },
    cover_cache::CoverCache,
    model::{CloudSyncStatus, GameItem},
    state::AppUiState,
    theme::{
        CARD_HOVER, DIVIDER, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY, PRIMARY_CONTAINER,
        SURFACE, SURFACE_VARIANT, WARNING, bold, glyph,
    },
};
use egui::{
    Align, Color32, CursorIcon, FontId, Frame, Label, Layout, Margin, Rect, RichText, ScrollArea,
    Sense, Spinner, Stroke, StrokeKind, Ui, pos2, text::LayoutJob, vec2,
};
use emusync_domain::library::GameEntry;

const MIN_CARD_WIDTH: f32 = 160.0;
const CARD_GAP: f32 = 12.0;
const CARD_HEIGHT: f32 = 205.0;
const COVER_HEIGHT: f32 = 140.0;

/// Number of columns and card width for an adaptive grid, as Compose's
/// `GridCells.Adaptive(minSize)`: as many columns as fit, stretched to fill the width.
pub fn adaptive_columns(available_width: f32, min_width: f32, gap: f32) -> (usize, f32) {
    let columns = (((available_width + gap) / (min_width + gap)).floor() as usize).max(1);
    let width = ((available_width - gap * (columns - 1) as f32) / columns as f32).max(0.0);
    (columns, width)
}

/// Renders the category toolbar and the game grid for the selected entry.
pub fn render_game_grid(
    ui: &mut Ui,
    state: &AppUiState,
    cover_cache: &mut CoverCache,
    on_action: &mut impl FnMut(UiAction),
    on_edit_game: &mut impl FnMut(GameItem),
    on_edit_category: &mut impl FnMut(GameEntry),
    on_delete_category: &mut impl FnMut(GameEntry),
) {
    let t = crate::i18n::tr(ui.ctx());
    let Some(selected) = &state.selected_entry else {
        empty_state(ui, t.select_category, None);
        return;
    };

    render_category_toolbar(
        ui,
        state,
        selected,
        on_action,
        on_edit_category,
        on_delete_category,
    );
    ui.add_space(12.0);

    if state.is_loading {
        ui.centered_and_justified(|ui| {
            ui.add(Spinner::new().size(32.0).color(PRIMARY));
        });
        return;
    }

    if state.game_items.is_empty() {
        match selected {
            GameEntry::Emulator(emulator) => empty_state(
                ui,
                t.no_roms_found,
                Some(&crate::i18n::fill(
                    t.add_roms_to,
                    &[("folder", &emulator.roms_directory)],
                )),
            ),
            GameEntry::Native(_) => empty_state(ui, t.select_category, None),
        }
        return;
    }

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let (columns, card_width) =
                adaptive_columns(ui.available_width(), MIN_CARD_WIDTH, CARD_GAP);
            ui.spacing_mut().item_spacing = vec2(CARD_GAP, CARD_GAP);
            for row in state.game_items.chunks(columns) {
                ui.horizontal(|ui| {
                    for item in row {
                        render_game_card(
                            ui,
                            item,
                            card_width,
                            cover_cache,
                            on_action,
                            on_edit_game,
                        );
                    }
                });
            }
            ui.add_space(8.0);
        });
}

fn empty_state(ui: &mut Ui, title: &str, detail: Option<&str>) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(title).size(15.0).color(ON_SURFACE_DIM));
            if let Some(detail) = detail {
                ui.add_space(4.0);
                ui.label(RichText::new(detail).size(12.0).color(ON_SURFACE_DIM));
            }
        });
    });
}

fn render_category_toolbar(
    ui: &mut Ui,
    state: &AppUiState,
    selected: &GameEntry,
    on_action: &mut impl FnMut(UiAction),
    on_edit_category: &mut impl FnMut(GameEntry),
    on_delete_category: &mut impl FnMut(GameEntry),
) {
    let name = &selected.common().name;
    // Fixed row height so every item is centered on the same line.
    let row = vec2(ui.available_width(), 40.0);
    ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_height(40.0);
        ui.spacing_mut().item_spacing.x = 0.0;
        // Actions are laid out first (from the right) so the title only takes what is left.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let t = crate::i18n::tr(ui.ctx());
            let count = state.game_items.len();
            let games = if count == 1 {
                t.game_count_one.to_string()
            } else {
                crate::i18n::fill(t.game_count_other, &[("count", &count)])
            };
            Pill {
                text: &games,
                text_color: PRIMARY,
                font: FontId::proportional(12.0),
                fill: PRIMARY_CONTAINER,
                padding: vec2(12.0, 4.0),
                corner_radius: 12,
                interactive: false,
                ..Pill::default()
            }
            .show(ui);
            ui.add_space(8.0);

            if toolbar_icon(ui, "category_delete", glyph::DELETE, t.delete_category).clicked() {
                on_delete_category(selected.clone());
            }
            if toolbar_icon(ui, "category_edit", glyph::PENCIL, t.edit_category).clicked() {
                on_edit_category(selected.clone());
            }
            ui.add_space(8.0);

            match sync_chip(ui, state.sync_status_for_selected()) {
                Some(SyncChipAction::Sync) => on_action(UiAction::SyncEntry(selected.clone())),
                Some(SyncChipAction::Check) => {
                    on_action(UiAction::CheckSyncEntry(selected.clone()))
                }
                None => {}
            }

            if let GameEntry::Emulator(emulator) = selected {
                ui.add_space(8.0);
                let open = Pill {
                    icon: Some((glyph::COG, PRIMARY)),
                    text: t.open_emulator,
                    text_color: PRIMARY,
                    font: FontId::proportional(13.0),
                    fill: Color32::TRANSPARENT,
                    stroke: Stroke::new(1.0, DIVIDER),
                    padding: vec2(16.0, 8.0),
                    min_height: 36.0,
                    corner_radius: 18,
                    ..Pill::default()
                };
                if open.show(ui).clicked() {
                    on_action(UiAction::OpenEmulator(emulator.clone()));
                }
            }

            ui.add_space(12.0);
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

fn toolbar_icon(ui: &mut Ui, id: &str, icon: &str, label: &str) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
    let id = ui.make_persistent_id(id);
    glyph_button(ui, id, rect, icon, 20.0, ON_SURFACE_DIM).on_hover_text(label)
}

enum SyncChipAction {
    Sync,
    Check,
}

/// Cloud status chip with Kotlin's per-state presentation.
fn sync_chip(ui: &mut Ui, status: CloudSyncStatus) -> Option<SyncChipAction> {
    let t = crate::i18n::tr(ui.ctx());
    let button = |icon, color, text, fill, stroke| Pill {
        icon: Some((icon, color)),
        text,
        text_color: color,
        fill,
        stroke,
        ..Pill::default()
    };
    match status {
        CloudSyncStatus::InSync => {
            let mut check = false;
            // Right to left: refresh, label, then the status icon.
            status_frame(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                let id = ui.make_persistent_id("sync_check");
                check = glyph_button(ui, id, rect, glyph::REFRESH, 13.0, ON_SURFACE_DIM)
                    .on_hover_text(t.check_sync)
                    .clicked();
                ui.add_space(4.0);
                ui.label(RichText::new(t.in_sync).size(LABEL_SIZE).color(ON_SURFACE));
                icon_slot(ui, glyph::CLOUD_CHECK, crate::theme::SUCCESS);
            });
            check.then_some(SyncChipAction::Check)
        }
        CloudSyncStatus::OutOfSync => button(
            glyph::CLOUD_DOWNLOAD,
            PRIMARY,
            t.out_of_sync_sync_all,
            PRIMARY_CONTAINER,
            Stroke::NONE,
        )
        .show(ui)
        .clicked()
        .then_some(SyncChipAction::Sync),
        CloudSyncStatus::Conflict => button(
            glyph::ALERT,
            WARNING,
            t.conflict_sync,
            WARNING.gamma_multiply(0.2),
            Stroke::NONE,
        )
        .show(ui)
        .clicked()
        .then_some(SyncChipAction::Sync),
        CloudSyncStatus::Checking | CloudSyncStatus::Syncing => {
            let text = if status == CloudSyncStatus::Syncing {
                t.syncing
            } else {
                t.checking
            };
            // Right to left: label, then the spinner.
            status_frame(ui, |ui| {
                ui.label(RichText::new(text).size(LABEL_SIZE).color(ON_SURFACE));
                ui.add_space(6.0);
                ui.add(Spinner::new().size(14.0).color(PRIMARY));
            });
            None
        }
        CloudSyncStatus::Idle | CloudSyncStatus::Error => button(
            glyph::CLOUD_SYNC,
            PRIMARY,
            t.sync_all,
            Color32::TRANSPARENT,
            Stroke::new(1.0, DIVIDER),
        )
        .show(ui)
        .clicked()
        .then_some(SyncChipAction::Sync),
        CloudSyncStatus::NotConfigured => None,
    }
}

/// Status chip in the right-to-left toolbar. `add_contents` adds items from right to
/// left: a left-to-right child would take the whole remaining width, stretching the
/// chip and leaving no room for the category title.
fn status_frame(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(SURFACE_VARIANT)
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                add_contents(ui);
            });
        });
}

/// Status icon followed by its gap to the label, for a right-to-left chip.
fn icon_slot(ui: &mut Ui, icon: &str, color: Color32) {
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
    paint_glyph(ui, rect.center(), icon, 16.0, color);
}

fn render_game_card(
    ui: &mut Ui,
    item: &GameItem,
    card_width: f32,
    cover_cache: &mut CoverCache,
    on_action: &mut impl FnMut(UiAction),
    on_edit_game: &mut impl FnMut(GameItem),
) {
    let is_enabled = ui.is_enabled();
    let (card_rect, card_response) =
        ui.allocate_exact_size(vec2(card_width, CARD_HEIGHT), Sense::click());
    if !ui.is_rect_visible(card_rect) {
        return;
    }
    let card_response = card_response.on_hover_cursor(CursorIcon::PointingHand);
    let hovered = is_enabled && ui.rect_contains_pointer(card_rect);
    let painter = ui.painter();

    let (fill, border) = if hovered {
        (CARD_HOVER, PRIMARY)
    } else {
        (SURFACE_VARIANT, DIVIDER)
    };
    painter.rect(
        card_rect,
        12,
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );

    let inner = card_rect.shrink(8.0);
    let cover_rect = Rect::from_min_size(inner.min, vec2(inner.width(), COVER_HEIGHT));
    let texture = item
        .effective_cover_path()
        .as_deref()
        .and_then(|path| cover_cache.get_or_load(ui.ctx(), path));

    if let Some(texture) = texture {
        // ContentScale.Fit: keep the aspect ratio inside the cover slot.
        let size = texture.size_vec2();
        let scale = (cover_rect.width() / size.x).min(cover_rect.height() / size.y.max(1.0));
        let image_rect = Rect::from_center_size(cover_rect.center(), size * scale);
        ui.painter().image(
            texture.id(),
            image_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if hovered {
            ui.painter()
                .rect_filled(cover_rect, 8, Color32::from_black_alpha(115));
            paint_glyph(ui, cover_rect.center(), glyph::PLAY, 36.0, Color32::WHITE);
        }
    } else {
        ui.painter()
            .rect_filled(cover_rect, 8, SURFACE.gamma_multiply(0.5));
        let play_box = Rect::from_center_size(cover_rect.center(), vec2(56.0, 56.0));
        ui.painter().rect_filled(play_box, 14, PRIMARY_CONTAINER);
        paint_glyph(ui, play_box.center(), glyph::PLAY, 32.0, PRIMARY);
    }

    // Title: centered, up to two lines with an ellipsis.
    let title = item.effective_title();
    let mut job = LayoutJob::simple(title, bold(13.0), ON_BACKGROUND, inner.width() - 8.0);
    job.wrap.max_rows = 2;
    job.halign = Align::Center;
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        pos2(inner.center().x, cover_rect.bottom() + 8.0),
        galley,
        ON_BACKGROUND,
    );

    // Edit button on hover, registered after the card so it receives the click.
    let mut edit_clicked = false;
    if hovered {
        let edit_rect = Rect::from_min_size(
            pos2(inner.right() - 4.0 - 22.0, inner.top() + 4.0),
            vec2(22.0, 22.0),
        );
        let edit_id = ui.make_persistent_id(("edit_game", &item.name, &item.rom_file));
        let edit = ui
            .interact(edit_rect, edit_id, Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(crate::i18n::tr(ui.ctx()).edit_saves);
        let alpha = if edit.hovered() { 200 } else { 140 };
        ui.painter()
            .circle_filled(edit_rect.center(), 11.0, Color32::from_black_alpha(alpha));
        paint_glyph(
            ui,
            edit_rect.center(),
            glyph::PENCIL,
            12.0,
            Color32::WHITE.gamma_multiply(0.9),
        );
        edit_clicked = edit.clicked();
    }

    if !is_enabled {
        return;
    }
    if edit_clicked {
        on_edit_game(item.clone());
    } else if card_response.clicked() {
        on_action(UiAction::LaunchGame(item.clone()));
    }
}
