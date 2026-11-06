//! Runtime ownership for the desktop application.
mod supervisor;
pub use supervisor::{OperationFailure, OperationSupervisor, StartError};

mod drive_recovery;
pub use drive_recovery::{
    AuthorizedTransferFailure, PersistedRecoveryStatus, RecoveryError, RecoveryStatus,
    reconcile_conditional_upload, reconcile_created_upload, reconcile_persisted_uploads,
    reconcile_tracked_upload, reconcile_tracked_upload_with_listing, start_authorized_transfer,
};

mod runtime_owner;
pub use runtime_owner::{CloseState, Closing, RecoveryPanicked, RecoveryStartError, RuntimeOwner};
