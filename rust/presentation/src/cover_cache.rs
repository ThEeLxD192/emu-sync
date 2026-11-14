//! In-memory texture cache for game covers and art.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::collections::HashMap;

/// Largest cover file read from disk, so a wrong path cannot stall the UI thread.
const MAX_COVER_BYTES: u64 = 32 * 1024 * 1024;

/// Cache mapping file paths or cover keys to egui TextureHandles; `None` remembers a
/// path that could not be loaded so it is not read again every frame.
pub struct CoverCache {
    textures: HashMap<String, Option<TextureHandle>>,
}

impl Default for CoverCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CoverCache {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
        }
    }

    /// Retrieves an existing texture, or loads it from disk if not yet cached.
    pub fn get_or_load(&mut self, ctx: &Context, path: &str) -> Option<&TextureHandle> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return None;
        }
        if !self.textures.contains_key(trimmed) {
            let texture = load_texture(ctx, trimmed);
            self.textures.insert(trimmed.to_string(), texture);
        }
        self.textures.get(trimmed).and_then(Option::as_ref)
    }

    /// Explicitly inserts a texture into the cache.
    pub fn insert(&mut self, key: impl Into<String>, handle: TextureHandle) {
        self.textures.insert(key.into(), Some(handle));
    }

    /// Forgets remembered failures so a cover created later is tried again.
    pub fn forget_failures(&mut self) {
        self.textures.retain(|_, texture| texture.is_some());
    }

    /// Number of covers loaded and ready to draw.
    pub fn loaded_count(&self) -> usize {
        self.textures
            .values()
            .filter(|texture| texture.is_some())
            .count()
    }

    /// Clears the cache.
    pub fn clear(&mut self) {
        self.textures.clear();
    }
}

fn load_texture(ctx: &Context, path: &str) -> Option<TextureHandle> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_COVER_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();
    let color_image = ColorImage::from_rgba_unmultiplied(size, &pixels);
    Some(ctx.load_texture(path, color_image, TextureOptions::LINEAR))
}
