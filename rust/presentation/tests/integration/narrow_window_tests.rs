//! The main view at every window width from the 600-point minimum, in English and Spanish:
//! the header and the category toolbar never overlap or leave the window, wide windows show
//! every button with its text, narrow ones show icons whose text becomes the tooltip, and
//! every button keeps its action in both forms.
//!
//! The views are drawn with the application fonts (`configure_egui_theme`), as the app
//! does: what fits depends on the real width of the texts.

use crate::common;

use egui::{
    Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, ViewportId,
    epaint::ClippedShape, vec2,
};
use emusync_application::{DownloadFailure, UpdateState};
use emusync_domain::{
    config::{Credentials, Language},
    library::GameEntry,
};
use emusync_presentation::{
    ActiveDialog, CloudSyncStatus, EmuSyncApp, GameItem, UiAction, configure_egui_theme,
    theme::glyph,
};

/// Smallest window the app allows, in points.
const MIN_WINDOW_WIDTH: f32 = 600.0;
const MIN_WINDOW_HEIGHT: f32 = 480.0;
/// The sidebar takes this many points on the left, below the header.
const SIDEBAR_WIDTH: f32 = 240.0;
/// Two painted elements closer than this are not counted as overlapping (pixel rounding).
const OVERLAP_TOLERANCE: f32 = 0.5;
/// The category title keeps at least this many visible points at the minimum width.
const MIN_VISIBLE_TITLE: f32 = 90.0;
/// A long version, as a release candidate would show it.
const VERSION: &str = "10.20.300-rc.1";
const VERSION_LABEL: &str = "v10.20.300-rc.1";
const LONG_NAME: &str = "Nintendo Entertainment System Classics Collection";
const SHORT_NAME: &str = "GBA";

/// The visible texts the tests look for, written out per language.
struct Words {
    safe_close: &'static str,
    check_updates: &'static str,
    update_to: &'static str,
    restart: &'static str,
    downloading_half: &'static str,
    checking_updates: &'static str,
    connect_drive: &'static str,
    drive_linked: &'static str,
    out_of_sync: &'static str,
    conflict: &'static str,
    sync_all: &'static str,
    in_sync: &'static str,
    check_sync: &'static str,
    checking: &'static str,
    syncing: &'static str,
    open_emulator: &'static str,
    edit_category: &'static str,
    delete_category: &'static str,
    one_game: &'static str,
    games_suffix: &'static str,
    code: &'static str,
}

fn words(language: Language) -> Words {
    match language {
        Language::English => Words {
            safe_close: "Safe Close",
            check_updates: "Check Updates",
            update_to: "Update v10.20.300",
            restart: "Restart EmuSync",
            downloading_half: "Downloading 50%",
            checking_updates: "Checking updates…",
            connect_drive: "Connect Drive",
            drive_linked: "Drive Linked",
            out_of_sync: "Out of sync • Sync All",
            conflict: "Conflict • Sync",
            sync_all: "Sync All",
            in_sync: "In Sync",
            check_sync: "Check sync",
            checking: "Checking...",
            syncing: "Syncing...",
            open_emulator: "Open Emulator",
            edit_category: "Edit Category",
            delete_category: "Delete Category",
            one_game: "1 game",
            games_suffix: "games",
            code: "EN",
        },
        Language::Spanish => Words {
            safe_close: "Cerrar con seguridad",
            check_updates: "Buscar actualizaciones",
            update_to: "Actualizar a v10.20.300",
            restart: "Reiniciar EmuSync",
            downloading_half: "Descargando 50 %",
            checking_updates: "Buscando actualizaciones…",
            connect_drive: "Conectar Drive",
            drive_linked: "Drive conectado",
            out_of_sync: "Sin sincronizar • Sincronizar todo",
            conflict: "Conflicto • Sincronizar",
            sync_all: "Sincronizar todo",
            in_sync: "Sincronizado",
            check_sync: "Comprobar sincronización",
            checking: "Comprobando...",
            syncing: "Sincronizando...",
            open_emulator: "Abrir emulador",
            edit_category: "Editar categoría",
            delete_category: "Eliminar categoría",
            one_game: "1 juego",
            games_suffix: "juegos",
            code: "ES",
        },
    }
}

const LANGUAGES: [Language; 2] = [Language::English, Language::Spanish];

fn language_name(language: Language) -> &'static str {
    match language {
        Language::English => "English",
        Language::Spanish => "Spanish",
    }
}

/// One state of the header and the toolbar.
#[derive(Clone)]
struct Scenario {
    update: UpdateState,
    is_drive_connected: bool,
    sync: CloudSyncStatus,
    is_emulator: bool,
    name: &'static str,
    game_count: usize,
}

fn downloading_half() -> UpdateState {
    UpdateState::Downloading {
        progress: 0.5,
        info: common::update_info("10.20.300"),
    }
}

fn ready_to_restart() -> UpdateState {
    UpdateState::ReadyToRestart {
        downloaded_path: "/tmp/EmuSync.AppImage".to_string(),
        is_steam_game_mode: false,
    }
}

/// A scenario with a long category name and thousands of games.
fn scenario_with(
    update: UpdateState,
    is_drive_connected: bool,
    sync: CloudSyncStatus,
    is_emulator: bool,
) -> Scenario {
    Scenario {
        update,
        is_drive_connected,
        sync,
        is_emulator,
        name: LONG_NAME,
        game_count: 5_000,
    }
}

/// Every update state, Drive state, sync status and entry kind at least once, with the
/// longest texts and counts.
fn scenarios() -> Vec<Scenario> {
    let with_name_and_count = |mut scenario: Scenario, name, game_count| {
        scenario.name = name;
        scenario.game_count = game_count;
        scenario
    };
    vec![
        scenario_with(UpdateState::Idle, false, CloudSyncStatus::OutOfSync, true),
        with_name_and_count(
            scenario_with(
                UpdateState::Available(common::update_info("10.20.300")),
                true,
                CloudSyncStatus::Conflict,
                true,
            ),
            LONG_NAME,
            1,
        ),
        with_name_and_count(
            scenario_with(downloading_half(), false, CloudSyncStatus::InSync, true),
            LONG_NAME,
            1_234,
        ),
        with_name_and_count(
            scenario_with(ready_to_restart(), true, CloudSyncStatus::Checking, false),
            LONG_NAME,
            0,
        ),
        with_name_and_count(
            scenario_with(UpdateState::Checking, false, CloudSyncStatus::Syncing, true),
            LONG_NAME,
            99_999,
        ),
        with_name_and_count(
            scenario_with(
                UpdateState::CheckFailed("offline".to_string()),
                true,
                CloudSyncStatus::Idle,
                false,
            ),
            SHORT_NAME,
            5_000,
        ),
        scenario_with(
            UpdateState::Idle,
            true,
            CloudSyncStatus::NotConfigured,
            true,
        ),
        with_name_and_count(
            scenario_with(UpdateState::Idle, false, CloudSyncStatus::Error, true),
            LONG_NAME,
            2,
        ),
        scenario_with(
            UpdateState::DownloadFailed(DownloadFailure::Transfer("reset".to_string())),
            false,
            CloudSyncStatus::OutOfSync,
            true,
        ),
        scenario_with(
            UpdateState::RestartFailed {
                downloaded_path: "/tmp/EmuSync.AppImage".to_string(),
            },
            true,
            CloudSyncStatus::Conflict,
            true,
        ),
    ]
}

impl Scenario {
    fn entry(&self) -> GameEntry {
        if self.is_emulator {
            common::emulator_entry(self.name, "/roms", &["iso"])
        } else {
            common::native_entry(self.name)
        }
    }

    fn app(&self, language: Language) -> EmuSyncApp {
        let entry = self.entry();
        let credentials = Credentials {
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            refresh_token: self.is_drive_connected.then(|| "1//token".to_string()),
        };
        let mut config = common::config_with(vec![entry.clone()], Some(credentials));
        config.language = language;
        let mut app = EmuSyncApp::new();
        app.state.config = Some(config);
        app.state.selected_entry = Some(entry.clone());
        app.state.game_items = (0..self.game_count)
            .map(|index| GameItem::new(format!("Game {index:04}"), entry.clone(), None))
            .collect();
        app.state.update_state = self.update.clone();
        app.state.current_version = VERSION.to_string();
        app.state
            .entry_sync_status
            .insert(self.name.to_string(), self.sync);
        app
    }

    /// The text of the update item when it shows its text, and its icon otherwise.
    fn update_text_and_icon(&self, words: &Words) -> (&'static str, &'static str) {
        match self.update {
            UpdateState::Idle
            | UpdateState::CheckFailed(_)
            | UpdateState::DownloadFailed(_)
            | UpdateState::RestartFailed { .. } => (words.check_updates, glyph::UPDATE),
            UpdateState::Checking => (words.checking_updates, glyph::HOURGLASS),
            UpdateState::Available(_) => (words.update_to, glyph::UPDATE),
            UpdateState::Downloading { .. } => (words.downloading_half, glyph::DOWNLOAD),
            UpdateState::ReadyToRestart { .. } => (words.restart, glyph::RESTART),
        }
    }

    fn drive_text_and_icon(&self, words: &Words) -> (&'static str, &'static str) {
        if self.is_drive_connected {
            (words.drive_linked, glyph::CLOUD)
        } else {
            (words.connect_drive, glyph::CLOUD_SYNC)
        }
    }

    /// The label of the sync chip and the icon it keeps when the label goes to the
    /// tooltip; `None` as icon for the busy chips, whose spinner is not a text.
    fn sync_text_and_icon(&self, words: &Words) -> Option<(&'static str, Option<&'static str>)> {
        match self.sync {
            CloudSyncStatus::OutOfSync => Some((words.out_of_sync, Some(glyph::CLOUD_DOWNLOAD))),
            CloudSyncStatus::Conflict => Some((words.conflict, Some(glyph::ALERT))),
            CloudSyncStatus::Idle | CloudSyncStatus::Error => {
                Some((words.sync_all, Some(glyph::CLOUD_SYNC)))
            }
            CloudSyncStatus::InSync => Some((words.in_sync, Some(glyph::CLOUD_CHECK))),
            CloudSyncStatus::Checking => Some((words.checking, None)),
            CloudSyncStatus::Syncing => Some((words.syncing, None)),
            CloudSyncStatus::NotConfigured => None,
        }
    }

    fn game_count_text(&self, words: &Words) -> String {
        if self.game_count == 1 {
            words.one_game.to_string()
        } else {
            format!("{} {}", self.game_count, words.games_suffix)
        }
    }

    /// Texts of the header and toolbar buttons, shown whole on a wide window.
    fn button_texts(&self, words: &Words) -> Vec<&'static str> {
        let mut texts = vec![
            words.safe_close,
            self.update_text_and_icon(words).0,
            self.drive_text_and_icon(words).0,
        ];
        if let Some((label, _)) = self.sync_text_and_icon(words) {
            texts.push(label);
        }
        if self.is_emulator {
            texts.push(words.open_emulator);
        }
        texts
    }
}

/// A painted text: its string, where it is painted and the clip it is painted with.
#[derive(Clone, Debug, PartialEq)]
struct PaintedText {
    text: String,
    rect: Rect,
    clip: Rect,
}

/// What one frame painted.
struct Painted {
    texts: Vec<PaintedText>,
    /// Rectangles (chips, buttons, panels) with their clip.
    boxes: Vec<(Rect, Rect)>,
}

#[derive(Debug)]
struct Element {
    description: String,
    rect: Rect,
}

/// The painted elements of the header or the toolbar: a chip with the texts inside it, or
/// a text on its own.
struct Region {
    texts: Vec<PaintedText>,
    elements: Vec<Element>,
}

impl Painted {
    fn from_shapes(shapes: &[ClippedShape]) -> Self {
        let mut texts = Vec::new();
        let mut boxes = Vec::new();
        for clipped in shapes {
            match &clipped.shape {
                Shape::Text(text) if !text.galley.text().trim().is_empty() => {
                    texts.push(PaintedText {
                        text: text.galley.text().to_string(),
                        rect: text.visual_bounding_rect(),
                        clip: clipped.clip_rect,
                    });
                }
                Shape::Rect(rect) if rect.rect.width() > 0.0 && rect.rect.height() > 0.0 => {
                    boxes.push((rect.rect, clipped.clip_rect));
                }
                _ => {}
            }
        }
        Self { texts, boxes }
    }

    fn has_text(&self, text: &str) -> bool {
        self.texts.iter().any(|painted| painted.text == text)
    }

    fn header(&self) -> Region {
        let brand = self
            .texts
            .iter()
            .find(|painted| painted.text == "EmuSync")
            .unwrap_or_else(|| panic!("the brand is painted: {:?}", self.texts));
        // The header is one row centered on the brand, above the sidebar.
        let band_bottom = brand.rect.center().y * 2.0;
        self.region(brand.clip, |rect| rect.center().y <= band_bottom)
    }

    /// The category title above the grid (the sidebar shows the same name on its left).
    fn title(&self, name: &str) -> PaintedText {
        self.texts
            .iter()
            .filter(|painted| painted.text == name && painted.rect.center().x > SIDEBAR_WIDTH)
            .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .cloned()
            .unwrap_or_else(|| panic!("the category title `{name}` is painted: {:?}", self.texts))
    }

    fn toolbar(&self, name: &str) -> Region {
        let title = self.title(name);
        let center_y = title.rect.center().y;
        self.region(title.clip, |rect| {
            (rect.center().y - center_y).abs() <= 24.0
        })
    }

    /// Texts and rectangles painted with `clip` inside the band, grouped into elements.
    fn region(&self, clip: Rect, in_band: impl Fn(Rect) -> bool) -> Region {
        let texts: Vec<PaintedText> = self
            .texts
            .iter()
            .filter(|painted| painted.clip == clip && in_band(painted.rect))
            .cloned()
            .collect();
        // Chips are row-sized; the panel backgrounds are taller.
        let mut chips: Vec<Rect> = Vec::new();
        for (rect, _) in self
            .boxes
            .iter()
            .filter(|(rect, box_clip)| *box_clip == clip && rect.height() < 60.0 && in_band(*rect))
        {
            if !chips.contains(rect) {
                chips.push(*rect);
            }
        }
        // A hover overlay or an inner rectangle is part of the chip around it.
        let outer: Vec<Rect> = chips
            .iter()
            .copied()
            .filter(|candidate| {
                !chips
                    .iter()
                    .any(|other| other != candidate && other.contains_rect(*candidate))
            })
            .collect();

        let mut elements = Vec::new();
        for chip in &outer {
            let inside: Vec<&PaintedText> = texts
                .iter()
                .filter(|painted| chip.contains(painted.rect.center()))
                .collect();
            for painted in &inside {
                assert!(
                    chip.expand(1.0).contains_rect(painted.rect),
                    "`{}` at {:?} spills out of its chip {chip:?}",
                    painted.text,
                    painted.rect
                );
            }
            let contents: Vec<&str> = inside.iter().map(|painted| painted.text.as_str()).collect();
            elements.push(Element {
                description: format!("chip [{}]", contents.join(" ")),
                rect: *chip,
            });
        }
        for painted in &texts {
            if !outer
                .iter()
                .any(|chip| chip.contains(painted.rect.center()))
            {
                elements.push(Element {
                    description: format!("text `{}`", painted.text),
                    rect: painted.rect,
                });
            }
        }
        Region { texts, elements }
    }
}

impl Region {
    fn has_text(&self, text: &str) -> bool {
        self.texts.iter().any(|painted| painted.text == text)
    }

    fn center_of(&self, text: &str) -> Option<Pos2> {
        self.texts
            .iter()
            .find(|painted| painted.text == text)
            .map(|painted| painted.rect.center())
    }

    /// The center of the chip without any text: the busy chip once its label went to the
    /// tooltip (its spinner is not a text).
    fn textless_chip_center(&self) -> Option<Pos2> {
        self.elements
            .iter()
            .find(|element| element.description == "chip []")
            .map(|element| element.rect.center())
    }

    /// No two elements overlap, and every one lies between `left` and `right`.
    fn assert_apart_and_within(&self, left: f32, right: f32, context: &str) {
        for (index, first) in self.elements.iter().enumerate() {
            assert!(
                first.rect.left() >= left - OVERLAP_TOLERANCE
                    && first.rect.right() <= right + OVERLAP_TOLERANCE,
                "{context}: {} at {:?} leaves {left}..{right}",
                first.description,
                first.rect
            );
            for second in &self.elements[index + 1..] {
                let overlap = first.rect.intersect(second.rect);
                assert!(
                    overlap.width() <= OVERLAP_TOLERANCE || overlap.height() <= OVERLAP_TOLERANCE,
                    "{context}: {} at {:?} overlaps {} at {:?}",
                    first.description,
                    first.rect,
                    second.description,
                    second.rect
                );
            }
        }
    }
}

/// A headless window of the app with the application fonts.
struct Window {
    ctx: Context,
    app: EmuSyncApp,
    width: f32,
    pixels_per_point: f32,
    time_seconds: f64,
}

impl Window {
    fn new(app: EmuSyncApp, width: f32) -> Self {
        Self::scaled(app, width, 1.0)
    }

    fn scaled(app: EmuSyncApp, width: f32, pixels_per_point: f32) -> Self {
        let ctx = Context::default();
        configure_egui_theme(&ctx);
        Self {
            ctx,
            app,
            width,
            pixels_per_point,
            time_seconds: 0.0,
        }
    }

    fn frame_after(&mut self, seconds: f64, events: Vec<Event>) -> Painted {
        self.time_seconds += seconds;
        let mut input = RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                vec2(self.width, MIN_WINDOW_HEIGHT),
            )),
            time: Some(self.time_seconds),
            events,
            ..RawInput::default()
        };
        input
            .viewports
            .entry(ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(self.pixels_per_point);
        let app = &mut self.app;
        let output = self.ctx.run_ui(input, |ui| app.update(ui));
        let painted = Painted::from_shapes(&output.shapes);
        output.drop_without_applying_deltas();
        painted
    }

    fn frame(&mut self, events: Vec<Event>) -> Painted {
        self.frame_after(1.0 / 60.0, events)
    }

    /// Frames until the painted texts stop moving; returns the last one.
    fn settle(&mut self) -> Painted {
        let mut previous: Option<Vec<PaintedText>> = None;
        let mut stable_frames = 0;
        for _ in 0..30 {
            let painted = self.frame(Vec::new());
            stable_frames = if previous.as_ref() == Some(&painted.texts) {
                stable_frames + 1
            } else {
                0
            };
            if stable_frames >= 2 {
                return painted;
            }
            previous = Some(painted.texts.clone());
        }
        panic!("the view never settled at width {}", self.width);
    }

    fn resize(&mut self, width: f32) -> Painted {
        self.width = width;
        self.settle()
    }

    /// Clicks `pos` and returns the actions the click published.
    fn click(&mut self, pos: Pos2) -> Vec<UiAction> {
        self.app.take_actions();
        let button = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(pos)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.frame(Vec::new());
        self.app.take_actions()
    }

    /// Rests the pointer on `pos` long enough for a tooltip and returns every text painted
    /// meanwhile.
    fn hover_texts(&mut self, pos: Pos2) -> Vec<String> {
        let mut texts: Vec<String> = self
            .frame(vec![Event::PointerMoved(pos)])
            .texts
            .into_iter()
            .map(|painted| painted.text)
            .collect();
        for _ in 0..4 {
            texts.extend(
                self.frame_after(1.0, Vec::new())
                    .texts
                    .into_iter()
                    .map(|painted| painted.text),
            );
        }
        texts
    }
}

/// Checks the header and the toolbar of one settled frame: nothing overlaps, nothing leaves
/// the window or goes under the sidebar, and every control is still painted.
fn assert_header_and_toolbar_fit(
    painted: &Painted,
    scenario: &Scenario,
    language: Language,
    width: f32,
    context: &str,
) {
    let words = words(language);
    let header = painted.header();
    header.assert_apart_and_within(0.0, width, &format!("{context} header"));
    assert!(header.has_text(words.code), "{context}: language chip");
    let (update_text, update_icon) = scenario.update_text_and_icon(&words);
    let (drive_text, drive_icon) = scenario.drive_text_and_icon(&words);
    for (what, text, icon) in [
        ("safe close", words.safe_close, glyph::POWER),
        ("update", update_text, update_icon),
        ("drive", drive_text, drive_icon),
    ] {
        assert!(
            header.has_text(text) || header.has_text(icon),
            "{context}: the {what} button is not painted in the header"
        );
    }

    let toolbar = painted.toolbar(scenario.name);
    toolbar.assert_apart_and_within(SIDEBAR_WIDTH, width, &format!("{context} toolbar"));
    for (what, icon) in [("edit", glyph::PENCIL), ("delete", glyph::DELETE)] {
        assert!(
            toolbar.has_text(icon),
            "{context}: the {what} button is not painted"
        );
    }
    if let Some((label, icon)) = scenario.sync_text_and_icon(&words) {
        let is_shown = toolbar.has_text(label)
            || match icon {
                Some(icon) => toolbar.has_text(icon),
                None => toolbar.textless_chip_center().is_some(),
            };
        assert!(is_shown, "{context}: the sync chip is not painted");
    }
    if scenario.is_emulator {
        assert!(
            toolbar.has_text(words.open_emulator) || toolbar.has_text(glyph::COG),
            "{context}: the open-emulator button is not painted"
        );
    }
    if scenario.name == LONG_NAME {
        let title = painted.title(scenario.name);
        assert!(
            title.rect.width() >= MIN_VISIBLE_TITLE,
            "{context}: only {} points of the title are visible",
            title.rect.width()
        );
    }
}

#[test]
fn header_and_toolbar_never_overlap_or_leave_the_window_at_any_width_from_the_minimum() {
    // Widening and narrowing in turns, the minimum first and between two widths.
    let widths = [
        1280.0, 600.0, 1600.0, 615.0, 1080.0, 650.0, 1000.0, 700.0, 900.0, 760.0, 820.0, 600.5,
        1180.0, 1920.0, 640.3,
    ];
    for scenario in scenarios() {
        for language in LANGUAGES {
            let mut window = Window::new(scenario.app(language), 1280.0);
            for width in widths {
                let painted = window.resize(width);
                let context = format!(
                    "{} / {:?} / {:?} / {} at {width}",
                    language_name(language),
                    scenario.update,
                    scenario.sync,
                    scenario.name
                );
                assert_header_and_toolbar_fit(&painted, &scenario, language, width, &context);
            }
        }
    }
}

#[test]
fn header_and_toolbar_fit_from_the_minimum_width_with_fractional_display_scales() {
    for pixels_per_point in [1.25, 1.5, 1.75] {
        for scenario in scenarios() {
            for language in LANGUAGES {
                let mut window =
                    Window::scaled(scenario.app(language), MIN_WINDOW_WIDTH, pixels_per_point);
                for width in [600.0, 607.0, 640.0, 731.0, 1080.0] {
                    let painted = window.resize(width);
                    let context = format!(
                        "{} / {:?} / {:?} at {width} × {pixels_per_point}",
                        language_name(language),
                        scenario.update,
                        scenario.sync
                    );
                    assert_header_and_toolbar_fit(&painted, &scenario, language, width, &context);
                }
            }
        }
    }
}

#[test]
fn a_wide_window_shows_every_button_with_its_whole_text_and_the_version() {
    for scenario in scenarios() {
        for language in LANGUAGES {
            let words = words(language);
            let mut window = Window::new(scenario.app(language), 1280.0);
            let painted = window.settle();
            let context = format!(
                "{} / {:?} / {:?}",
                language_name(language),
                scenario.update,
                scenario.sync
            );

            let header = painted.header();
            let toolbar = painted.toolbar(scenario.name);
            for text in scenario.button_texts(&words) {
                assert!(
                    header.has_text(text) || toolbar.has_text(text),
                    "{context}: `{text}` is not shown whole"
                );
            }
            assert!(header.has_text(VERSION_LABEL), "{context}: version");
            assert!(
                toolbar.has_text(&scenario.game_count_text(&words)),
                "{context}: game count"
            );
            // The close icon belongs only to the narrow form.
            assert!(!header.has_text(glyph::POWER), "{context}: close icon");
        }
    }
}

#[test]
fn at_the_minimum_width_buttons_show_their_icon_and_their_text_only_on_hover() {
    for scenario in scenarios() {
        for language in LANGUAGES {
            let words = words(language);
            let mut window = Window::new(scenario.app(language), MIN_WINDOW_WIDTH);
            let painted = window.settle();
            let context = format!(
                "{} / {:?} / {:?}",
                language_name(language),
                scenario.update,
                scenario.sync
            );
            let header = painted.header();
            let toolbar = painted.toolbar(scenario.name);

            for text in scenario.button_texts(&words) {
                assert!(
                    !painted.has_text(text),
                    "{context}: `{text}` is still shown whole"
                );
            }
            assert!(!painted.has_text(VERSION_LABEL), "{context}: version");

            let (update_text, update_icon) = scenario.update_text_and_icon(&words);
            let (drive_text, drive_icon) = scenario.drive_text_and_icon(&words);
            let mut hovers: Vec<(Pos2, &str)> = vec![
                (header.center_of("EmuSync").expect("brand"), VERSION_LABEL),
                (
                    header.center_of(glyph::POWER).expect("close icon"),
                    words.safe_close,
                ),
                (
                    header.center_of(update_icon).expect("update icon"),
                    update_text,
                ),
                (
                    header.center_of(drive_icon).expect("drive icon"),
                    drive_text,
                ),
                (
                    toolbar.center_of(glyph::PENCIL).expect("edit icon"),
                    words.edit_category,
                ),
                (
                    toolbar.center_of(glyph::DELETE).expect("delete icon"),
                    words.delete_category,
                ),
            ];
            if scenario.is_emulator {
                hovers.push((
                    toolbar.center_of(glyph::COG).expect("open-emulator icon"),
                    words.open_emulator,
                ));
            }
            match scenario.sync_text_and_icon(&words) {
                Some((label, Some(icon))) => {
                    hovers.push((toolbar.center_of(icon).expect("sync icon"), label));
                }
                Some((label, None)) => {
                    hovers.push((toolbar.textless_chip_center().expect("busy chip"), label));
                }
                None => {}
            }
            if scenario.sync == CloudSyncStatus::InSync {
                hovers.push((
                    toolbar.center_of(glyph::REFRESH).expect("refresh icon"),
                    words.check_sync,
                ));
            }

            for (pos, tooltip) in hovers {
                let texts = window.hover_texts(pos);
                assert!(
                    texts.iter().any(|text| text == tooltip),
                    "{context}: hovering {pos:?} does not show `{tooltip}`"
                );
                // Back to an empty spot so the next tooltip starts afresh.
                window.hover_texts(Pos2::new(SIDEBAR_WIDTH / 2.0, MIN_WINDOW_HEIGHT - 4.0));
            }
        }
    }
}

#[test]
fn at_the_minimum_width_the_grid_shows_one_column_beside_the_sidebar() {
    for language in LANGUAGES {
        let name = language_name(language);
        let scenario = scenario_with(UpdateState::Idle, false, CloudSyncStatus::OutOfSync, true);
        let mut window = Window::new(scenario.app(language), MIN_WINDOW_WIDTH);
        let painted = window.settle();

        let cards: Vec<&PaintedText> = painted
            .texts
            .iter()
            .filter(|painted| painted.text.starts_with("Game "))
            .collect();
        assert!(cards.len() >= 2, "{name}: {cards:?}");
        let first_left = cards[0].rect.left();
        for card in &cards {
            assert!(
                (card.rect.left() - first_left).abs() < 1.0,
                "{name}: more than one column: {cards:?}"
            );
            assert!(
                card.rect.left() >= SIDEBAR_WIDTH && card.rect.right() <= MIN_WINDOW_WIDTH,
                "{name}: {} at {:?}",
                card.text,
                card.rect
            );
        }
        let title = painted.title(LONG_NAME);
        assert!(
            title.rect.width() >= MIN_VISIBLE_TITLE,
            "{name}: title {} points",
            title.rect.width()
        );
    }
}

/// A settled window of `scenario` at the minimum width, and its header and toolbar.
fn narrow(scenario: &Scenario, language: Language) -> (Window, Region, Region) {
    let mut window = Window::new(scenario.app(language), MIN_WINDOW_WIDTH);
    let painted = window.settle();
    let header = painted.header();
    let toolbar = painted.toolbar(scenario.name);
    (window, header, toolbar)
}

#[test]
fn header_icons_at_the_minimum_width_keep_their_actions() {
    for language in LANGUAGES {
        let name = language_name(language);
        let idle = scenario_with(UpdateState::Idle, false, CloudSyncStatus::Idle, true);

        let (mut window, header, _) = narrow(&idle, language);
        let actions = window.click(header.center_of(glyph::POWER).expect("close icon"));
        assert_eq!(actions, vec![UiAction::Exit], "{name}: safe close");

        let (mut window, header, _) = narrow(&idle, language);
        let actions = window.click(header.center_of(glyph::UPDATE).expect("check icon"));
        assert_eq!(
            actions,
            vec![UiAction::CheckForUpdates { manual: true }],
            "{name}: check updates"
        );

        let (mut window, header, _) = narrow(&idle, language);
        let actions = window.click(header.center_of(glyph::CLOUD_SYNC).expect("connect icon"));
        assert_eq!(
            actions,
            vec![UiAction::ConnectDrive {
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
            }],
            "{name}: connect drive"
        );
        assert_eq!(window.app.active_dialog(), &ActiveDialog::None, "{name}");

        // A blank client secret opens the setup dialog instead.
        let mut app = idle.app(language);
        if let Some(credentials) = app
            .state
            .config
            .as_mut()
            .and_then(|config| config.credentials.as_mut())
        {
            credentials.client_secret = "  ".to_string();
        }
        let mut window = Window::new(app, MIN_WINDOW_WIDTH);
        let header = window.settle().header();
        let actions = window.click(header.center_of(glyph::CLOUD_SYNC).expect("connect icon"));
        assert_eq!(actions, Vec::<UiAction>::new(), "{name}: blank secret");
        assert!(
            matches!(window.app.active_dialog(), ActiveDialog::DriveSetup(_)),
            "{name}: a blank secret opens the setup"
        );

        let linked = scenario_with(UpdateState::Idle, true, CloudSyncStatus::Idle, true);
        let (mut window, header, _) = narrow(&linked, language);
        let actions = window.click(header.center_of(glyph::CLOUD).expect("linked icon"));
        assert_eq!(actions, Vec::<UiAction>::new(), "{name}: drive linked");
        assert!(
            matches!(window.app.active_dialog(), ActiveDialog::DriveSetup(_)),
            "{name}: drive linked opens the setup"
        );

        for (update, icon) in [
            (
                UpdateState::Available(common::update_info("10.20.300")),
                glyph::UPDATE,
            ),
            (downloading_half(), glyph::DOWNLOAD),
            (ready_to_restart(), glyph::RESTART),
        ] {
            let scenario = scenario_with(update.clone(), true, CloudSyncStatus::Idle, true);
            let (mut window, header, _) = narrow(&scenario, language);
            let actions = window.click(header.center_of(icon).expect("update chip icon"));
            assert_eq!(actions, Vec::<UiAction>::new(), "{name}: {update:?}");
            assert_eq!(
                window.app.active_dialog(),
                &ActiveDialog::Update,
                "{name}: {update:?} opens the update dialog"
            );
        }

        let checking = scenario_with(UpdateState::Checking, true, CloudSyncStatus::Idle, true);
        let (mut window, header, _) = narrow(&checking, language);
        let actions = window.click(header.center_of(glyph::HOURGLASS).expect("checking icon"));
        assert_eq!(actions, Vec::<UiAction>::new(), "{name}: checking");
        assert_eq!(window.app.active_dialog(), &ActiveDialog::None, "{name}");
    }
}

#[test]
fn the_language_chip_at_the_minimum_width_still_opens_its_list() {
    for (language, other, other_name) in [
        (Language::English, Language::Spanish, "Español"),
        (Language::Spanish, Language::English, "English"),
    ] {
        let scenario = scenario_with(UpdateState::Idle, false, CloudSyncStatus::OutOfSync, true);
        let (mut window, header, _) = narrow(&scenario, language);
        let code = words(language).code;
        window.click(header.center_of(code).expect("language chip"));

        let painted = window.settle();
        let entry = painted
            .texts
            .iter()
            .find(|painted| painted.text == other_name)
            .unwrap_or_else(|| panic!("{code}: the list does not open"))
            .clone();
        assert!(
            entry.rect.left() >= 0.0 && entry.rect.right() <= MIN_WINDOW_WIDTH,
            "{code}: the list leaves the window at {:?}",
            entry.rect
        );
        let actions = window.click(entry.rect.center());
        assert_eq!(
            actions
                .into_iter()
                .filter(|action| matches!(action, UiAction::SetLanguage(_)))
                .collect::<Vec<_>>(),
            vec![UiAction::SetLanguage(other)],
            "{code}"
        );
    }
}

#[test]
fn toolbar_icons_at_the_minimum_width_keep_their_actions() {
    for language in LANGUAGES {
        let name = language_name(language);
        for (sync, icon) in [
            (CloudSyncStatus::OutOfSync, glyph::CLOUD_DOWNLOAD),
            (CloudSyncStatus::Conflict, glyph::ALERT),
            (CloudSyncStatus::Idle, glyph::CLOUD_SYNC),
            (CloudSyncStatus::Error, glyph::CLOUD_SYNC),
        ] {
            let scenario = scenario_with(UpdateState::Idle, true, sync, true);
            let (mut window, _, toolbar) = narrow(&scenario, language);
            let actions = window.click(toolbar.center_of(icon).expect("sync icon"));
            assert_eq!(
                actions,
                vec![UiAction::SyncEntry(scenario.entry())],
                "{name}: {sync:?}"
            );
        }

        let in_sync = scenario_with(UpdateState::Idle, true, CloudSyncStatus::InSync, true);
        let (mut window, _, toolbar) = narrow(&in_sync, language);
        let actions = window.click(toolbar.center_of(glyph::REFRESH).expect("refresh icon"));
        assert_eq!(
            actions,
            vec![UiAction::CheckSyncEntry(in_sync.entry())],
            "{name}: refresh"
        );
        let (mut window, _, toolbar) = narrow(&in_sync, language);
        let actions = window.click(toolbar.center_of(glyph::CLOUD_CHECK).expect("status icon"));
        assert_eq!(actions, Vec::<UiAction>::new(), "{name}: in-sync icon");

        for busy in [CloudSyncStatus::Checking, CloudSyncStatus::Syncing] {
            let scenario = scenario_with(UpdateState::Idle, true, busy, true);
            let (mut window, _, toolbar) = narrow(&scenario, language);
            let actions = window.click(toolbar.textless_chip_center().expect("busy chip"));
            assert_eq!(actions, Vec::<UiAction>::new(), "{name}: {busy:?}");
        }

        let emulator = scenario_with(UpdateState::Idle, true, CloudSyncStatus::Idle, true);
        let GameEntry::Emulator(expected_emulator) = emulator.entry() else {
            unreachable!("an emulator scenario");
        };
        let (mut window, _, toolbar) = narrow(&emulator, language);
        let actions = window.click(toolbar.center_of(glyph::COG).expect("open icon"));
        assert_eq!(
            actions,
            vec![UiAction::OpenEmulator(expected_emulator)],
            "{name}: open emulator"
        );

        let (mut window, _, toolbar) = narrow(&emulator, language);
        let actions = window.click(toolbar.center_of(glyph::PENCIL).expect("edit icon"));
        assert_eq!(actions, Vec::<UiAction>::new(), "{name}: edit");
        assert!(
            matches!(window.app.active_dialog(), ActiveDialog::Entry(_)),
            "{name}: edit opens the category form"
        );

        let (mut window, _, toolbar) = narrow(&emulator, language);
        let actions = window.click(toolbar.center_of(glyph::DELETE).expect("delete icon"));
        assert_eq!(actions, Vec::<UiAction>::new(), "{name}: delete");
        assert_eq!(
            window.app.active_dialog(),
            &ActiveDialog::ConfirmDelete(Box::new(emulator.entry())),
            "{name}: delete asks for confirmation"
        );
    }
}

#[test]
fn clicking_the_in_sync_label_or_icon_on_a_wide_window_does_nothing() {
    for language in LANGUAGES {
        let words = words(language);
        let scenario = scenario_with(UpdateState::Idle, true, CloudSyncStatus::InSync, true);
        for target in [words.in_sync, glyph::CLOUD_CHECK] {
            let mut window = Window::new(scenario.app(language), 1280.0);
            let toolbar = window.settle().toolbar(scenario.name);
            let actions = window.click(toolbar.center_of(target).expect("in-sync chip"));
            assert_eq!(
                actions,
                Vec::<UiAction>::new(),
                "{}: {target}",
                language_name(language)
            );
        }
    }
}

#[test]
fn wide_buttons_with_their_text_keep_their_actions() {
    for language in LANGUAGES {
        let name = language_name(language);
        let words = words(language);

        let emulator = scenario_with(UpdateState::Idle, true, CloudSyncStatus::Idle, true);
        let GameEntry::Emulator(expected_emulator) = emulator.entry() else {
            unreachable!("an emulator scenario");
        };
        let mut window = Window::new(emulator.app(language), 1280.0);
        let toolbar = window.settle().toolbar(emulator.name);
        let actions = window.click(toolbar.center_of(words.open_emulator).expect("open button"));
        assert_eq!(
            actions,
            vec![UiAction::OpenEmulator(expected_emulator)],
            "{name}: open emulator"
        );

        for (update, text) in [
            (
                UpdateState::Available(common::update_info("10.20.300")),
                words.update_to,
            ),
            (downloading_half(), words.downloading_half),
            (ready_to_restart(), words.restart),
        ] {
            let scenario = scenario_with(update.clone(), true, CloudSyncStatus::Idle, true);
            let mut window = Window::new(scenario.app(language), 1280.0);
            let header = window.settle().header();
            let actions = window.click(header.center_of(text).expect("update chip"));
            assert_eq!(actions, Vec::<UiAction>::new(), "{name}: {update:?}");
            assert_eq!(
                window.app.active_dialog(),
                &ActiveDialog::Update,
                "{name}: {update:?} opens the update dialog"
            );
        }
    }
}
