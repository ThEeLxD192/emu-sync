//! Game grid of the selected category, below its toolbar, laid out like Kotlin's `GameGrid`.

use crate::{
    actions::UiAction,
    components::icons::paint_glyph,
    cover_cache::CoverCache,
    model::GameItem,
    state::AppUiState,
    theme::{
        CARD_HOVER, DIVIDER, ON_BACKGROUND, ON_SURFACE_DIM, PRIMARY, PRIMARY_CONTAINER, SURFACE,
        SURFACE_VARIANT, bold, glyph,
    },
};
use egui::{
    Align, Color32, CursorIcon, Rect, RichText, ScrollArea, Sense, Spinner, Stroke, StrokeKind, Ui,
    UiBuilder, pos2, text::LayoutJob, vec2,
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

    super::category_toolbar::render_category_toolbar(
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
        .show_viewport(ui, |ui, viewport| {
            let (columns, card_width) =
                adaptive_columns(ui.available_width(), MIN_CARD_WIDTH, CARD_GAP);
            ui.spacing_mut().item_spacing = vec2(CARD_GAP, CARD_GAP);
            // Only the rows in view are laid out, so a frame costs the same in front of a
            // large ROM folder as in front of a few games. Each row is a card high with a gap
            // below it, and the list ends 8 points below the last gap.
            let row_height_with_gap = CARD_HEIGHT + CARD_GAP;
            let total_rows = state.game_items.len().div_ceil(columns);
            ui.set_height(total_rows as f32 * row_height_with_gap + 8.0);
            let first_row =
                ((viewport.min.y / row_height_with_gap).floor().max(0.0) as usize).min(total_rows);
            // One row more: the content is placed on whole pixels and the viewport is not, so
            // a row whose top edge peeks in by under a pixel would otherwise be left out.
            let end_row = ((viewport.max.y / row_height_with_gap).ceil().max(0.0) as usize + 1)
                .min(total_rows);
            let content = ui.max_rect();
            let rows_top = content.top() + first_row as f32 * row_height_with_gap;
            let rows_rect = Rect::from_x_y_ranges(content.x_range(), rows_top..=content.bottom());
            ui.scope_builder(UiBuilder::new().max_rect(rows_rect), |ui| {
                // Each row takes one automatic id; skipping those of the rows above keeps
                // every row's id the same whichever rows are in view.
                ui.skip_ahead_auto_ids(first_row);
                let visible_rows = state
                    .game_items
                    .chunks(columns)
                    .skip(first_row)
                    .take(end_row.saturating_sub(first_row));
                for row in visible_rows {
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
            });
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
