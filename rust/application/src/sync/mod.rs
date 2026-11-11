//! Save synchronization, mapping, lifecycle, and cloud migration.

mod channel;
mod context;
mod legacy_placement;
mod lifecycle;
mod manual;
mod migration;
mod migrator;
mod orchestrator;
mod post_game;
mod status;
mod target_mapping;
mod target_resolver;

pub use channel::{OneshotReceiver, OneshotSender, oneshot};
pub use context::{SyncContext, SyncServices};
pub use manual::{ManualEntrySync, ManualFileSync, PlannedSave};
pub use migration::CloudEntryMigration;
pub use migrator::{CloudFolderMigrator, MigrationError};
pub use orchestrator::{SyncOrchestrator, SyncOrchestratorFactory};
pub use target_mapping::{
    SaveTargetMapping, SaveTargetMappingError, UnplacedLegacyFiles, project_directory_files,
};
pub use target_resolver::{ResolvedSaveTarget, SaveTargetResolver};
