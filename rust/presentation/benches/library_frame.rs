//! UI-thread cost of the game library: one settled frame, the time until the visible
//! covers are ready, and a scroll through covers that are not loaded yet.
//!
//! Run with `cargo bench -p emusync-presentation --bench library_frame`; compare
//! builds with `-- --save-baseline <name>` and `-- --baseline <name>`.

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use egui::{
    Context, Event, Modifiers, MouseWheelSource, MouseWheelUnit, Pos2, RawInput, Rect, TouchPhase,
    pos2, vec2,
};
use emusync_domain::library::{Emulator, GameEntry};
use emusync_presentation::{EmuSyncApp, GameItem};
use image::{ImageFormat, RgbImage};
use std::{
    hint::black_box,
    path::{Path, PathBuf},
    time::Duration,
};

const GAMES: usize = 200;
/// Frames a cold load may take before the benchmark gives up on it.
const MAX_FRAMES: usize = 2_000;

fn screen() -> Rect {
    Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))
}

fn frame(ctx: &Context, app: &mut EmuSyncApp, events: Vec<Event>) {
    let input = RawInput {
        screen_rect: Some(screen()),
        events,
        ..RawInput::default()
    };
    let output = ctx.run_ui(input, |ui| app.update(ui));
    black_box(output.shapes.len());
    output.drop_without_applying_deltas();
}

/// A noisy gradient: decodes like a photo, unlike a flat image.
fn cover_image(width: u32, height: u32, seed: u32) -> RgbImage {
    let mut state = seed.wrapping_mul(2_654_435_761).max(1);
    RgbImage::from_fn(width, height, |x, y| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let noise = (state & 0x3f) as u8;
        image::Rgb([
            ((x * 255 / width) as u8).wrapping_add(noise),
            ((y * 255 / height) as u8).wrapping_add(noise / 2),
            (seed as u8).wrapping_mul(37).wrapping_add(noise),
        ])
    })
}

/// Writes the covers: most are 600x900 JPEG, as scraped box art; every fourth is a
/// large PNG, as a cover the user picked.
fn write_covers(dir: &Path) -> Vec<PathBuf> {
    (0..GAMES)
        .map(|index| {
            let large = index % 4 == 3;
            let (path, format, size) = if large {
                (
                    dir.join(format!("{index}.png")),
                    ImageFormat::Png,
                    (1500, 2250),
                )
            } else {
                (
                    dir.join(format!("{index}.jpg")),
                    ImageFormat::Jpeg,
                    (600, 900),
                )
            };
            cover_image(size.0, size.1, index as u32)
                .save_with_format(&path, format)
                .expect("write cover");
            path
        })
        .collect()
}

fn library(covers: &[PathBuf]) -> EmuSyncApp {
    let entry = GameEntry::Emulator(Emulator::new(
        "Bench",
        "/usr/bin/bench",
        "/roms",
        vec!["iso".into()],
    ));
    let mut app = EmuSyncApp::new();
    app.state.selected_entry = Some(entry.clone());
    app.state.game_items = covers
        .iter()
        .enumerate()
        .map(|(index, cover)| {
            GameItem::new(format!("Game {index:03}"), entry.clone(), None)
                .with_cover_path(Some(cover.to_string_lossy().into_owned()))
        })
        .collect();
    app
}

/// Draws frames until the cache holds `expected` covers.
fn load_visible(ctx: &Context, app: &mut EmuSyncApp, expected: usize) {
    for _ in 0..MAX_FRAMES {
        frame(ctx, app, Vec::new());
        if app.cover_cache.loaded_count() >= expected {
            return;
        }
        std::thread::yield_now();
    }
    panic!("covers did not load within {MAX_FRAMES} frames");
}

/// Covers visible on the first screen, counted once the cache has settled.
fn visible_covers(covers: &[PathBuf]) -> usize {
    let ctx = Context::default();
    let mut app = library(covers);
    let mut loaded = 0;
    let mut settled = 0;
    while settled < 20 {
        frame(&ctx, &mut app, Vec::new());
        std::thread::sleep(Duration::from_millis(5));
        let now = app.cover_cache.loaded_count();
        settled = if now == loaded { settled + 1 } else { 0 };
        loaded = now;
    }
    assert!(loaded > 0, "no cover became visible");
    loaded
}

fn scroll_events(delta: f32) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos2(800.0, 500.0)),
        Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, -delta),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Unknown,
            modifiers: Modifiers::default(),
        },
    ]
}

fn library_frame(c: &mut Criterion) {
    let dir = std::env::temp_dir().join(format!("emusync-bench-covers-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("cover dir");
    let covers = write_covers(&dir);
    let visible = visible_covers(&covers);

    let mut group = c.benchmark_group("library_frame");

    let ctx = Context::default();
    let mut app = library(&covers);
    load_visible(&ctx, &mut app, visible);
    group.bench_function("settled", |b| b.iter(|| frame(&ctx, &mut app, Vec::new())));

    group.sample_size(10);
    group.bench_function("cold_visible_covers", |b| {
        b.iter_batched(
            || (Context::default(), library(&covers)),
            |(ctx, mut app)| load_visible(&ctx, &mut app, visible),
            BatchSize::PerIteration,
        );
    });
    group.bench_function("cold_scroll_60_frames", |b| {
        b.iter_batched(
            || (Context::default(), library(&covers)),
            |(ctx, mut app)| {
                for _ in 0..60 {
                    frame(&ctx, &mut app, scroll_events(60.0));
                }
            },
            BatchSize::PerIteration,
        );
    });
    group.finish();

    drop(app);
    let _ = std::fs::remove_dir_all(&dir);
}

criterion_group!(benches, library_frame);
criterion_main!(benches);
