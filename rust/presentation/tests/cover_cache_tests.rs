//! Cover cache failure memory and size guard.

use egui::Context;
use emusync_presentation::CoverCache;

#[test]
fn failed_cover_is_remembered_until_failures_are_forgotten() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = std::env::temp_dir().join(format!("emusync-cover-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cover.png");
    let path_text = path.to_string_lossy().into_owned();

    assert!(cache.get_or_load(&ctx, &path_text).is_none());
    image::RgbaImage::new(2, 2)
        .save(&path)
        .expect("write cover");

    assert!(
        cache.get_or_load(&ctx, &path_text).is_none(),
        "a failure is not retried while remembered"
    );
    cache.forget_failures();
    assert!(
        cache.get_or_load(&ctx, &path_text).is_some(),
        "a cover created later loads after forgetting failures"
    );

    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn directories_and_blank_paths_are_not_loaded() {
    let ctx = Context::default();
    let mut cache = CoverCache::new();
    let dir = std::env::temp_dir();
    assert!(cache.get_or_load(&ctx, &dir.to_string_lossy()).is_none());
    assert!(cache.get_or_load(&ctx, "   ").is_none());
}
