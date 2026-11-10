use crate::{update::installer::AppImageInstaller, util::AppImageEnvironment};
use emusync_domain::update::UpdateInstaller;
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

pub type ExitFn = Arc<dyn Fn(i32) + Send + Sync>;
pub type WindowsLauncherFn = Arc<dyn Fn(&[&str]) -> io::Result<()> + Send + Sync>;
pub type LinuxLauncherFn = Arc<dyn Fn(&str) -> io::Result<()> + Send + Sync>;

#[derive(Clone)]
pub struct DesktopUpdateInstaller {
    is_windows: bool,
    current_app_image_path: Option<PathBuf>,
    steam_game_mode_override: Option<bool>,
    exit_application: ExitFn,
    windows_launcher: WindowsLauncherFn,
    linux_launcher: LinuxLauncherFn,
    app_image_installer: Arc<AppImageInstaller>,
}

impl Default for DesktopUpdateInstaller {
    fn default() -> Self {
        let is_windows = cfg!(windows);
        let current_app_image_path = std::env::var("APPIMAGE")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from);

        Self {
            is_windows,
            current_app_image_path,
            steam_game_mode_override: None,
            // Salida abrupta: `desktop-app` debe inyectar una salida ordenada con
            // `with_exit_application` (cierre de recursos y tareas).
            exit_application: Arc::new(|code| std::process::exit(code)),
            windows_launcher: Arc::new(default_windows_launcher),
            linux_launcher: Arc::new(default_linux_launcher),
            app_image_installer: Arc::new(AppImageInstaller::new()),
        }
    }
}

fn default_windows_launcher(args: &[&str]) -> io::Result<()> {
    if args.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Empty command"));
    }
    Command::new(args[0]).args(&args[1..]).spawn()?;
    Ok(())
}

fn default_linux_launcher(path: &str) -> io::Result<()> {
    let mut cmd = Command::new(path);
    if let Ok(null_in) = File::open("/dev/null") {
        cmd.stdin(null_in);
    }
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    AppImageEnvironment::clean_command(&mut cmd);

    cmd.spawn()?;
    Ok(())
}

impl DesktopUpdateInstaller {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_exit_application(mut self, exit_fn: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.exit_application = Arc::new(exit_fn);
        self
    }

    pub fn with_is_windows(mut self, is_windows: bool) -> Self {
        self.is_windows = is_windows;
        self
    }

    pub fn with_current_app_image_path(mut self, path: Option<PathBuf>) -> Self {
        self.current_app_image_path = path;
        self
    }

    pub fn with_steam_game_mode(mut self, is_game_mode: bool) -> Self {
        self.steam_game_mode_override = Some(is_game_mode);
        self
    }

    pub fn with_windows_launcher(
        mut self,
        launcher: impl Fn(&[&str]) -> io::Result<()> + Send + Sync + 'static,
    ) -> Self {
        self.windows_launcher = Arc::new(launcher);
        self
    }

    pub fn with_linux_launcher(
        mut self,
        launcher: impl Fn(&str) -> io::Result<()> + Send + Sync + 'static,
    ) -> Self {
        self.linux_launcher = Arc::new(launcher);
        self
    }

    pub fn with_app_image_installer(mut self, installer: AppImageInstaller) -> Self {
        self.app_image_installer = Arc::new(installer);
        self
    }

    /// `Ok(false)` solo significa "no aplica" (archivo inexistente, extensión no instalable o
    /// sin `APPIMAGE`); los fallos de instalación o relanzamiento se propagan.
    ///
    /// Tras aplicar la actualización invoca `exit_application`; por defecto termina el proceso
    /// de forma abrupta, así que `desktop-app` debe inyectar una salida ordenada.
    pub fn apply_update_and_restart(&self, downloaded_file: &Path) -> io::Result<bool> {
        if !downloaded_file.exists() {
            return Ok(false);
        }

        let applied = if self.is_windows {
            self.apply_windows_update_and_restart(downloaded_file)?
        } else {
            self.apply_linux_update_and_restart(downloaded_file)?
        };

        if applied {
            (self.exit_application)(0);
        }
        Ok(applied)
    }

    pub fn apply_windows_update_and_restart(&self, downloaded_file: &Path) -> io::Result<bool> {
        let name = downloaded_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        let path_str = downloaded_file.to_string_lossy();

        if name.ends_with(".msi") {
            (self.windows_launcher)(&["msiexec", "/i", &path_str])?;
        } else if name.ends_with(".exe") {
            (self.windows_launcher)(&[&path_str])?;
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    pub fn apply_linux_update_and_restart(&self, downloaded_file: &Path) -> io::Result<bool> {
        let Some(current_path) = self.current_app_image_path.as_deref() else {
            return Ok(false);
        };

        if !current_path.exists() || !downloaded_file.exists() {
            return Ok(false);
        }

        let in_game_mode = self.is_steam_game_mode().unwrap_or(false);
        let launcher = self.linux_launcher.clone();
        let current_path_str = current_path.to_string_lossy().to_string();

        self.app_image_installer
            .replace(downloaded_file, current_path, move || {
                if !in_game_mode {
                    launcher(&current_path_str)?;
                }
                Ok(())
            })?;
        Ok(true)
    }
}

impl UpdateInstaller for DesktopUpdateInstaller {
    type Error = io::Error;

    fn is_steam_game_mode(&self) -> Result<bool, Self::Error> {
        if let Some(mode) = self.steam_game_mode_override {
            return Ok(mode);
        }
        let gamepad_ui = std::env::var("SteamGamepadUI").as_deref() == Ok("1");
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        let is_gamescope = desktop == "gamescope";
        Ok(gamepad_ui || is_gamescope)
    }

    fn restart(&self, path: &str) -> Result<bool, Self::Error> {
        self.apply_update_and_restart(Path::new(path))
    }
}
