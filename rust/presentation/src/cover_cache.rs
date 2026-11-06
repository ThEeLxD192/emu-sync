//! In-memory texture cache for game covers and art.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::collections::HashMap;

/// Cache mapping file paths or cover keys to egui TextureHandles.
pub struct CoverCache {
    textures: HashMap<String, TextureHandle>,
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
        if !self.textures.contains_key(trimmed)
            && let Ok(bytes) = std::fs::read(trimmed)
            && let Ok(image) = image::load_from_memory(&bytes)
        {
            let rgba = image.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let pixels = rgba.into_raw();
            let color_image = ColorImage::from_rgba_unmultiplied(size, &pixels);
            let texture = ctx.load_texture(trimmed, color_image, TextureOptions::LINEAR);
            self.textures.insert(trimmed.to_string(), texture);
        }
        self.textures.get(trimmed)
    }

    /// Explicitly inserts a texture into the cache.
    pub fn insert(&mut self, key: impl Into<String>, handle: TextureHandle) {
        self.textures.insert(key.into(), handle);
    }

    /// Clears the cache.
    pub fn clear(&mut self) {
        self.textures.clear();
    }
}
