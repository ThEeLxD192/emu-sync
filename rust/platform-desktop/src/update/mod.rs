pub mod desktop;
pub mod installer;
pub mod staging;

pub use desktop::DesktopUpdateInstaller;
pub use installer::{AppImageInstaller, move_replacing};
pub use staging::UpdateStagingFiles;
