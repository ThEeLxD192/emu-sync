//! Opt-in frame metrics read by `cargo xtask perf`.
//!
//! Nothing is recorded unless `EMUSYNC_PERF_LOG` names a file. Each frame then appends
//! one CSV line (flushed at once, so a killed process keeps every frame it drew):
//! time since start, time spent building the UI, time spent tessellating and painting,
//! how many covers the cache holds and their texture memory, and the application status. `EMUSYNC_PERF_SCENARIO`
//! optionally drives the UI so a scenario measures the same work on every build:
//!
//! - `spinner`: the sync status stays on screen, as during a long transfer;
//! - `scroll`: once the game grid has games, it scrolls down at a fixed speed for
//!   `SCROLL_FOR`;
//! - `fling`: the same, `FLING_FACTOR` times faster, as when the scroll bar is dragged;
//! - `sync`: once the startup check of the selected entry ends, synchronizes it once and
//!   appends `# sync <final status> <milliseconds>` when the status leaves syncing.

use egui::{Event, Modifiers, MouseWheelSource, MouseWheelUnit, RawInput, TouchPhase, pos2, vec2};
use std::{
    fs::File,
    io::{self, BufWriter, Write},
    time::{Duration, Instant},
};

/// Scroll speed of the `scroll` scenario, in points per second.
const SCROLL_SPEED: f32 = 1500.0;
/// How long the `scroll` scenario keeps scrolling.
const SCROLL_FOR: Duration = Duration::from_secs(8);
/// How much faster, and so shorter, the `fling` scenario scrolls.
const FLING_FACTOR: u32 = 4;
/// A point over the game grid at the default window size.
const GRID_POINT: (f32, f32) = (700.0, 450.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    None,
    Spinner,
    Scroll,
    Fling,
    Sync,
}

pub struct PerfLog {
    out: BufWriter<File>,
    started: Instant,
    scenario: Scenario,
    /// When the `scroll` scenario began, and when it last scrolled.
    scrolling: Option<(Instant, Instant)>,
    sync: SyncProgress,
}

#[derive(Clone, Copy, PartialEq)]
enum SyncProgress {
    NotRequested,
    Requested { at: Instant, seen_syncing: bool },
    Reported,
}

/// Timing of one drawn frame.
pub struct FrameSample {
    pub began: Instant,
    pub ui: Duration,
    pub paint: Duration,
    pub covers: usize,
    pub texture_bytes: usize,
    /// A short name of the application status.
    pub status: &'static str,
}

impl PerfLog {
    /// The log configured by the environment, if any. `started` is when the process began.
    pub fn from_env(started: Instant) -> io::Result<Option<Self>> {
        let Some(path) = std::env::var_os("EMUSYNC_PERF_LOG") else {
            return Ok(None);
        };
        let scenario = match std::env::var("EMUSYNC_PERF_SCENARIO").as_deref() {
            Err(_) | Ok("" | "none") => Scenario::None,
            Ok("spinner") => Scenario::Spinner,
            Ok("scroll") => Scenario::Scroll,
            Ok("fling") => Scenario::Fling,
            Ok("sync") => Scenario::Sync,
            Ok(other) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown EMUSYNC_PERF_SCENARIO: {other}"),
                ));
            }
        };
        let mut out = BufWriter::new(File::create(path)?);
        writeln!(out, "t_us,ui_us,paint_us,covers,status,texture_bytes")?;
        out.flush()?;
        Ok(Some(Self {
            out,
            started,
            scenario,
            scrolling: None,
            sync: SyncProgress::NotRequested,
        }))
    }

    pub fn scenario(&self) -> Scenario {
        self.scenario
    }

    /// Adds the scenario's synthetic input to this frame. Returns whether the scenario
    /// needs another frame right away.
    pub fn drive(&mut self, input: &mut RawInput, has_games: bool) -> bool {
        let (speed, scroll_for) = match self.scenario {
            Scenario::Scroll => (SCROLL_SPEED, SCROLL_FOR),
            Scenario::Fling => (
                SCROLL_SPEED * FLING_FACTOR as f32,
                SCROLL_FOR / FLING_FACTOR,
            ),
            _ => return false,
        };
        if !has_games {
            return false;
        }
        let now = Instant::now();
        let (began, last) = *self.scrolling.get_or_insert((now, now));
        if now.duration_since(began) > scroll_for {
            return false;
        }
        self.scrolling = Some((began, now));
        input
            .events
            .push(Event::PointerMoved(pos2(GRID_POINT.0, GRID_POINT.1)));
        input.events.push(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, -speed * now.duration_since(last).as_secs_f32()),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Unknown,
            modifiers: Modifiers::default(),
        });
        true
    }

    /// Whether the `sync` scenario should start its synchronization now. True once, when
    /// `ready` first is.
    pub fn take_sync_request(&mut self, ready: bool) -> bool {
        let start =
            self.scenario == Scenario::Sync && ready && self.sync == SyncProgress::NotRequested;
        if start {
            self.sync = SyncProgress::Requested {
                at: Instant::now(),
                seen_syncing: false,
            };
        }
        start
    }

    /// Follows every status change, not only those a frame shows: a quick synchronization
    /// can start and end between two frames.
    pub fn observe_status(&mut self, status: &'static str) {
        let SyncProgress::Requested { at, seen_syncing } = &mut self.sync else {
            return;
        };
        if status == "syncing" {
            *seen_syncing = true;
            return;
        }
        // Idle before syncing began is the state the request found, not its end.
        if status == "idle" && !*seen_syncing {
            return;
        }
        let line = format!("# sync {status} {}", at.elapsed().as_secs_f64() * 1000.0);
        self.sync = SyncProgress::Reported;
        if let Err(error) = writeln!(self.out, "{line}").and_then(|()| self.out.flush()) {
            eprintln!("EMUSYNC_PERF_LOG: {error}");
        }
    }

    /// Appends one frame. A write failure disables nothing else: metrics are best effort.
    pub fn record(&mut self, frame: &FrameSample) {
        let line = writeln!(
            self.out,
            "{},{},{},{},{},{}",
            frame.began.duration_since(self.started).as_micros(),
            frame.ui.as_micros(),
            frame.paint.as_micros(),
            frame.covers,
            frame.status,
            frame.texture_bytes,
        )
        .and_then(|()| self.out.flush());
        if let Err(error) = line {
            eprintln!("EMUSYNC_PERF_LOG: {error}");
        }
    }
}
