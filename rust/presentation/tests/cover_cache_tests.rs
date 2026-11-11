//! Cover cache failure memory and size guard.

mod common;

use common::TempDirGuard;
use egui::Context;
use emusync_presentation::CoverCache;
use std::{fs::File, path::Path};

fn write_png(path: &Path) {
    image::RgbaImage::new(2, 2).save(path).expect("write cover");
}

#[test]
fn failed_cover_is_remembered_until_failures_are_forgotten() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = TempDirGuard::new("failed-cover");
    let path = dir.path.join("cover.png");
    let path_text = path.to_string_lossy().into_owned();

    assert!(cache.get_or_load(&ctx, &path_text).is_none());
    write_png(&path);

    assert!(
        cache.get_or_load(&ctx, &path_text).is_none(),
        "a failure is not retried while remembered"
    );
    cache.forget_failures();
    assert!(
        cache.get_or_load(&ctx, &path_text).is_some(),
        "a cover created later loads after forgetting failures"
    );
}

#[test]
fn directories_and_blank_paths_are_not_loaded() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = std::env::temp_dir();
    assert!(cache.get_or_load(&ctx, &dir.to_string_lossy()).is_none());
    assert!(cache.get_or_load(&ctx, "   ").is_none());
}

#[test]
fn oversized_cover_is_rejected_before_decoding() {
    let ctx = Context::default();
    let dir = TempDirGuard::new("oversized-cover");
    let path = dir.path.join("cover.png");
    write_png(&path);
    let path_text = path.to_string_lossy().into_owned();

    // Trailing zeros do not stop a PNG from decoding, so only the size guard can
    // reject this file.
    assert!(CoverCache::new().get_or_load(&ctx, &path_text).is_some());
    File::options()
        .write(true)
        .open(&path)
        .expect("open cover")
        .set_len(32 * 1024 * 1024 + 1)
        .expect("grow cover");

    assert!(CoverCache::new().get_or_load(&ctx, &path_text).is_none());
}

#[test]
fn corrupt_image_is_remembered_as_failure() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = TempDirGuard::new("corrupt-cover");
    let path = dir.path.join("cover.png");
    let path_text = path.to_string_lossy().into_owned();
    std::fs::write(&path, b"not an image").expect("write corrupt cover");

    assert!(cache.get_or_load(&ctx, &path_text).is_none());
    write_png(&path);
    assert!(
        cache.get_or_load(&ctx, &path_text).is_none(),
        "the failure is remembered even though the file is now valid"
    );

    cache.forget_failures();
    assert!(cache.get_or_load(&ctx, &path_text).is_some());
}

#[test]
fn padded_and_trimmed_paths_share_one_entry() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = TempDirGuard::new("padded-cover");
    let path = dir.path.join("cover.png");
    write_png(&path);
    let path_text = path.to_string_lossy().into_owned();

    assert!(
        cache
            .get_or_load(&ctx, &format!("  {path_text}  "))
            .is_some()
    );
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

    assert!(cache.get_or_load(&ctx, "/dev/zero").is_none());
}
