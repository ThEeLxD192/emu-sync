//! Game grid in front of libraries of every size: every game is reachable by scrolling,
//! a click lands on the card under the pointer, and resizing the window or changing the
//! library while scrolled keeps a whole grid.

use crate::common;

use egui::{
    Context, Event, Modifiers, MouseWheelSource, MouseWheelUnit, Pos2, RawInput, Rect, Shape,
    TouchPhase, pos2, vec2,
};
use emusync_domain::library::GameEntry;
use emusync_presentation::{EmuSyncApp, GameItem, UiAction, theme::glyph::PENCIL};
use std::collections::BTreeSet;

/// Window height used by every test; only the width changes.
const WINDOW_HEIGHT: f32 = 800.0;
/// Space below the last row at the end of the scroll, as the grid always left it: the
/// 12-point gap after a row and 8 more points that end the list.
const BOTTOM_MARGIN: f32 = 20.0;
/// A card must show at least this many points to be clicked in a test.
const MIN_CLICKABLE_STRIP: f32 = 2.0;

fn library_entry() -> GameEntry {
    common::emulator_entry("Library", "/roms", &["iso"])
}

fn game_title(index: usize) -> String {
    format!("Game {index:04}")
}

fn game_item(index: usize) -> GameItem {
    GameItem::new(game_title(index), library_entry(), None)
}

fn games(count: usize) -> Vec<GameItem> {
    (0..count).map(game_item).collect()
}

/// A library of `count` games with its emulator selected.
fn library_app(count: usize) -> EmuSyncApp {
    let entry = library_entry();
    let mut app = EmuSyncApp::new();
    app.state.selected_entry = Some(entry.clone());
    app.state.config = Some(common::config_with(vec![entry], None));
    app.state.game_items = games(count);
    app
}

fn window(width: f32) -> Rect {
    Rect::from_min_size(Pos2::ZERO, vec2(width, WINDOW_HEIGHT))
}

/// A card painted in a frame: its title, its rectangle and the clip of the grid.
#[derive(Debug, Clone)]
struct Card {
    title: String,
    rect: Rect,
    clip: Rect,
}

impl Card {
    fn index(&self) -> usize {
        self.title["Game ".len()..].parse().expect("a game title")
    }

    fn visible_part(&self) -> Rect {
        self.rect.intersect(self.clip)
    }

    fn is_visible(&self) -> bool {
        let visible = self.visible_part();
        visible.width() > 0.0 && visible.height() > 0.0
    }

    fn is_fully_visible(&self) -> bool {
        self.clip.contains_rect(self.rect)
    }
}

fn is_game_title(text: &str) -> bool {
    text.strip_prefix("Game ")
        .is_some_and(|number| number.len() == 4 && number.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Runs one frame and returns every card the grid painted, visible or not.
fn frame(ctx: &Context, app: &mut EmuSyncApp, width: f32, events: Vec<Event>) -> Vec<Card> {
    let input = RawInput {
        screen_rect: Some(window(width)),
        events,
        ..RawInput::default()
    };
    let output = ctx.run_ui(input, |ui| app.update(ui));
    let rects: Vec<(Rect, Rect)> = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Rect(rect) => Some((rect.rect, clipped.clip_rect)),
            _ => None,
        })
        .collect();
    let cards = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(text) if is_game_title(text.galley.text()) => {
                let title_center = text.visual_bounding_rect().center();
                // The card is the smallest rectangle painted in the grid around its title
                // (the cover sits above the title; a clipped title may lie over the
                // toolbar, which is painted with another clip).
                let rect = rects
                    .iter()
                    .filter(|(rect, clip)| {
                        *clip == clipped.clip_rect && rect.contains(title_center)
                    })
                    .map(|(rect, _)| *rect)
                    .min_by(|a, b| a.area().total_cmp(&b.area()))
                    .expect("a card around its title");
                Some(Card {
                    title: text.galley.text().to_string(),
                    rect,
                    clip: clipped.clip_rect,
                })
            }
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    cards
}

fn visible_cards(cards: &[Card]) -> Vec<Card> {
    cards
        .iter()
        .filter(|card| card.is_visible())
        .cloned()
        .collect()
}

/// Frames until the painted cards stop moving; returns the last frame's cards.
fn settle(ctx: &Context, app: &mut EmuSyncApp, width: f32) -> Vec<Card> {
    let mut previous: Option<Vec<(String, Rect)>> = None;
    let mut stable_frames = 0;
    for _ in 0..60 {
        let cards = frame(ctx, app, width, Vec::new());
        let current: Vec<(String, Rect)> = cards
            .iter()
            .map(|card| (card.title.clone(), card.rect))
            .collect();
        stable_frames = if previous.as_ref() == Some(&current) {
            stable_frames + 1
        } else {
            0
        };
        if stable_frames >= 4 {
            return cards;
        }
        previous = Some(current);
    }
    panic!("the grid never settled");
}

/// A point inside the grid of a window `width` wide, under the toolbar.
fn grid_point(width: f32) -> Pos2 {
    // The sidebar takes the first 240 points; the grid fills the rest.
    pos2(240.0 + (width - 240.0) / 2.0, WINDOW_HEIGHT / 2.0)
}

/// Scrolls the grid down by `points` in one frame, then lets the grid settle.
///
/// A scroll that starts with `TouchPhase::Start` is applied at once (a plain wheel step is
/// spread over several frames), and an `Unknown` source is clamped to the content like a
/// mouse wheel (a trackpad would rubber-band past the end and spring back over many frames).
fn scroll_down(ctx: &Context, app: &mut EmuSyncApp, width: f32, points: f32) -> Vec<Card> {
    let wheel = |delta: f32, phase| Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, -delta),
        phase,
        source: MouseWheelSource::Unknown,
        modifiers: Modifiers::NONE,
    };
    frame(
        ctx,
        app,
        width,
        vec![
            Event::PointerMoved(grid_point(width)),
            wheel(0.0, TouchPhase::Start),
            wheel(points, TouchPhase::Move),
        ],
    );
    frame(ctx, app, width, vec![wheel(0.0, TouchPhase::End)]);
    settle(ctx, app, width)
}

/// Scrolls in large steps until the grid stops moving.
fn scroll_to_end(ctx: &Context, app: &mut EmuSyncApp, width: f32) -> Vec<Card> {
    let mut cards = settle(ctx, app, width);
    for _ in 0..50 {
        let next = scroll_down(ctx, app, width, 1_000_000.0);
        let unchanged = next.len() == cards.len()
            && next
                .iter()
                .zip(&cards)
                .all(|(after, before)| after.title == before.title && after.rect == before.rect);
        cards = next;
        if unchanged {
            return cards;
        }
    }
    panic!("the grid kept scrolling");
}

/// Clicks `pos` in a window `width` wide and returns the actions the click published.
fn click(ctx: &Context, app: &mut EmuSyncApp, width: f32, pos: Pos2) -> Vec<UiAction> {
    app.take_actions();
    for events in common::click_at(pos) {
        frame(ctx, app, width, events);
    }
    app.take_actions()
}

fn assert_click_launches(ctx: &Context, app: &mut EmuSyncApp, width: f32, card: &Card) {
    let actions = click(ctx, app, width, card.visible_part().center());
    assert_eq!(
        actions,
        vec![UiAction::LaunchGame(game_item(card.index()))],
        "click on {} at {:?} (visible part {:?})",
        card.title,
        card.visible_part().center(),
        card.visible_part()
    );
}

/// The view left the top of the library: a whole grid is in view and none of its cards is in
/// the first row. Guards the tests that start from a scrolled view. Returns the columns.
fn assert_scrolled_past_first_row(cards: &[Card], game_count: usize) -> usize {
    let columns = assert_whole_grid(cards, game_count);
    assert!(
        visible_cards(cards)
            .iter()
            .all(|card| card.index() >= columns),
        "the view did not leave the first row: {cards:?}"
    );
    columns
}

/// The visible cards form a whole grid of `game_count` games laid out row by row: no game
/// twice, no game skipped, every row full except the library's last, rows evenly spaced
/// and covering the view. Returns the number of columns.
fn assert_whole_grid(cards: &[Card], game_count: usize) -> usize {
    let visible = visible_cards(cards);
    assert!(!visible.is_empty(), "no card is visible");

    let titles: Vec<&str> = visible.iter().map(|card| card.title.as_str()).collect();
    let unique: BTreeSet<&str> = titles.iter().copied().collect();
    assert_eq!(unique.len(), titles.len(), "a card is repeated: {titles:?}");

    let mut indices: Vec<usize> = visible.iter().map(Card::index).collect();
    indices.sort_unstable();
    assert!(
        indices.iter().all(|&index| index < game_count),
        "a card of a game that is not in the library: {indices:?}"
    );
    let first = indices[0];
    let last = *indices.last().expect("at least one card");
    assert_eq!(
        indices,
        (first..=last).collect::<Vec<_>>(),
        "a game is missing between the visible cards"
    );

    // Rows, top to bottom, each sorted left to right.
    let mut rows: Vec<Vec<Card>> = Vec::new();
    let mut by_top = visible.clone();
    by_top.sort_by(|a, b| {
        a.rect
            .top()
            .total_cmp(&b.rect.top())
            .then(a.rect.left().total_cmp(&b.rect.left()))
    });
    for card in by_top {
        match rows.last_mut() {
            Some(row) if (row[0].rect.top() - card.rect.top()).abs() < 0.5 => row.push(card),
            _ => rows.push(vec![card]),
        }
    }
    let columns = rows.iter().map(Vec::len).max().expect("a row");
    for row in &rows {
        let row_indices: Vec<usize> = row.iter().map(Card::index).collect();
        assert_eq!(
            row_indices[0] % columns,
            0,
            "a row does not start a new line of {columns}: {row_indices:?}"
        );
        let holds_last_game = row_indices.contains(&(game_count - 1));
        if !holds_last_game {
            assert_eq!(row.len(), columns, "a row with a gap: {row_indices:?}");
        }
        assert_eq!(
            row_indices,
            (row_indices[0]..row_indices[0] + row.len()).collect::<Vec<_>>(),
            "a row out of order"
        );
    }

    // Rows evenly spaced, and no empty band at the top or bottom of the view other than
    // the gap between two rows.
    if rows.len() >= 2 {
        let pitch = rows[1][0].rect.top() - rows[0][0].rect.top();
        let gap = pitch - rows[0][0].rect.height();
        assert!(gap >= 0.0, "rows overlap: pitch {pitch}");
        for pair in rows.windows(2) {
            let step = pair[1][0].rect.top() - pair[0][0].rect.top();
            assert!((step - pitch).abs() < 1.0, "uneven rows: {step} vs {pitch}");
        }
        let clip = rows[0][0].clip;
        assert!(
            rows[0][0].rect.top() <= clip.top() + gap + 1.0,
            "empty band above the first row: row at {}, view at {}",
            rows[0][0].rect.top(),
            clip.top()
        );
        let last_row = rows.last().expect("a row");
        let shows_last_game = last_row.iter().any(|card| card.index() == game_count - 1);
        if !shows_last_game {
            assert!(
                last_row[0].rect.bottom() >= clip.bottom() - gap - 1.0,
                "empty band below the last row: row ends at {}, view at {}",
                last_row[0].rect.bottom(),
                clip.bottom()
            );
        }
    }
    columns
}

/// At the end of the scroll the library's last row is whole and, when the library is taller
/// than the view, the usual margin is left below it (within a point of pixel rounding).
fn assert_end_of_library(cards: &[Card], game_count: usize, columns: usize) {
    let fits_in_view = cards
        .iter()
        .find(|card| card.index() == 0)
        .is_some_and(Card::is_fully_visible);
    let last_row_start = (game_count - 1) / columns * columns;
    for index in last_row_start..game_count {
        let card = cards
            .iter()
            .find(|card| card.index() == index)
            .unwrap_or_else(|| panic!("{} is not painted at the end", game_title(index)));
        assert!(
            card.is_fully_visible(),
            "{} is cut at the end: {:?} in {:?}",
            card.title,
            card.rect,
            card.clip
        );
        if fits_in_view {
            continue;
        }
        let margin = card.clip.bottom() - card.rect.bottom();
        assert!(
            (margin - BOTTOM_MARGIN).abs() <= 1.0,
            "{} ends {margin} points above the bottom of the view",
            card.title
        );
    }
}

#[test]
fn scrolling_through_a_library_with_a_partial_last_row_shows_every_game_whole() {
    // 5 columns on a 1280 window: 20 full rows and a last row of 3.
    const GAME_COUNT: usize = 103;
    let ctx = Context::default();
    let mut app = library_app(GAME_COUNT);
    let mut cards = settle(&ctx, &mut app, 1280.0);
    let mut seen_whole = BTreeSet::new();

    for _ in 0..400 {
        assert_whole_grid(&cards, GAME_COUNT);
        seen_whole.extend(
            cards
                .iter()
                .filter(|card| card.is_fully_visible())
                .map(Card::index),
        );
        if seen_whole.contains(&(GAME_COUNT - 1)) {
            break;
        }
        // Less than a row and not a whole number of points.
        cards = scroll_down(&ctx, &mut app, 1280.0, 97.3);
    }

    assert_eq!(seen_whole, (0..GAME_COUNT).collect::<BTreeSet<_>>());
}

#[test]
fn a_library_scrolled_to_its_end_shows_the_whole_last_row_above_a_small_margin() {
    for game_count in [7, 103, 5_003] {
        let ctx = Context::default();
        let mut app = library_app(game_count);

        let cards = scroll_to_end(&ctx, &mut app, 1280.0);

        let columns = assert_whole_grid(&cards, game_count);
        assert_end_of_library(&cards, game_count, columns);
    }
}

#[test]
fn the_first_screen_starts_with_the_first_game_at_the_top_of_the_view() {
    let ctx = Context::default();
    let mut app = library_app(5_000);

    let cards = settle(&ctx, &mut app, 1280.0);

    assert_whole_grid(&cards, 5_000);
    let first = cards
        .iter()
        .find(|card| card.index() == 0)
        .expect("the first game is painted");
    assert!(first.is_fully_visible(), "{first:?}");
    assert!(
        (first.rect.top() - first.clip.top()).abs() < 1.0,
        "the first row starts {} points into the view",
        first.rect.top() - first.clip.top()
    );
}

#[test]
fn clicking_any_visible_card_after_fractional_and_long_scrolls_launches_that_game() {
    const GAME_COUNT: usize = 5_003;
    let ctx = Context::default();
    let mut app = library_app(GAME_COUNT);
    settle(&ctx, &mut app, 1280.0);

    // Under a point, inside the first gap, a whole number of rows (5 rows of 205 + 12),
    // fractional amounts and a jump deep into the library.
    for step in [0.4, 6.6, 1_085.0, 216.5, 1_000.25, 3_333.3, 150_000.7] {
        let cards = scroll_down(&ctx, &mut app, 1280.0, step);
        assert_whole_grid(&cards, GAME_COUNT);
        let clickable: Vec<Card> = visible_cards(&cards)
            .into_iter()
            .filter(|card| card.visible_part().height() >= MIN_CLICKABLE_STRIP)
            .collect();
        assert!(clickable.len() >= 10, "after {step}: {clickable:?}");
        for card in &clickable {
            assert_click_launches(&ctx, &mut app, 1280.0, card);
        }
    }
}

#[test]
fn every_card_moves_up_by_exactly_the_distance_scrolled_even_deep_in_the_library() {
    const GAME_COUNT: usize = 5_003;
    let ctx = Context::default();
    let mut app = library_app(GAME_COUNT);
    // Where each column and row sits before any scroll: the oracle for every later frame.
    let first_screen = settle(&ctx, &mut app, 1280.0);
    let columns = assert_whole_grid(&first_screen, GAME_COUNT);
    let first_screen_rect_of = |index: usize| {
        first_screen
            .iter()
            .find(|card| card.index() == index)
            .unwrap_or_else(|| panic!("{} on the first screen", game_title(index)))
            .rect
    };
    let first_row_top = first_screen_rect_of(0).top();
    let row_height_with_gap = first_screen_rect_of(columns).top() - first_row_top;

    let mut scrolled = 0.0_f64;
    for step in [97.3, 216.5, 6.6, 1_085.0, 0.4, 333.3, 150_000.7, 12.5] {
        let cards = scroll_down(&ctx, &mut app, 1280.0, step as f32);
        scrolled += step;
        // A whole grid in view, so a misplaced row cannot pass by leaving no card to check.
        assert_whole_grid(&cards, GAME_COUNT);
        for card in visible_cards(&cards) {
            let index = card.index();
            let row = (index / columns) as f64;
            let expected_top =
                f64::from(first_row_top) + row * f64::from(row_height_with_gap) - scrolled;
            let expected_left = first_screen_rect_of(index % columns).left();
            assert!(
                (f64::from(card.rect.top()) - expected_top).abs() <= 1.0,
                "{} at {} after scrolling {scrolled}, expected {expected_top}",
                card.title,
                card.rect.top()
            );
            assert_eq!(card.rect.left(), expected_left, "{}", card.title);
        }
    }
}

#[test]
fn clicking_the_last_games_of_a_huge_library_launches_each_of_them() {
    const GAME_COUNT: usize = 5_003;
    let ctx = Context::default();
    let mut app = library_app(GAME_COUNT);

    let cards = scroll_to_end(&ctx, &mut app, 1280.0);

    for index in GAME_COUNT - 8..GAME_COUNT {
        let card = cards
            .iter()
            .find(|card| card.index() == index)
            .unwrap_or_else(|| panic!("{} is not painted at the end", game_title(index)));
        assert_click_launches(&ctx, &mut app, 1280.0, card);
    }
}

#[test]
fn the_edit_button_of_a_card_deep_in_the_library_edits_that_game() {
    const GAME_COUNT: usize = 2_000;
    let ctx = Context::default();
    let mut app = library_app(GAME_COUNT);
    settle(&ctx, &mut app, 1280.0);
    let cards = scroll_down(&ctx, &mut app, 1280.0, 40_000.7);
    assert_scrolled_past_first_row(&cards, GAME_COUNT);
    let target = visible_cards(&cards)
        .into_iter()
        .filter(|card| card.is_fully_visible())
        .nth(3)
        .expect("a whole card in view");

    for _ in 0..2 {
        frame(
            &ctx,
            &mut app,
            1280.0,
            vec![Event::PointerMoved(target.rect.center())],
        );
    }
    // The hovered card shows its edit button; the edit glyph of the toolbar is elsewhere.
    let input = RawInput {
        screen_rect: Some(window(1280.0)),
        events: vec![Event::PointerMoved(target.rect.center())],
        ..RawInput::default()
    };
    let output = ctx.run_ui(input, |ui| app.update(ui));
    let pencils: Vec<Pos2> = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(text) if text.galley.text() == PENCIL => {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        })
        .filter(|pencil| target.rect.contains(*pencil))
        .collect();
    output.drop_without_applying_deltas();
    assert_eq!(
        pencils.len(),
        1,
        "edit buttons on {}: {pencils:?}",
        target.title
    );

    let actions = click(&ctx, &mut app, 1280.0, pencils[0]);

    assert_eq!(actions, vec![UiAction::EditGame(game_item(target.index()))]);
}

#[test]
fn resizing_the_window_while_scrolled_keeps_a_whole_grid_of_clickable_cards() {
    const GAME_COUNT: usize = 1_001;
    let ctx = Context::default();
    let mut app = library_app(GAME_COUNT);
    settle(&ctx, &mut app, 1280.0);
    let scrolled = scroll_down(&ctx, &mut app, 1280.0, 20_000.5);
    assert_scrolled_past_first_row(&scrolled, GAME_COUNT);

    let mut column_counts = BTreeSet::new();
    // Fewer columns, more, the narrowest window whose toolbar still fits (two columns),
    // and back.
    for width in [900.0, 1920.0, 2560.0, 700.0, 1280.0, 1000.5] {
        let cards = settle(&ctx, &mut app, width);
        let columns = assert_whole_grid(&cards, GAME_COUNT);
        column_counts.insert(columns);
        let middle = visible_cards(&cards)
            .into_iter()
            .filter(|card| card.is_fully_visible())
            .nth(columns / 2)
            .unwrap_or_else(|| panic!("no whole card at width {width}"));
        assert_click_launches(&ctx, &mut app, width, &middle);
    }
    // The widths above really change the number of columns.
    assert!(column_counts.len() >= 5, "{column_counts:?}");

    // After all of it, the end of the library is still reachable at the narrowest width.
    let cards = scroll_to_end(&ctx, &mut app, 700.0);
    let columns = assert_whole_grid(&cards, GAME_COUNT);
    assert_eq!(columns, 2);
    assert_end_of_library(&cards, GAME_COUNT, columns);
}

#[test]
fn widening_the_window_at_the_end_of_the_library_still_shows_its_last_row_whole() {
    const GAME_COUNT: usize = 997;
    let ctx = Context::default();
    let mut app = library_app(GAME_COUNT);
    scroll_to_end(&ctx, &mut app, 700.0);

    // Wider means fewer rows, so the old offset is now past the end of the content.
    let cards = settle(&ctx, &mut app, 2560.0);

    let columns = assert_whole_grid(&cards, GAME_COUNT);
    assert_end_of_library(&cards, GAME_COUNT, columns);
    let last = cards
        .iter()
        .find(|card| card.index() == GAME_COUNT - 1)
        .expect("the last game is painted");
    assert_click_launches(&ctx, &mut app, 2560.0, last);
}

#[test]
fn shrinking_the_library_while_scrolled_deep_shows_the_remaining_games() {
    let ctx = Context::default();
    let mut app = library_app(3_000);
    scroll_to_end(&ctx, &mut app, 1280.0);

    for remaining in [40, 7, 1] {
        app.state.game_items = games(remaining);

        let cards = settle(&ctx, &mut app, 1280.0);

        let columns = assert_whole_grid(&cards, remaining);
        assert_end_of_library(&cards, remaining, columns);
        let last = cards
            .iter()
            .find(|card| card.index() == remaining - 1)
            .expect("the last game is painted");
        assert_click_launches(&ctx, &mut app, 1280.0, last);
    }
}

#[test]
fn emptying_the_library_while_scrolled_then_refilling_it_shows_a_whole_grid() {
    let ctx = Context::default();
    let mut app = library_app(3_000);
    settle(&ctx, &mut app, 1280.0);
    let scrolled = scroll_down(&ctx, &mut app, 1280.0, 30_000.0);
    assert_scrolled_past_first_row(&scrolled, 3_000);

    app.state.game_items = Vec::new();
    let cards = settle(&ctx, &mut app, 1280.0);
    assert!(cards.is_empty(), "{cards:?}");
    assert!(common::text_exists(&ctx, &mut app, "No ROMs found"));

    app.state.game_items = games(500);
    let cards = settle(&ctx, &mut app, 1280.0);
    assert_whole_grid(&cards, 500);
    let card = visible_cards(&cards)
        .into_iter()
        .find(Card::is_fully_visible)
        .expect("a whole card");
    assert_click_launches(&ctx, &mut app, 1280.0, &card);
}

#[test]
fn growing_the_library_at_its_end_lets_the_new_games_be_reached_and_launched() {
    let ctx = Context::default();
    let mut app = library_app(23);
    let cards = scroll_to_end(&ctx, &mut app, 1280.0);
    let columns = assert_whole_grid(&cards, 23);
    assert_end_of_library(&cards, 23, columns);

    app.state.game_items = games(4_321);
    let cards = settle(&ctx, &mut app, 1280.0);
    assert_whole_grid(&cards, 4_321);

    let cards = scroll_to_end(&ctx, &mut app, 1280.0);
    let columns = assert_whole_grid(&cards, 4_321);
    assert_end_of_library(&cards, 4_321, columns);
    let last = cards
        .iter()
        .find(|card| card.index() == 4_320)
        .expect("the last game is painted");
    assert_click_launches(&ctx, &mut app, 1280.0, last);
}

#[test]
fn a_single_game_stays_in_place_and_launches_after_a_scroll_attempt() {
    let ctx = Context::default();
    let mut app = library_app(1);
    let before = settle(&ctx, &mut app, 1280.0);

    let after = scroll_down(&ctx, &mut app, 1280.0, 500.0);

    assert_eq!(after.len(), 1, "{after:?}");
    assert_eq!(after[0].rect, before[0].rect);
    assert!(after[0].is_fully_visible());
    assert_click_launches(&ctx, &mut app, 1280.0, &after[0]);
}

#[test]
fn an_empty_library_ignores_scrolling_and_shows_where_to_add_roms() {
    let ctx = Context::default();
    let mut app = library_app(0);

    let cards = scroll_down(&ctx, &mut app, 1280.0, 500.0);

    assert!(cards.is_empty(), "{cards:?}");
    assert!(common::text_exists(&ctx, &mut app, "No ROMs found"));
    assert!(common::text_exists(&ctx, &mut app, "Add ROMs to: /roms"));
    assert_eq!(app.take_actions(), Vec::<UiAction>::new());
}

/// Widgets the last finished frame registered (for interaction), in every layer. egui
/// moves a finished frame's widgets to `prev_pass`.
fn registered_widget_count(ctx: &Context) -> usize {
    ctx.viewport(|viewport| {
        viewport
            .prev_pass
            .widgets
            .layers()
            .map(|(_, widgets)| widgets.len())
            .sum()
    })
}

#[test]
fn a_frame_lays_out_the_same_widgets_in_front_of_a_small_or_a_huge_library() {
    // Not a timing test: the cards out of view must not be laid out at all, so a frame
    // registers as many widgets in front of 20 000 games as in front of 100.
    let widgets_after = |game_count: usize, scroll_points: f32| {
        let ctx = Context::default();
        let mut app = library_app(game_count);
        settle(&ctx, &mut app, 1280.0);
        if scroll_points > 0.0 {
            let cards = scroll_down(&ctx, &mut app, 1280.0, scroll_points);
            assert_scrolled_past_first_row(&cards, game_count);
        }
        frame(&ctx, &mut app, 1280.0, Vec::new());
        registered_widget_count(&ctx)
    };

    let first_screen = widgets_after(100, 0.0);
    assert_eq!(widgets_after(20_000, 0.0), first_screen);
    // Deep in the library, the games below the view still cost nothing...
    let deep = widgets_after(20_000, 400_000.3);
    assert_eq!(widgets_after(40_000, 400_000.3), deep);
    // ...and neither do the games above it: the frame registers fewer than twice the widgets
    // of the first screen, which has the same panels and toolbar around the cards.
    assert!(
        deep < first_screen * 2,
        "{deep} widgets deep in the library, {first_screen} on the first screen"
    );
}
