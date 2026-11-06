//! Save synchronization, mapping, lifecycle, and cloud migration.

pub mod channel;
pub mod context;
pub mod lifecycle;
pub mod manual;
pub mod migration;
pub mod migrator;
pub mod orchestrator;
pub mod post_game;
pub mod status;
pub mod target_mapping;
pub mod target_resolver;

pub use context::{SyncContext, SyncServices};
pub use lifecycle::GameSyncLifecycle;
pub use manual::{ManualEntrySync, ManualFileSync, PlannedSave};
pub use migration::CloudEntryMigration;
pub use migrator::{CloudFolderMigrator, MigrationError};
pub use orchestrator::{SyncOrchestrator, SyncOrchestratorFactory};
pub use post_game::{PostGameSync, PostSaveSync, PostSyncResult, SaveUploadTarget};
pub use status::SyncStatusChecker;
pub use target_mapping::{
    SaveTargetMapping, SaveTargetMappingError, directory_cloud_name, project_directory_files,
};
pub use target_resolver::{ResolvedSaveTarget, SaveTargetResolver};
