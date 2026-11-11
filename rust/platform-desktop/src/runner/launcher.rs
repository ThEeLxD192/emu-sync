use crate::{
    runner::{
        process::ProcessMonitor,
        tracker::{ExternalProcessTracker, ProcessLaunch, ProcessPolling},
    },
    steam::manager::SteamShortcutManager,
    util::AppImageEnvironment,
};
use emusync_domain::{
    execution::{
        GameLaunchRequest, GameLauncher, GameResult, SteamLaunchPreparation, parse_arguments,
    },
    library::{Emulator, GameEntry},
};
use std::{
    collections::HashSet,
    future::Future,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::process::Command;

pub struct SteamLaunchOutcome {
    pub exit_code: Option<i32>,
    pub is_alive: bool,
    pub handoff_succeeded: bool,
}

/// Tiempo máximo que el lanzador por defecto espera al comando que abre la URI de Steam.
const STEAM_HANDOFF_TIMEOUT: Duration = Duration::from_secs(10);
const STEAM_HANDOFF_POLL: Duration = Duration::from_millis(50);

fn missing_rom_error(emu: &Emulator) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "A ROM file must be provided to launch an Emulator ('{}')",
            emu.common.name
        ),
    )
}

async fn matching_process_ids_blocking(name: &str) -> HashSet<u64> {
    let name = name.to_string();
    tokio::task::spawn_blocking(move || ProcessMonitor::matching_process_ids(&name))
        .await
        .unwrap_or_default()
}

pub type SteamUriLauncherFn = Arc<dyn Fn(&str) -> Option<SteamLaunchOutcome> + Send + Sync>;

#[derive(Clone)]
pub struct GameRunner<S> {
    steam_launches: Arc<S>,
    tracker: ExternalProcessTracker,
    steam_uri_launcher: SteamUriLauncherFn,
    polling: ProcessPolling,
}

impl<S> GameRunner<S>
where
    S: SteamLaunchPreparation + Send + Sync + 'static,
    S::Error: std::fmt::Display,
{
    pub fn new(steam_launches: Arc<S>) -> Self {
        let tracker = ExternalProcessTracker::new(
            Arc::new(ProcessMonitor::matching_process_ids),
            Arc::new(ProcessMonitor::is_steam_shader_compiling),
            Arc::new(ProcessMonitor::is_process_alive),
        );
        Self {
            steam_launches,
            tracker,
            steam_uri_launcher: Arc::new(default_steam_uri_launcher),
            polling: ProcessPolling::default(),
        }
    }

    pub fn with_tracker(steam_launches: Arc<S>, tracker: ExternalProcessTracker) -> Self {
        Self {
            steam_launches,
            tracker,
            steam_uri_launcher: Arc::new(default_steam_uri_launcher),
            polling: ProcessPolling::default(),
        }
    }

    pub fn with_steam_uri_launcher(mut self, launcher: SteamUriLauncherFn) -> Self {
        self.steam_uri_launcher = launcher;
        self
    }

    pub fn with_polling(mut self, polling: ProcessPolling) -> Self {
        self.polling = polling;
        self
    }

    pub async fn open_emulator(&self, entry: &Emulator) -> io::Result<tokio::process::Child> {
        let mut cmd = Command::new(&entry.common.executable_path);
        let args: Vec<&str> = entry
            .common
            .arguments
            .iter()
            .filter(|arg| !arg.contains("{ROM}"))
            .map(|s| s.as_str())
            .collect();
        cmd.args(args);

        if let Some(parent) = Path::new(&entry.common.executable_path).parent()
            && parent.exists()
        {
            cmd.current_dir(parent);
        }

        AppImageEnvironment::clean_command(cmd.as_std_mut());

        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());

        cmd.spawn()
    }

    fn resolve_process_name(entry: &GameEntry) -> Option<String> {
        match entry {
            GameEntry::Emulator(emu) => emu.steam_process_name.clone().or_else(|| {
                Path::new(&emu.common.executable_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_string)
            }),
            GameEntry::Native(nat) => nat.wait_for_process.clone().or_else(|| {
                Path::new(&nat.common.executable_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_string)
            }),
        }
    }

    fn build_command(
        entry: &GameEntry,
        rom_path: Option<&str>,
    ) -> io::Result<(String, Vec<String>, PathBuf)> {
        match entry {
            GameEntry::Emulator(emu) => {
                let rom = rom_path.ok_or_else(|| missing_rom_error(emu))?;

                let mut args = Vec::new();
                let fs_args = SteamShortcutManager::resolve_fullscreen_args(emu);
                if !fs_args.trim().is_empty() {
                    let parsed = parse_arguments(&fs_args).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Fullscreen arguments contain unmatched quotes",
                        )
                    })?;
                    args.extend(parsed);
                }

                for arg in &emu.common.arguments {
                    args.push(arg.replace("{ROM}", rom));
                }

                let working_dir = working_directory_of(Path::new(rom));

                Ok((emu.common.executable_path.clone(), args, working_dir))
            }
            GameEntry::Native(nat) => {
                let working_dir = working_directory_of(Path::new(&nat.common.executable_path));

                Ok((
                    nat.common.executable_path.clone(),
                    nat.common.arguments.clone(),
                    working_dir,
                ))
            }
        }
    }

    async fn launch_direct(&self, entry: &GameEntry, rom_path: Option<&str>) -> io::Result<bool> {
        let (exe, args, working_dir) = Self::build_command(entry, rom_path)?;

        let wait_for_process = match entry {
            GameEntry::Native(n) => n.wait_for_process.as_deref(),
            _ => None,
        };

        let existing_matches: HashSet<u64> = match wait_for_process {
            Some(name) => matching_process_ids_blocking(name).await,
            None => HashSet::new(),
        };

        let mut cmd = Command::new(&exe);
        cmd.args(args);
        cmd.current_dir(working_dir);

        if matches!(entry, GameEntry::Emulator(_)) {
            AppImageEnvironment::clean_command(cmd.as_std_mut());
        }

        let mut child = cmd.spawn()?;

        if let Some(proc_name) = wait_for_process {
            // El hijo directo suele ser un intermediario (launcher/wrapper) cuyo resultado no
            // decide el éxito: se recoge en segundo plano para que no quede zombi.
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            let launch = ProcessLaunch {
                name: proc_name.to_string(),
                existing_ids: existing_matches,
                request_exit_code: None,
                request_is_alive: true,
                desktop_handoff_succeeded: false,
            };
            let success = self.tracker.wait_for_process(launch, self.polling).await;
            Ok(success)
        } else {
            let status = child.wait().await?;
            Ok(status.success())
        }
    }

    async fn launch_via_steam(
        &self,
        entry: &GameEntry,
        rom_path: Option<&str>,
        app_id: i32,
    ) -> io::Result<bool> {
        if let GameEntry::Emulator(emu) = entry {
            let rom = rom_path.ok_or_else(|| missing_rom_error(emu))?;
            self.steam_launches
                .prepare(emu, rom)
                .map_err(|e| io::Error::other(format!("Could not prepare Steam launch: {e}")))?;
        }

        let unsigned_app_id = app_id as u32 as u64;
        let game_id = (unsigned_app_id << 32) | 0x02000000;
        let uri = format!("steam://rungameid/{game_id}");

        let process_name = Self::resolve_process_name(entry);
        let existing_matches = match process_name.as_deref() {
            Some(name) => matching_process_ids_blocking(name).await,
            None => HashSet::new(),
        };

        let launcher = Arc::clone(&self.steam_uri_launcher);
        let handoff = tokio::task::spawn_blocking(move || launcher(&uri))
            .await
            .map_err(|e| io::Error::other(format!("Steam launch task failed: {e}")))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Steam could not be opened"))?;

        if let Some(name) = process_name {
            let launch = ProcessLaunch {
                name,
                existing_ids: existing_matches,
                request_exit_code: handoff.exit_code,
                request_is_alive: handoff.is_alive,
                desktop_handoff_succeeded: handoff.handoff_succeeded,
            };
            let success = self.tracker.wait_for_process(launch, self.polling).await;
            Ok(success)
        } else {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            Ok(true)
        }
    }
}

/// Espera al comando de traspaso hasta `STEAM_HANDOFF_TIMEOUT`. Si sigue vivo (p. ej. `steam`
/// arrancó Steam y permanece en ejecución) lo da por traspasado y lo recoge en un hilo aparte.
fn await_handoff(mut child: std::process::Child) -> Option<SteamLaunchOutcome> {
    let deadline = Instant::now() + STEAM_HANDOFF_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Some(SteamLaunchOutcome {
                    exit_code: status.code(),
                    is_alive: false,
                    handoff_succeeded: status.success(),
                });
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(STEAM_HANDOFF_POLL),
            Ok(None) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Some(SteamLaunchOutcome {
                    exit_code: None,
                    is_alive: true,
                    handoff_succeeded: true,
                });
            }
            Err(_) => {
                return Some(SteamLaunchOutcome {
                    exit_code: None,
                    is_alive: false,
                    handoff_succeeded: false,
                });
            }
        }
    }
}

/// Bloqueante: debe invocarse desde `spawn_blocking`.
fn default_steam_uri_launcher(uri: &str) -> Option<SteamLaunchOutcome> {
    if cfg!(windows) {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/c", "start", uri]);
        if let Ok(mut child) = cmd.spawn() {
            let status = child.wait().ok();
            return Some(SteamLaunchOutcome {
                exit_code: status.and_then(|s| s.code()),
                is_alive: false,
                handoff_succeeded: status.is_some_and(|s| s.success()),
            });
        }
    } else {
        let mut cmd = std::process::Command::new("steam");
        cmd.arg(uri);
        AppImageEnvironment::clean_command(&mut cmd);
        if let Ok(child) = cmd.spawn() {
            return await_handoff(child);
        }
        let mut xdg = std::process::Command::new("xdg-open");
        xdg.arg(uri);
        AppImageEnvironment::clean_command(&mut xdg);
        if let Ok(child) = xdg.spawn() {
            return await_handoff(child);
        }
    }
    None
}

impl<S> GameLauncher for GameRunner<S>
where
    S: SteamLaunchPreparation + Send + Sync + 'static,
    S::Error: std::fmt::Display,
{
    type Error = io::Error;

    fn launch(
        &self,
        request: GameLaunchRequest,
    ) -> impl Future<Output = Result<GameResult, Self::Error>> + Send {
        let runner = self;
        let entry = request.entry;
        let rom_path = request.rom_path;
        let steam_app_id = request.steam_app_id;

        async move {
            let start = Instant::now();
            let success = if let Some(app_id) = steam_app_id {
                runner
                    .launch_via_steam(&entry, rom_path.as_deref(), app_id)
                    .await?
            } else {
                runner.launch_direct(&entry, rom_path.as_deref()).await?
            };

            let duration_ms = start.elapsed().as_millis() as i64;
            Ok(GameResult {
                exit_code: if success { 0 } else { 1 },
                duration_ms,
            })
        }
    }
}

/// Directory containing `path`, or the current directory for a bare name such as a
/// command resolved through `PATH` (`Path::parent` returns an empty path for it, which
/// cannot be a working directory). Matches Kotlin's `parentFile ?: File(".")`.
fn working_directory_of(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}
