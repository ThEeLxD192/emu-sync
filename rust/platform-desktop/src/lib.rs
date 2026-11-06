pub mod cover;
pub mod picker;
pub mod runner;
pub mod scanner;
pub mod steam;
pub mod update;
pub mod util;

pub use cover::DesktopCoverFiles;
pub use picker::{ExternalChooserResult, FilePicker, run_external_choosers};
pub use runner::{
    DesktopBrowserLauncher, ExternalProcessTracker, GameRunner, ProcessLaunch, ProcessMonitor,
    ProcessPolling, SteamLaunchOutcome, SteamUriLauncherFn,
};
pub use scanner::{RomFile, scan_roms};
pub use steam::{
    BinaryVdf, DesktopSteamLibrary, SteamShortcut, SteamShortcutManager, VdfValue,
    WindowsLaunchConfiguration,
};
pub use update::{AppImageInstaller, DesktopUpdateInstaller, UpdateStagingFiles};
pub use util::{AppImageEnvironment, shell_argument};
