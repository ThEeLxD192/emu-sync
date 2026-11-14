//! `cargo xtask perf`: runs the release app on a synthetic library and records frame,
//! CPU and memory metrics, so a change can be compared with an earlier build.
//!
//! - `cargo xtask perf run [--label NAME] [--runs N] [--bin PATH] [SCENARIO...]` builds
//!   the release app (unless `--bin` is given), runs each scenario `N` times and writes
//!   `target/perf/<label>/<scenario>.json` with the median of every metric.
//! - `cargo xtask perf compare BASE NEW` prints a table of both labels.
//!
//! Linux only: CPU and memory come from `/proc`. The window opens on the desktop and must
//! stay visible and untouched while a scenario runs.

use image::{ImageFormat, RgbImage};
use serde_json::{Map, Value, json};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub(crate) type PerfResult<T> = Result<T>;

const GAMES: usize = 200;
/// Bumped when the synthetic library changes, so stale data is regenerated.
const DATA_VERSION: &str = "1";

struct Scenario {
    name: &'static str,
    /// Value of `EMUSYNC_PERF_SCENARIO`.
    drive: &'static str,
    /// How long the app runs.
    run_for: Duration,
    /// CPU and frame rate are measured from here to the end, after startup settles.
    window_from: Duration,
}

const SCENARIOS: &[Scenario] = &[
    // Startup with 200 covers, then idle.
    Scenario {
        name: "library",
        drive: "none",
        run_for: Duration::from_secs(10),
        window_from: Duration::from_secs(4),
    },
    // Scrolls through covers that are not loaded yet. The app scrolls for 8 s once the
    // games are listed; the window stays inside that time.
    Scenario {
        name: "scroll",
        drive: "scroll",
        run_for: Duration::from_secs(8),
        window_from: Duration::from_secs(1),
    },
    // Crosses the whole library in 2 s, as when the scroll bar is dragged: most covers
    // pass by before they can be decoded. Measured over the whole run.
    Scenario {
        name: "fling",
        drive: "fling",
        run_for: Duration::from_secs(4),
        window_from: Duration::ZERO,
    },
    // The sync status with its spinner stays on screen.
    Scenario {
        name: "spinner",
        drive: "spinner",
        run_for: Duration::from_secs(10),
        window_from: Duration::from_secs(2),
    },
];

pub fn main(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        Some("compare") if args.len() == 3 => compare(&args[1], &args[2]),
        Some("drive") => crate::perf_drive::main(&args[1..]),
        _ => Err(
            "usage: cargo xtask perf run [--label NAME] [--runs N] [--bin PATH] \
                  [library|scroll|fling|spinner...] | cargo xtask perf compare BASE NEW \
                  | cargo xtask perf drive --config PATH --entry NAME [--label NAME] \
                  [--runs N] [--files N] [--bin PATH]"
                .into(),
        ),
    }
}

pub(crate) fn perf_dir() -> PathBuf {
    PathBuf::from("target/perf")
}

fn run(args: &[String]) -> Result<()> {
    let mut label = "current".to_string();
    let mut runs = 5usize;
    let mut bin = None;
    let mut selected = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--label" => label = args.next().ok_or("--label requires a name")?.clone(),
            "--runs" => runs = args.next().ok_or("--runs requires a number")?.parse()?,
            "--bin" => bin = Some(PathBuf::from(args.next().ok_or("--bin requires a path")?)),
            name => selected.push(
                SCENARIOS
                    .iter()
                    .find(|s| s.name == name)
                    .ok_or_else(|| format!("unknown scenario: {name}"))?,
            ),
        }
    }
    if selected.is_empty() {
        selected = SCENARIOS.iter().collect();
    }
    if runs == 0 {
        return Err("--runs must be at least 1".into());
    }
    let bin = match bin {
        Some(bin) => bin,
        None => {
            crate::run(&[
                "build",
                "--release",
                "--locked",
                "-p",
                "emusync-desktop-app",
            ])?;
            PathBuf::from("target/release/emusync-desktop-app")
        }
    };
    let data = synthetic_library()?;
    let out = perf_dir().join(&label);
    fs::create_dir_all(&out)?;
    let ticks = clock_ticks()?;
    for scenario in selected {
        let mut samples = Vec::new();
        for index in 1..=runs {
            println!("{} run {index}/{runs}", scenario.name);
            samples.push(run_once(&bin, &data, scenario, ticks)?);
        }
        let report = json!({
            "scenario": scenario.name,
            "runs": runs,
            "median": median(&samples),
            "samples": samples,
        });
        let path = out.join(format!("{}.json", scenario.name));
        fs::write(&path, serde_json::to_string_pretty(&report)?)?;
        print_table(scenario.name, &[(&label, &report["median"])]);
    }
    Ok(())
}

/// A config with one emulator holding `GAMES` ROMs, each with a cover beside it.
fn synthetic_library() -> Result<PathBuf> {
    let root = std::path::absolute(perf_dir().join("data"))?;
    let marker = root.join("version");
    if fs::read_to_string(&marker).is_ok_and(|v| v == DATA_VERSION) {
        return Ok(root.join("config.json"));
    }
    let _ = fs::remove_dir_all(&root);
    let roms = root.join("roms");
    fs::create_dir_all(&roms)?;
    println!("generating synthetic library in {}", root.display());
    for index in 0..GAMES {
        let name = format!("Game {index:03}");
        fs::write(roms.join(format!("{name}.iso")), b"")?;
        // Most covers are 600x900 JPEG, as scraped box art; every fourth is a large PNG,
        // as a cover the user picked.
        let (file, format, (width, height)) = if index % 4 == 3 {
            (format!("{name}.png"), ImageFormat::Png, (1500, 2250))
        } else {
            (format!("{name}.jpg"), ImageFormat::Jpeg, (600, 900))
        };
        cover_image(width, height, index as u32).save_with_format(roms.join(file), format)?;
    }
    let config = json!({
        "entries": [{
            "type": "emulator",
            "name": "Perf",
            "executablePath": "/bin/true",
            "romsDirectory": roms,
            "extensions": ["iso"],
        }]
    });
    fs::write(
        root.join("config.json"),
        serde_json::to_vec_pretty(&config)?,
    )?;
    fs::write(&marker, DATA_VERSION)?;
    Ok(root.join("config.json"))
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

fn clock_ticks() -> Result<f64> {
    let output = Command::new("getconf").arg("CLK_TCK").output()?;
    Ok(String::from_utf8(output.stdout)?.trim().parse()?)
}

/// CPU time used so far by the whole process, in milliseconds.
fn cpu_ms(pid: u32, ticks: f64) -> Result<f64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    // Fields after the parenthesized command name; utime and stime are fields 14 and 15.
    let rest = &stat[stat.rfind(')').ok_or("malformed stat")? + 2..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let utime: f64 = fields[11].parse()?;
    let stime: f64 = fields[12].parse()?;
    Ok((utime + stime) * 1000.0 / ticks)
}

/// Peak resident memory, in MiB.
fn peak_rss_mib(pid: u32) -> Result<f64> {
    let status = fs::read_to_string(format!("/proc/{pid}/status"))?;
    let line = status
        .lines()
        .find(|l| l.starts_with("VmHWM:"))
        .ok_or("no VmHWM")?;
    let kib: f64 = line
        .split_whitespace()
        .nth(1)
        .ok_or("malformed VmHWM")?
        .parse()?;
    Ok(kib / 1024.0)
}

pub(crate) struct Running(pub(crate) Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_once(bin: &Path, config: &Path, scenario: &Scenario, ticks: f64) -> Result<Value> {
    let log = std::path::absolute(perf_dir().join("frames.csv"))?;
    let _ = fs::remove_file(&log);
    let started = Instant::now();
    let mut child = Running(
        Command::new(bin)
            .arg("--config")
            .arg(config)
            .env("EMUSYNC_PERF_LOG", &log)
            .env("EMUSYNC_PERF_SCENARIO", scenario.drive)
            .spawn()?,
    );
    let pid = child.0.id();
    thread::sleep(scenario.window_from.saturating_sub(started.elapsed()));
    let window_cpu_start = cpu_ms(pid, ticks)?;
    let window_start = started.elapsed();
    thread::sleep(scenario.run_for.saturating_sub(started.elapsed()));
    if let Some(status) = child.0.try_wait()? {
        return Err(format!("the app exited early: {status}").into());
    }
    let cpu_end = cpu_ms(pid, ticks)?;
    let window = started.elapsed() - window_start;
    let rss = peak_rss_mib(pid)?;
    drop(child);

    let frames = read_frames(&log)?;
    let first = frames.first().ok_or("the app drew no frame")?;
    let final_covers = frames.last().map_or(0, |f| f.covers);
    // When the frame that first shows every cover has been drawn.
    let covers_ready = frames
        .iter()
        .find(|f| f.covers == final_covers)
        .map_or(0.0, |f| f.t_ms + f.ui_ms + f.paint_ms);
    let window_from_ms = window_start.as_secs_f64() * 1000.0;
    let in_window = frames.iter().filter(|f| f.t_ms >= window_from_ms).count();
    let mut ui: Vec<f64> = frames.iter().map(|f| f.ui_ms).collect();
    ui.sort_by(f64::total_cmp);
    let mut paint: Vec<f64> = frames.iter().map(|f| f.paint_ms).collect();
    paint.sort_by(f64::total_cmp);
    let mib = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
    let textures_max = frames.iter().filter_map(|f| f.texture_bytes).max();
    let textures_final = frames.last().and_then(|f| f.texture_bytes);
    Ok(json!({
        "first_frame_ms": first.t_ms,
        "frames": frames.len(),
        "fps_in_window": in_window as f64 / window.as_secs_f64(),
        "cpu_total_ms": cpu_end,
        "cpu_in_window_pct": (cpu_end - window_cpu_start) / (window.as_secs_f64() * 10.0),
        "ui_p50_ms": percentile(&ui, 0.50),
        "ui_p95_ms": percentile(&ui, 0.95),
        "ui_p99_ms": percentile(&ui, 0.99),
        "ui_max_ms": ui.last().copied().unwrap_or(0.0),
        // Frames whose UI alone misses a 60 Hz refresh: the stutter a user notices.
        "slow_frames": ui.iter().filter(|&&ms| ms > 1000.0 / 60.0).count(),
        "paint_p50_ms": percentile(&paint, 0.50),
        "paint_max_ms": paint.last().copied().unwrap_or(0.0),
        "covers_loaded": final_covers,
        "covers_ready_ms": covers_ready,
        "peak_rss_mib": rss,
        // Cover textures, which live in GPU memory that `peak_rss_mib` does not see.
        // Absent for builds that predate the column.
        "texture_max_mib": textures_max.map(mib),
        "texture_final_mib": textures_final.map(mib),
    }))
}

struct Frame {
    t_ms: f64,
    ui_ms: f64,
    paint_ms: f64,
    covers: u64,
    texture_bytes: Option<u64>,
}

fn read_frames(log: &Path) -> Result<Vec<Frame>> {
    let text = fs::read_to_string(log)?;
    let mut frames = Vec::new();
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split(',').collect();
        // Newer builds add the status and the texture memory; a shorter line was cut when
        // the app was stopped, and `#` lines report events.
        if !matches!(fields.len(), 4..=6) || fields.last().is_some_and(|f| f.is_empty()) {
            continue;
        }
        frames.push(Frame {
            t_ms: fields[0].parse::<f64>()? / 1000.0,
            ui_ms: fields[1].parse::<f64>()? / 1000.0,
            paint_ms: fields[2].parse::<f64>()? / 1000.0,
            covers: fields[3].parse()?,
            texture_bytes: fields.get(5).map(|f| f.parse()).transpose()?,
        });
    }
    Ok(frames)
}

pub(crate) fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

/// Median of each metric across runs.
pub(crate) fn median(samples: &[Value]) -> Value {
    let mut result = Map::new();
    let Some(first) = samples.first().and_then(Value::as_object) else {
        return Value::Object(result);
    };
    for key in first.keys() {
        let mut values: Vec<f64> = samples.iter().filter_map(|s| s[key].as_f64()).collect();
        values.sort_by(f64::total_cmp);
        result.insert(key.clone(), json!(percentile(&values, 0.5)));
    }
    Value::Object(result)
}

fn compare(base: &str, new: &str) -> Result<()> {
    for name in SCENARIOS.iter().map(|s| s.name).chain(["drive"]) {
        let read = |label: &str| -> Result<Option<Value>> {
            let path = perf_dir().join(label).join(format!("{name}.json"));
            match fs::read_to_string(path) {
                Ok(text) => Ok(Some(serde_json::from_str(&text)?)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.into()),
            }
        };
        if let (Some(a), Some(b)) = (read(base)?, read(new)?) {
            print_table(name, &[(base, &a["median"]), (new, &b["median"])]);
        }
    }
    Ok(())
}

pub(crate) fn print_table(scenario: &str, columns: &[(&str, &Value)]) {
    println!("\n### {scenario}\n");
    let header: Vec<&str> = columns.iter().map(|(label, _)| *label).collect();
    let change = columns.len() == 2;
    println!(
        "| metric | {} |{}",
        header.join(" | "),
        if change { " change |" } else { "" }
    );
    println!(
        "|---|{}{}",
        "---:|".repeat(columns.len()),
        if change { "---:|" } else { "" }
    );
    let Some(keys) = columns[0].1.as_object() else {
        return;
    };
    for key in keys.keys() {
        let values: Vec<f64> = columns
            .iter()
            .map(|(_, m)| m[key].as_f64().unwrap_or(f64::NAN))
            .collect();
        let cells: Vec<String> = values.iter().map(|v| format!("{v:.2}")).collect();
        let delta = if change && values[0] != 0.0 {
            format!(" {:+.1}% |", (values[1] - values[0]) / values[0] * 100.0)
        } else if change {
            " — |".to_string()
        } else {
            String::new()
        };
        println!("| {key} | {} |{delta}", cells.join(" | "));
    }
}
