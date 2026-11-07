//! Application coordination, library editing, update lifecycles, and synchronization workflows.

pub mod connection;
pub mod library;
pub mod operation;
pub mod sync;
pub mod update;

pub use connection::{CloudConnection, ConnectionResult};
pub use library::{
    EditLibrary, EntryEditResult, EntrySyncChecks, GameOverrideChanges, LoadLibrary,
    SteamRegistration, SteamRegistrationError, build_game_entry,
};
pub use operation::{OperationCoordinator, OperationEvent};
pub use sync::{
    CloudEntryMigration, CloudFolderMigrator, ManualEntrySync, ManualFileSync, MigrationError,
    PlannedSave, SaveTargetMapping, SaveTargetMappingError, SaveTargetResolver, SyncContext,
    SyncOrchestrator, SyncOrchestratorFactory, SyncServices,
};
pub use update::{UpdateCoordinator, UpdateState};
