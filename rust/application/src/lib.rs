//! Application coordination, library editing, update lifecycles, and synchronization workflows.

pub mod connection;
pub mod library;
pub mod operation;
pub mod sync;
pub mod update;

pub use connection::{CloudConnection, ConnectionResult};
pub use library::{
    EditLibrary, EntryEditResult, EntrySyncChecks, GameOverrideChanges, LoadLibrary,
    SteamRegistration, SteamRegistrationError, SyncCheckRequest, build_game_entry,
    preserve_shared_cloud_root,
};
pub use operation::{OperationCoordinator, OperationEvent};
pub use sync::{
    CloudEntryMigration, CloudFolderMigrator, GameSyncLifecycle, ManualEntrySync, ManualFileSync,
    MigrationError, PlannedSave, PostGameSync, PostSaveSync, PostSyncResult, ResolvedSaveTarget,
    SaveTargetMapping, SaveTargetMappingError, SaveTargetResolver, SaveUploadTarget, SyncContext,
    SyncOrchestrator, SyncOrchestratorFactory, SyncServices, SyncStatusChecker,
};
pub use update::{UpdateCoordinator, UpdateState};
