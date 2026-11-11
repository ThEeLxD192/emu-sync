//! Headless egui helpers and fixtures shared by the presentation suites.
#![allow(dead_code)]

use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, vec2};
use emusync_domain::{
    config::{AppConfig, Credentials},
    library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType},
    update::UpdateInfo,
};
use emusync_presentation::{
    EmuSyncApp, GameItem, SaveSetupRequest, UiAction,
    model::{PathTarget, PickMode},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

/// Consecutive frames a position must stay unchanged to count as settled.
const STABLE_FRAMES: u32 = 4;

pub fn screen() -> Rect {
    Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))
}

pub fn escape() -> Vec<Event> {
    vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]
}

/// Frames that move the pointer to `pos`, press and release the primary button.
pub fn click_at(pos: Pos2) -> Vec<Vec<Event>> {
    let button = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    vec![
        vec![Event::PointerMoved(pos)],
        vec![button(true)],
        vec![button(false)],
        Vec::new(),
    ]
}

pub fn run_frames(ctx: &Context, app: &mut EmuSyncApp, frames: Vec<Vec<Event>>) {
    for events in frames {
        let input = RawInput {
            screen_rect: Some(screen()),
            events,
            ..RawInput::default()
        };
        let output = ctx.run_ui(input, |ui| app.update(ui));
        output.drop_without_applying_deltas();
    }
}

/// Two frames with the pointer resting on `pos`.
pub fn hover(ctx: &Context, app: &mut EmuSyncApp, pos: Pos2) {
    run_frames(
        ctx,
        app,
        vec![
            vec![Event::PointerMoved(pos)],
            vec![Event::PointerMoved(pos)],
        ],
    );
}

/// Runs one frame and returns the centers of every text shape for which `matches`
/// holds.
fn text_centers(ctx: &Context, app: &mut EmuSyncApp, matches: impl Fn(&str) -> bool) -> Vec<Pos2> {
    let input = RawInput {
        screen_rect: Some(screen()),
        ..RawInput::default()
    };
    let output = ctx.run_ui(input, |ui| app.update(ui));
    let centers = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(text) if matches(text.galley.text()) => {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    centers
}

pub fn text_center(ctx: &Context, app: &mut EmuSyncApp, label: &str) -> Option<Pos2> {
    text_centers(ctx, app, |text| text == label)
        .into_iter()
        .next()
}

/// Every position where `glyph` is painted in one frame.
pub fn find_glyph_centers(ctx: &Context, app: &mut EmuSyncApp, glyph: &str) -> Vec<Pos2> {
    text_centers(ctx, app, |text| text == glyph)
}

/// Every position of the text `label` in one frame (a title may appear twice).
pub fn all_text_centers(ctx: &Context, app: &mut EmuSyncApp, label: &str) -> Vec<Pos2> {
    text_centers(ctx, app, |text| text == label)
}

pub fn text_exists(ctx: &Context, app: &mut EmuSyncApp, label: &str) -> bool {
    text_center(ctx, app, label).is_some()
}

/// Center of the text `label` once the layout has settled (dialogs move over their
/// first frames while they size themselves).
pub fn center_of_text(ctx: &Context, app: &mut EmuSyncApp, label: &str) -> Pos2 {
    let mut previous = None;
    let mut stable = 0;
    for _ in 0..30 {
        let current = text_center(ctx, app, label);
        stable = if current.is_some() && current == previous {
            stable + 1
        } else {
            0
        };
        if stable >= STABLE_FRAMES {
            return current.unwrap_or_default();
        }
        previous = current;
    }
    panic!("`{label}` never settled");
}

pub fn click_button(ctx: &Context, app: &mut EmuSyncApp, label: &str) {
    let pos = center_of_text(ctx, app, label);
    run_frames(ctx, app, click_at(pos));
}

pub fn native_entry(name: &str) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: format!("/games/{}", name.to_lowercase().replace(' ', "_")),
            save_targets: vec![SaveTarget {
                path: format!("/saves/{name}"),
                kind: SaveTargetType::Directory,
                cloud_path: None,
                excluded_folders: Vec::new(),
            }],
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    })
}

pub fn emulator_entry(name: &str, roms_dir: &str, exts: &[&str]) -> GameEntry {
    GameEntry::Emulator(Emulator::new(
        name,
        format!("/usr/bin/{}", name.to_lowercase()),
        roms_dir,
        exts.iter().map(|e| e.to_string()).collect(),
    ))
}

pub fn setup_request(id: u64, cloud_files: &[&str]) -> SaveSetupRequest {
    let entry = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "SNES".to_string(),
            ..EntryCommon::default()
        },
        ..Emulator::default()
    });
    SaveSetupRequest {
        id,
        item: GameItem::new("smw.sfc", entry, None),
        notice: Some("Configure saves before play".to_string()),
        cloud_file_names: cloud_files.iter().map(|f| f.to_string()).collect(),
        targets: Vec::new(),
    }
}

pub fn update_info(version: &str) -> UpdateInfo {
    UpdateInfo {
        version: version.to_string(),
        tag_name: format!("v{version}"),
        release_notes: "Fixes and improvements".to_string(),
        download_url: "https://example.com/app.AppImage".to_string(),
        asset_size: 1024,
        asset_name: "EmuSync.AppImage".to_string(),
    }
}

pub fn config_with(entries: Vec<GameEntry>, credentials: Option<Credentials>) -> AppConfig {
    AppConfig {
        credentials,
        entries,
    }
}

/// Unique temporary directory removed on drop, even if an assertion fails.
pub struct TempDirGuard {
    pub path: PathBuf,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

impl TempDirGuard {
    pub fn new(test_name: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "emusync-{}-{unique}-{test_name}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("temp dir");
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Center of the widget `id` once its layout stopped moving (dialogs re-center over
/// their first frames while they size themselves).
pub fn settled_center_of(ctx: &Context, app: &mut EmuSyncApp, id: egui::Id) -> Pos2 {
    let mut previous = None;
    let mut stable = 0;
    for _ in 0..30 {
        run_frames(ctx, app, vec![Vec::new()]);
        let current = ctx.read_response(id).map(|response| response.rect.center());
        stable = if current.is_some() && current == previous {
            stable + 1
        } else {
            0
        };
        if stable >= STABLE_FRAMES {
            return current.unwrap_or_default();
        }
        previous = current;
    }
    panic!("widget never settled");
}

/// The only picker request in `actions`, as `(request_id, target, mode)`.
pub fn pick_request(actions: Vec<UiAction>) -> (u64, PathTarget, PickMode) {
    match actions.as_slice() {
        [
            UiAction::PickPath {
                request_id,
                target,
                mode,
                ..
            },
        ] => (*request_id, target.clone(), *mode),
        other => panic!("expected exactly one PickPath, got {other:?}"),
    }
}

/// The only restore preview request in `actions`, as `(request_id, destination, names)`.
pub fn preview_request(actions: Vec<UiAction>) -> (u64, String, Vec<String>) {
    match actions.as_slice() {
        [
            UiAction::PreviewRestorePaths {
                request_id,
                destination,
                names,
            },
        ] => (*request_id, destination.clone(), names.clone()),
        other => panic!("expected exactly one PreviewRestorePaths, got {other:?}"),
    }
}

/// Waits for the layout of `label` to settle, then clicks the `nth` (0-based, top to bottom,
/// then left to right) occurrence of the button `label` and returns the actions the
/// click published. Actions queued before the click are discarded.
pub fn click_nth(ctx: &Context, app: &mut EmuSyncApp, label: &str, nth: usize) -> Vec<UiAction> {
    let mut centers = Vec::new();
    let mut stable = 0;
    for _ in 0..30 {
        let mut current = all_text_centers(ctx, app, label);
        current.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        stable = if !current.is_empty() && current == centers {
            stable + 1
        } else {
            0
        };
        centers = current;
        if stable >= STABLE_FRAMES {
            break;
        }
    }
    let pos = *centers
        .get(nth)
        .unwrap_or_else(|| panic!("no `{label}` button number {nth}"));
    app.take_actions();
    run_frames(ctx, app, click_at(pos));
    app.take_actions()
}

/// Scrolls the open dialog's content to its end; buttons near the bottom of a tall
/// dialog sit under the footer until then.
pub fn scroll_dialog_to_end(ctx: &Context, app: &mut EmuSyncApp) {
    for _ in 0..3 {
        run_frames(
            ctx,
            app,
            vec![vec![
                Event::PointerMoved(Pos2::new(640.0, 400.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, -300.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: Modifiers::NONE,
                },
            ]],
        );
    }
}

/// Clicks the Folder button of the restore destination and answers it with `path`,
/// then runs a frame and returns the actions that follow (the preview request).
pub fn choose_restore_destination(
    ctx: &Context,
    app: &mut EmuSyncApp,
    path: &str,
) -> Vec<UiAction> {
    scroll_dialog_to_end(ctx, app);
    let (id, target, mode) = pick_request(click_nth(ctx, app, "Folder", 0));
    assert_eq!(target, PathTarget::RestoreDestination);
    app.apply_picked_path(id, target, mode, path.to_string());
    run_frames(ctx, app, vec![Vec::new()]);
    app.take_actions()
}
