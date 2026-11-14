//! Cover cache: background loading, failure memory, size guards and scaling.

mod common;

use common::TempDirGuard;
use egui::{Context, RawInput};
use emusync_presentation::CoverCache;
use std::{
    fs::File,
    path::Path,
    thread,
    time::{Duration, Instant},
};

fn write_png(path: &Path, width: u32, height: u32) {
    image::RgbaImage::new(width, height)
        .save(path)
        .expect("write cover");
}

/// Asks for `path` on every frame until it is no longer loading, as the grid does, and
/// returns the size of the texture when it loaded.
fn settle(cache: &mut CoverCache, ctx: &Context, path: &str) -> Option<[usize; 2]> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(texture) = cache.get_or_load(ctx, path) {
            return Some(texture.size());
        }
        if cache.pending_count() == 0 {
            return None;
        }
        assert!(Instant::now() < deadline, "cover still loading: {path}");
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_cover_loads_in_the_background_and_appears_on_a_later_frame() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = TempDirGuard::new("background-cover");
    let path = dir.path.join("cover.png");
    write_png(&path, 2, 2);
    let path_text = path.to_string_lossy().into_owned();

    assert!(
        cache.get_or_load(&ctx, &path_text).is_none(),
        "the frame that asks for a cover does not wait for it"
    );
    assert_eq!(cache.pending_count(), 1);
    assert_eq!(settle(&mut cache, &ctx, &path_text), Some([2, 2]));
    assert_eq!((cache.loaded_count(), cache.pending_count()), (1, 0));
}

#[test]
fn large_covers_are_scaled_down_keeping_their_shape() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = TempDirGuard::new("large-cover");
    let tall = dir.path.join("tall.png");
    let wide = dir.path.join("wide.png");
    let small = dir.path.join("small.png");
    write_png(&tall, 1200, 3000);
    write_png(&wide, 2400, 600);
    write_png(&small, 300, 450);

    assert_eq!(
        settle(&mut cache, &ctx, &tall.to_string_lossy()),
        Some([240, 600])
    );
    assert_eq!(
        settle(&mut cache, &ctx, &wide.to_string_lossy()),
        Some([400, 100])
    );
    assert_eq!(
        settle(&mut cache, &ctx, &small.to_string_lossy()),
        Some([300, 450]),
        "small covers keep their size"
    );
    assert_eq!(
        cache.texture_bytes(),
        (240 * 600 + 400 * 100 + 300 * 450) * 4,
        "the reported memory is that of the RGBA textures held"
    );
}

/// Ends a frame: covers asked for before it count as drawn in an earlier pass.
fn next_frame(ctx: &Context) {
    ctx.run_ui(RawInput::default(), |_| {})
        .drop_without_applying_deltas();
}

/// Bytes of a 100x100 cover once loaded.
const COVER: usize = 100 * 100 * 4;

#[test]
fn over_budget_the_cover_drawn_least_recently_is_released_and_loads_again() {
    let ctx = Context::default();
    let mut cache = CoverCache::with_budget(2 * COVER);
    let dir = TempDirGuard::new("budget-cover");
    let paths: Vec<String> = ["a", "b", "c"]
        .iter()
        .map(|name| {
            let path = dir.path.join(format!("{name}.png"));
            write_png(&path, 100, 100);
            path.to_string_lossy().into_owned()
        })
        .collect();

    for path in &paths {
        assert!(settle(&mut cache, &ctx, path).is_some());
        next_frame(&ctx);
        next_frame(&ctx);
    }

    assert_eq!(cache.loaded_count(), 2, "the oldest cover was released");
    assert_eq!(cache.texture_bytes(), 2 * COVER);
    assert!(
        settle(&mut cache, &ctx, &paths[0]).is_some(),
        "a released cover loads again when it is drawn again"
    );
}

#[test]
fn covers_drawn_in_this_or_the_previous_frame_are_kept_over_budget() {
    let ctx = Context::default();
    let mut cache = CoverCache::with_budget(0);
    let dir = TempDirGuard::new("visible-cover");
    let first = dir.path.join("first.png");
    let second = dir.path.join("second.png");
    write_png(&first, 100, 100);
    write_png(&second, 100, 100);
    let first = first.to_string_lossy().into_owned();
    let second = second.to_string_lossy().into_owned();

    assert!(settle(&mut cache, &ctx, &first).is_some());
    next_frame(&ctx);
    // The next frame draws the second cover before reaching the first one again.
    assert!(settle(&mut cache, &ctx, &second).is_some());
    assert!(
        cache.get_or_load(&ctx, &first).is_some(),
        "a cover drawn in the previous frame is not released before it is drawn again"
    );
    assert_eq!(cache.texture_bytes(), 2 * COVER);
}

#[test]
fn failed_cover_is_remembered_until_failures_are_forgotten() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = TempDirGuard::new("failed-cover");
    let path = dir.path.join("cover.png");
    let path_text = path.to_string_lossy().into_owned();

    assert!(settle(&mut cache, &ctx, &path_text).is_none());
    write_png(&path, 2, 2);

    assert!(
        settle(&mut cache, &ctx, &path_text).is_none(),
        "a failure is not retried while remembered"
    );
    cache.forget_failures();
    assert!(
        settle(&mut cache, &ctx, &path_text).is_some(),
        "a cover created later loads after forgetting failures"
    );
}

#[test]
fn directories_and_blank_paths_are_not_loaded() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = std::env::temp_dir();
    assert!(settle(&mut cache, &ctx, &dir.to_string_lossy()).is_none());
    assert!(cache.get_or_load(&ctx, "   ").is_none());
    assert_eq!(cache.pending_count(), 0, "a blank path starts no load");
}

#[test]
fn oversized_cover_is_rejected_before_decoding() {
    let ctx = Context::default();
    let dir = TempDirGuard::new("oversized-cover");
    let path = dir.path.join("cover.png");
    write_png(&path, 2, 2);
    let path_text = path.to_string_lossy().into_owned();

    // Trailing zeros do not stop a PNG from decoding, so only the size guard can
    // reject this file.
    assert!(settle(&mut CoverCache::new(), &ctx, &path_text).is_some());
    File::options()
        .write(true)
        .open(&path)
        .expect("open cover")
        .set_len(32 * 1024 * 1024 + 1)
        .expect("grow cover");

    assert!(settle(&mut CoverCache::new(), &ctx, &path_text).is_none());
}

#[test]
fn corrupt_image_is_remembered_as_failure() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = TempDirGuard::new("corrupt-cover");
    let path = dir.path.join("cover.png");
    let path_text = path.to_string_lossy().into_owned();
    std::fs::write(&path, b"not an image").expect("write corrupt cover");

    assert!(settle(&mut cache, &ctx, &path_text).is_none());
    write_png(&path, 2, 2);
    assert!(
        settle(&mut cache, &ctx, &path_text).is_none(),
        "the failure is remembered even though the file is now valid"
    );

    cache.forget_failures();
    assert!(settle(&mut cache, &ctx, &path_text).is_some());
}

#[test]
fn padded_and_trimmed_paths_share_one_entry() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = TempDirGuard::new("padded-cover");
    let path = dir.path.join("cover.png");
    write_png(&path, 2, 2);
    let path_text = path.to_string_lossy().into_owned();

    assert!(settle(&mut cache, &ctx, &format!("  {path_text}  ")).is_some());
    std::fs::remove_file(&path).expect("remove cover");

    assert!(
        cache.get_or_load(&ctx, &path_text).is_some(),
        "the trimmed path is served from the cache"
    );
}

#[cfg(unix)]
#[test]
fn device_paths_are_not_read() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();

    assert!(settle(&mut cache, &ctx, "/dev/zero").is_none());
}
