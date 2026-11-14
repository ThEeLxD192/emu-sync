//! In-memory texture cache for game covers and art.
//!
//! Covers are read and decoded on background threads so the UI never waits for the disk
//! or a decoder: a cover that is not ready yet is simply not drawn, and the frame after it
//! finishes shows it. Large images are scaled down to what a card or preview can show.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex, PoisonError,
        mpsc::{Receiver, Sender, channel},
    },
    thread,
};

/// Largest cover file read from disk, so a wrong path cannot tie up a decoder.
const MAX_COVER_BYTES: u64 = 32 * 1024 * 1024;
/// Largest cover kept in memory: covers are drawn at most a few hundred pixels tall, so
/// this keeps them sharp on high-density screens without holding full-size art.
const MAX_COVER_SIZE: (u32, u32) = (600, 900);
/// Upper bound on decoding threads; covers are decoded only while the library scrolls.
const MAX_DECODERS: usize = 4;

enum Slot {
    /// Being decoded by the job with this id.
    Loading(u64),
    Ready(TextureHandle),
    /// Could not be read or decoded; not tried again until failures are forgotten.
    Failed,
}

struct Job {
    id: u64,
    path: String,
}

struct Decoded {
    id: u64,
    path: String,
    image: Option<ColorImage>,
}

/// Jobs waiting for a decoder. The newest is taken first: while the library scrolls, the
/// covers on screen were requested last.
#[derive(Default)]
struct Queue {
    jobs: Mutex<QueueState>,
    ready: Condvar,
}

#[derive(Default)]
struct QueueState {
    pending: Vec<Job>,
    closed: bool,
}

struct Decoders {
    queue: Arc<Queue>,
    results: Receiver<Decoded>,
}

/// Cache mapping file paths or cover keys to egui TextureHandles; a path that could not be
/// loaded is remembered so it is not read again every frame.
pub struct CoverCache {
    textures: HashMap<String, Slot>,
    /// Memory held by the loaded textures, in bytes.
    texture_bytes: usize,
    next_job: u64,
    decoders: Option<Decoders>,
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
            texture_bytes: 0,
            next_job: 0,
            decoders: None,
        }
    }

    /// Retrieves an existing texture, or starts loading it in the background. Returns
    /// `None` while the cover loads, and for a cover that cannot be loaded.
    pub fn get_or_load(&mut self, ctx: &Context, path: &str) -> Option<&TextureHandle> {
        self.receive(ctx);
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return None;
        }
        if !self.textures.contains_key(trimmed) {
            let id = self.next_job;
            self.next_job += 1;
            self.textures.insert(trimmed.to_string(), Slot::Loading(id));
            self.decoders(ctx).submit(Job {
                id,
                path: trimmed.to_string(),
            });
        }
        match self.textures.get(trimmed) {
            Some(Slot::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    /// Explicitly inserts a texture into the cache.
    pub fn insert(&mut self, key: impl Into<String>, handle: TextureHandle) {
        self.texture_bytes += texture_size(&handle);
        if let Some(Slot::Ready(old)) = self.textures.insert(key.into(), Slot::Ready(handle)) {
            self.texture_bytes -= texture_size(&old);
        }
    }

    /// Forgets remembered failures so a cover created later is tried again.
    pub fn forget_failures(&mut self) {
        self.textures
            .retain(|_, slot| !matches!(slot, Slot::Failed));
    }

    /// Number of covers loaded and ready to draw.
    pub fn loaded_count(&self) -> usize {
        self.textures
            .values()
            .filter(|slot| matches!(slot, Slot::Ready(_)))
            .count()
    }

    /// Memory held by the loaded covers, in bytes (RGBA, as uploaded to the GPU).
    pub fn texture_bytes(&self) -> usize {
        self.texture_bytes
    }

    /// Number of covers still being loaded.
    pub fn pending_count(&self) -> usize {
        self.textures
            .values()
            .filter(|slot| matches!(slot, Slot::Loading(_)))
            .count()
    }

    /// Clears the cache. Covers still loading are discarded when they finish.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.texture_bytes = 0;
    }

    /// Stores the covers decoded since the last frame.
    fn receive(&mut self, ctx: &Context) {
        let Some(decoders) = &self.decoders else {
            return;
        };
        while let Ok(decoded) = decoders.results.try_recv() {
            // Only the job the slot waits for may fill it: a cover cleared meanwhile and
            // requested again waits for its newer job.
            let Some(slot) = self.textures.get_mut(&decoded.path) else {
                continue;
            };
            if !matches!(slot, Slot::Loading(id) if *id == decoded.id) {
                continue;
            }
            *slot = match decoded.image {
                Some(image) => {
                    let texture = ctx.load_texture(&decoded.path, image, TextureOptions::LINEAR);
                    self.texture_bytes += texture_size(&texture);
                    Slot::Ready(texture)
                }
                None => Slot::Failed,
            };
        }
    }

    fn decoders(&mut self, ctx: &Context) -> &Decoders {
        self.decoders.get_or_insert_with(|| Decoders::start(ctx))
    }
}

impl Decoders {
    fn start(ctx: &Context) -> Self {
        let queue = Arc::new(Queue::default());
        let (results_tx, results) = channel();
        let count = thread::available_parallelism()
            .map_or(1, |n| n.get())
            .clamp(1, MAX_DECODERS);
        for index in 0..count {
            let queue = queue.clone();
            let results_tx = results_tx.clone();
            let ctx = ctx.clone();
            // A decoder that cannot start leaves its share to the others; with none, covers
            // stay loading, which only means they are not drawn.
            let _ = thread::Builder::new()
                .name(format!("cover-decoder-{index}"))
                .spawn(move || decode_jobs(&queue, &results_tx, &ctx));
        }
        Self { queue, results }
    }

    fn submit(&self, job: Job) {
        let mut state = self
            .queue
            .jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.pending.push(job);
        drop(state);
        self.queue.ready.notify_one();
    }
}

impl Drop for Decoders {
    /// Stops the decoders. They are not joined: one may be in the middle of a decode, and
    /// the UI must not wait for it. Each finishes its current cover and exits.
    fn drop(&mut self) {
        self.queue
            .jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .closed = true;
        self.queue.ready.notify_all();
    }
}

fn decode_jobs(queue: &Queue, results: &Sender<Decoded>, ctx: &Context) {
    loop {
        let job = {
            let mut state = queue.jobs.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if state.closed {
                    return;
                }
                if let Some(job) = state.pending.pop() {
                    break job;
                }
                state = queue
                    .ready
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        let image = decode(&job.path);
        let decoded = Decoded {
            id: job.id,
            path: job.path,
            image,
        };
        if results.send(decoded).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

fn texture_size(texture: &TextureHandle) -> usize {
    let [width, height] = texture.size();
    width * height * 4
}

fn decode(path: &str) -> Option<ColorImage> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_COVER_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let mut image = image::load_from_memory(&bytes).ok()?;
    let (max_width, max_height) = MAX_COVER_SIZE;
    if image.width() > max_width || image.height() > max_height {
        image = image.thumbnail(max_width, max_height);
    }
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}
