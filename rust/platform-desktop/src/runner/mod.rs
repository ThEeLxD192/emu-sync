pub mod browser;
pub mod launcher;
pub mod process;
pub mod tracker;
#[cfg(windows)]
#[expect(unsafe_code, reason = "std has no API to list running processes")]
mod windows_processes;

pub use browser::DesktopBrowserLauncher;
pub use launcher::{GameRunner, SteamLaunchOutcome, SteamUriLauncherFn};
pub use process::ProcessMonitor;
pub use tracker::{ExternalProcessTracker, ProcessLaunch, ProcessPolling};
