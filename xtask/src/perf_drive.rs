//! `cargo xtask perf drive`: times real synchronizations of one entry against Google Drive.
//!
//! The entry and the Drive connection are copied from `--config` into an isolated config
//! under `target/perf/drive`, whose save folder holds the entry's own files plus `--files`
//! synthetic files under `perf/`. Each run measures three synchronizations of that folder:
//!
//! - `upload_ms`: the synthetic files are new and are uploaded;
//! - `unchanged_ms`: nothing changed since;
//! - `changed_ms`: ten synthetic files changed and are uploaded again.
//!
//! Before and after every run, the Drive files whose name starts with `perf/` are deleted
//! from the entry's cloud folder, so it keeps only what it held before. The isolated
//! config holds Drive credentials and is removed at the end.

use crate::perf::{PerfResult as Result, Running, median, perf_dir, print_table};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Prefix, inside the save folder and therefore in Drive names, of the synthetic files.
const PREFIX: &str = "perf/";
const FILE_SIZE: usize = 4 * 1024;
const CHANGED_FILES: usize = 10;
/// Longest a synchronization may take before the run is abandoned.
const SYNC_TIMEOUT: Duration = Duration::from_secs(900);
const DRIVE_API: &str = "https://www.googleapis.com/drive/v3/files";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

struct Options {
    config: PathBuf,
    entry: String,
    label: String,
    runs: usize,
    files: usize,
    bin: Option<PathBuf>,
}

pub fn main(args: &[String]) -> Result<()> {
    let options = parse(args)?;
    let source: Value = serde_json::from_slice(&fs::read(&options.config)?)?;
    let drive = source
        .get("googleDrive")
        .filter(|d| d.get("refreshToken").and_then(Value::as_str).is_some())
        .ok_or("the config has no connected Google Drive")?
        .clone();
    let entry = source["entries"]
        .as_array()
        .and_then(|entries| entries.iter().find(|e| e["name"] == options.entry.as_str()))
        .ok_or_else(|| format!("no entry named {}", options.entry))?
        .clone();
    let cloud_folder = entry["cloudFolder"]
        .as_str()
        .or_else(|| entry["group"].as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&options.entry)
        .to_string();
    let original_saves = single_directory_target(&entry)?;

    let bin = match &options.bin {
        Some(bin) => bin.clone(),
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
    let root = std::path::absolute(perf_dir().join("drive"))?;
    let drive_api = DriveApi::new(&root, &drive)?;
    let result = measure(
        &options,
        &bin,
        &root,
        &drive,
        &entry,
        &original_saves,
        &drive_api,
        &cloud_folder,
    );
    // Whatever happened, leave Drive as it was and remove the credentials copy.
    let cleaned = drive_api.delete_synthetic(&cloud_folder);
    let _ = fs::remove_dir_all(&root);
    let samples = result?;
    let deleted = cleaned?;
    println!("deleted {deleted} synthetic file(s) from Drive after the last run");

    let report = json!({
        "scenario": "drive",
        "runs": options.runs,
        "files": options.files,
        "median": median(&samples),
        "samples": samples,
    });
    let out = perf_dir().join(&options.label);
    fs::create_dir_all(&out)?;
    fs::write(
        out.join("drive.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    print_table("drive", &[(&options.label, &report["median"])]);
    Ok(())
}

fn parse(args: &[String]) -> Result<Options> {
    let mut config = None;
    let mut entry = None;
    let mut options = Options {
        config: PathBuf::new(),
        entry: String::new(),
        label: "current".into(),
        runs: 3,
        files: 200,
        bin: None,
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} requires a value"));
        match arg.as_str() {
            "--config" => config = Some(PathBuf::from(value()?)),
            "--entry" => entry = Some(value()?.clone()),
            "--label" => options.label = value()?.clone(),
            "--runs" => options.runs = value()?.parse()?,
            "--files" => options.files = value()?.parse()?,
            "--bin" => options.bin = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown option: {other}").into()),
        }
    }
    options.config = config.ok_or("--config is required")?;
    options.entry = entry.ok_or("--entry is required")?;
    if options.runs == 0 || options.files < CHANGED_FILES {
        return Err(format!("need at least 1 run and {CHANGED_FILES} files").into());
    }
    Ok(options)
}

/// The entry's only save target, which must be a directory.
fn single_directory_target(entry: &Value) -> Result<PathBuf> {
    match entry["saveTargets"].as_array().map(Vec::as_slice) {
        Some([target]) if target["type"] == "directory" => Ok(PathBuf::from(
            target["path"]
                .as_str()
                .ok_or("save target without a path")?,
        )),
        _ => Err("the entry must have exactly one directory save target".into()),
    }
}

#[allow(clippy::too_many_arguments)]
fn measure(
    options: &Options,
    bin: &Path,
    root: &Path,
    drive: &Value,
    entry: &Value,
    original_saves: &Path,
    api: &DriveApi,
    cloud_folder: &str,
) -> Result<Vec<Value>> {
    let mut samples = Vec::new();
    for run in 1..=options.runs {
        let leftover = api.delete_synthetic(cloud_folder)?;
        if leftover > 0 {
            println!("deleted {leftover} synthetic file(s) left in Drive");
        }
        let config = prepare(root, drive, entry, original_saves)?;
        let saves = root.join("saves");
        // Brings what only Drive holds, so the measured runs ask nothing.
        println!(
            "drive run {run}/{}: matching the cloud folder",
            options.runs
        );
        sync_once(bin, &config)?;
        for index in 0..options.files {
            write_synthetic(&saves, index, 0)?;
        }
        println!("drive run {run}/{}: upload", options.runs);
        let upload = sync_once(bin, &config)?;
        println!("drive run {run}/{}: unchanged", options.runs);
        let unchanged = sync_once(bin, &config)?;
        for index in 0..CHANGED_FILES {
            write_synthetic(&saves, index, run + 1)?;
        }
        println!("drive run {run}/{}: changed", options.runs);
        let changed = sync_once(bin, &config)?;
        samples.push(json!({
            "upload_ms": upload,
            "unchanged_ms": unchanged,
            "changed_ms": changed,
        }));
    }
    Ok(samples)
}

/// A fresh isolated config whose entry saves to `root/saves`, holding the entry's own files.
fn prepare(root: &Path, drive: &Value, entry: &Value, original_saves: &Path) -> Result<PathBuf> {
    let _ = fs::remove_dir_all(root);
    let saves = root.join("saves");
    fs::create_dir_all(saves.join(PREFIX))?;
    for file in fs::read_dir(original_saves)? {
        let file = file?;
        if file.file_type()?.is_file() {
            fs::copy(file.path(), saves.join(file.file_name()))?;
        }
    }
    let mut entry = entry.clone();
    let saves_text = saves.to_str().ok_or("non-UTF-8 path")?;
    entry["savePaths"] = json!([saves_text]);
    entry["saveTargets"] = json!([{ "path": saves_text, "type": "directory" }]);
    let config = root.join("config.json");
    write_private(
        &config,
        &serde_json::to_vec_pretty(&json!({ "googleDrive": drive, "entries": [entry] }))?,
    )?;
    Ok(config)
}

fn write_synthetic(saves: &Path, index: usize, version: usize) -> Result<()> {
    let mut data = vec![0u8; FILE_SIZE];
    let mut state = (index as u64 + 1) * 0x9e37_79b9 + version as u64;
    for byte in &mut data {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        *byte = (state >> 56) as u8;
    }
    fs::write(saves.join(format!("{PREFIX}file-{index:04}.bin")), data)?;
    Ok(())
}

/// Starts the app in the `sync` scenario and returns how long the synchronization took, as
/// the app reports it: from its request until the status leaves syncing.
fn sync_once(bin: &Path, config: &Path) -> Result<f64> {
    let log = std::path::absolute(perf_dir().join("drive-frames.csv"))?;
    let _ = fs::remove_file(&log);
    let child = Running(
        Command::new(bin)
            .arg("--config")
            .arg(config)
            .env("EMUSYNC_PERF_LOG", &log)
            .env("EMUSYNC_PERF_SCENARIO", "sync")
            .stdout(Stdio::null())
            .spawn()?,
    );
    let started = Instant::now();
    loop {
        thread::sleep(Duration::from_millis(250));
        if started.elapsed() > SYNC_TIMEOUT {
            return Err("the synchronization did not finish in time".into());
        }
        let text = fs::read_to_string(&log).unwrap_or_default();
        let Some(report) = text.lines().find_map(|line| line.strip_prefix("# sync ")) else {
            continue;
        };
        drop(child);
        return match report.split_once(' ') {
            Some(("idle", ms)) => Ok(ms.parse()?),
            _ => Err(format!("the synchronization ended as: {report}").into()),
        };
    }
}

fn write_private(path: &Path, contents: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(contents)?;
    Ok(())
}

/// Just enough of the Drive API, through `curl`, to delete the synthetic files. Secrets go
/// through private files, never the command line.
struct DriveApi {
    dir: PathBuf,
}

impl DriveApi {
    fn new(root: &Path, drive: &Value) -> Result<Self> {
        let dir = root.with_file_name("drive-api");
        fs::create_dir_all(&dir)?;
        let field = |name: &str| drive[name].as_str().unwrap_or_default().to_string();
        let form = format!(
            "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
            encode(&field("clientId")),
            encode(&field("clientSecret")),
            encode(&field("refreshToken")),
        );
        write_private(&dir.join("token-form"), form.as_bytes())?;
        Ok(Self { dir })
    }

    fn curl(&self, args: &[&str]) -> Result<Value> {
        let output = Command::new("curl")
            .args(["--silent", "--show-error", "--fail-with-body"])
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "curl failed: {}{}",
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            )
            .into());
        }
        if output.stdout.is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    fn authorize(&self) -> Result<()> {
        let form = format!("@{}", self.dir.join("token-form").display());
        let token = self.curl(&["--data", &form, "https://oauth2.googleapis.com/token"])?;
        let access = token["access_token"]
            .as_str()
            .ok_or("no access token in the response")?;
        write_private(
            &self.dir.join("auth-header"),
            format!("Authorization: Bearer {access}").as_bytes(),
        )
    }

    fn get(&self, query: &str, page: Option<&str>) -> Result<Value> {
        let header = format!("@{}", self.dir.join("auth-header").display());
        let mut args = vec![
            "--get".to_string(),
            "-H".into(),
            header,
            "--data-urlencode".into(),
            format!("q={query}"),
            "--data-urlencode".into(),
            "fields=nextPageToken,files(id,name)".into(),
            "--data-urlencode".into(),
            "pageSize=1000".into(),
        ];
        if let Some(page) = page {
            args.extend(["--data-urlencode".into(), format!("pageToken={page}")]);
        }
        args.push(DRIVE_API.into());
        self.curl(&args.iter().map(String::as_str).collect::<Vec<_>>())
    }

    /// The id of the only folder named `name` inside `parent`.
    fn folder(&self, parent: &str, name: &str) -> Result<Option<String>> {
        let name = name.replace('\\', "\\\\").replace('\'', "\\'");
        let query = format!(
            "name = '{name}' and mimeType = '{FOLDER_MIME}' and '{parent}' in parents \
             and trashed = false"
        );
        let found = self.get(&query, None)?;
        match found["files"].as_array().map(Vec::as_slice) {
            Some([]) | None => Ok(None),
            Some([folder]) => Ok(folder["id"].as_str().map(str::to_string)),
            Some(_) => {
                Err(format!("more than one Drive folder named {name}; nothing deleted").into())
            }
        }
    }

    /// Deletes the files named `perf/...` in `EmuSync/<cloud_folder>`; returns how many.
    fn delete_synthetic(&self, cloud_folder: &str) -> Result<usize> {
        self.authorize()?;
        let Some(root) = self.folder("root", "EmuSync")? else {
            return Ok(0);
        };
        let Some(folder) = self.folder(&root, cloud_folder)? else {
            return Ok(0);
        };
        let query = format!("'{folder}' in parents and trashed = false");
        let mut doomed = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let listed = self.get(&query, page.as_deref())?;
            for file in listed["files"].as_array().into_iter().flatten() {
                if let (Some(id), Some(name)) = (file["id"].as_str(), file["name"].as_str())
                    && name.starts_with(PREFIX)
                {
                    doomed.push(id.to_string());
                }
            }
            match listed["nextPageToken"].as_str() {
                Some(next) => page = Some(next.to_string()),
                None => break,
            }
        }
        let header = format!("@{}", self.dir.join("auth-header").display());
        for id in &doomed {
            self.curl(&["-X", "DELETE", "-H", &header, &format!("{DRIVE_API}/{id}")])?;
        }
        Ok(doomed.len())
    }
}

impl Drop for DriveApi {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Percent-encodes a form value.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
