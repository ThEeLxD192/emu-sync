//! Runtime ownership and desktop application composition.
mod supervisor;
pub use supervisor::{OperationFailure, OperationSupervisor, StartError};

mod retained_files;
pub use retained_files::{FailureClass, RetainedFiles, RetainedFilesError};

mod guarded_files;
pub use guarded_files::GuardedFiles;

mod drive_recovery;
pub use drive_recovery::{
    AuthorizedTransferFailure, PersistedRecoveryStatus, RecoveryError, RecoveryStatus,
    reconcile_conditional_upload, reconcile_created_upload, reconcile_persisted_uploads,
    reconcile_tracked_upload, reconcile_tracked_upload_with_listing, start_authorized_transfer,
};

mod runtime_owner;
pub use runtime_owner::{CloseState, Closing, RecoveryPanicked, RecoveryStartError, RuntimeOwner};

pub mod coordinator;
pub use coordinator::{DesktopCoordinator, DriveEndpoints, resolve_effective_targets};

pub mod events;
pub mod language;
pub use events::{AppEvent, EventSender, apply_entry_games_scanned, apply_steam_state_refreshed};
pub use language::CurrentLanguage;

pub mod scanner_helper;
pub mod target_types;
pub use scanner_helper::scan_entry_games;
