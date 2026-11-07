//! Local persistence and filesystem adapters.

mod config;
mod config_dto;
mod history;
mod location;
mod recovery_store;
mod restore_paths;
mod save_access;
mod staging;
mod storage;
mod targets;
mod tracked;
mod tracked_commit;

pub use config::ConfigManager;
pub use history::{HistoryCheckpoint, SyncHistoryStore, file_sha256};
pub use location::resolve_config_location;
pub use recovery_store::{
    ObservedRemote, RecoveryEntry, RecoveryRecord, RecoveryState, RecoveryStore, Unverifiable,
};
pub use restore_paths::resolve_remote_file;
pub use save_access::{DesktopSaveAccess, DesktopSavePath};
pub use staging::{StagedRestore, UploadEvidence, UploadSnapshot};
pub use targets::LocalSaveTargets;
pub use tracked::{
    RecoveryCredentials, TrackedError, TrackedFileSync, UploadContext, UploadRequest,
};

#[cfg(target_os = "linux")]
mod pinned_path;
#[cfg(target_os = "linux")]
pub use pinned_path::PinnedRemoteDestination;
