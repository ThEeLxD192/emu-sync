pub mod browser;
pub mod launcher;
pub mod process;
pub mod tracker;
#[cfg(windows)]
mod windows_processes;

pub use browser::DesktopBrowserLauncher;
pub use launcher::{GameRunner, SteamLaunchOutcome, SteamUriLauncherFn};
pub use process::ProcessMonitor;
pub use tracker::{ExternalProcessTracker, ProcessLaunch, ProcessPolling};
