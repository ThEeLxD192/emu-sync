//! Top-level synchronization orchestrator implementing SaveSynchronizer and SaveSynchronizerFactory.

use super::context::{SyncContext, SyncServices};
use super::lifecycle::GameSyncLifecycle;
use super::manual::ManualEntrySync;
use super::migration::CloudEntryMigration;
use super::status::SyncStatusChecker;
use emusync_domain::config::{AppConfig, CloudAuthorization};
use emusync_domain::execution::GameLauncher;
use emusync_domain::library::{GameEntry, SaveTarget};
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, EntrySyncStatus, LocalSaveAccess, LocalSaveFiles,
    LocalSavePath, PostSyncFailure, SaveConflict, SaveSetup, SaveSynchronizer,
    SaveSynchronizerFactory, SyncEvent, SyncGame,
};
use std::sync::Arc;

pub struct SyncOrchestrator<A, L, F, T, Auth, R>
where
    A: LocalSaveAccess + Clone + Send + Sync + 'static,
    A::Path: Clone + Send + Sync + 'static,
    A::Error: std::fmt::Debug + Send + Sync + 'static,
    <A::Path as LocalSavePath>::Error: std::fmt::Debug + Send + Sync + 'static,
    L: CloudFileListing + Clone + Send + Sync + 'static,
    F: CloudFolderStore + Clone + Send + Sync + 'static,
    T: LocalSaveFiles + Clone + Send + Sync + 'static,
    Auth: CloudAuthorization + Clone + Send + Sync + 'static,
    R: GameLauncher + Clone + Send + Sync + 'static,
{
    context: Arc<SyncContext<A, L, F, T, Auth, R>>,
}

impl<A, L, F, T, Auth, R> SaveSynchronizer for SyncOrchestrator<A, L, F, T, Auth, R>
where
    A: LocalSaveAccess + Clone + Send + Sync + 'static,
    A::Path: Clone + Send + Sync + 'static,
    A::Error: std::fmt::Debug + Send + Sync + 'static,
    <A::Path as LocalSavePath>::Error: std::fmt::Debug + Send + Sync + 'static,
    L: CloudFileListing + Clone + Send + Sync + 'static,
    L::Error: std::fmt::Debug + Send + Sync + 'static,
    F: CloudFolderStore + Clone + Send + Sync + 'static,
    F::Error: std::fmt::Debug + Send + Sync + 'static,
    T: LocalSaveFiles + Clone + Send + Sync + 'static,
    T::Error: std::fmt::Debug + Send + Sync + 'static,
    Auth: CloudAuthorization + Clone + Send + Sync + 'static,
    Auth::Error: std::fmt::Display + Send + Sync + 'static,
    R: GameLauncher + Clone + Send + Sync + 'static,
    R::Error: std::fmt::Debug + Send + Sync + 'static,
{
    type Error = String;

    async fn check_entry(&self, entry: &GameEntry) -> Result<EntrySyncStatus, Self::Error> {
        SyncStatusChecker::new(&self.context)
            .check_entry_sync_status(entry)
            .await
    }

    async fn sync_entry(&self, entry: &GameEntry) -> Result<bool, Self::Error> {
        ManualEntrySync::new(&self.context).sync_entry(entry).await
    }

    async fn rename_game(
        &self,
        entry: &GameEntry,
        rom: Option<&str>,
        old_title: &str,
        new_title: &str,
        targets: &[SaveTarget],
    ) -> Result<bool, Self::Error> {
        CloudEntryMigration::new(&self.context)
            .rename_game_cloud_data(entry, rom, old_title, new_title, targets)
            .await
    }

    async fn rename_entry(&self, old: &GameEntry, new: &GameEntry) -> Result<bool, Self::Error> {
        CloudEntryMigration::new(&self.context)
            .rename_entry_cloud_data(old, new)
            .await
    }

    async fn play_with_sync(
        &self,
        game: SyncGame,
        setup: &(impl SaveSetup + Sync),
    ) -> Result<(), Self::Error> {
        GameSyncLifecycle::new(Arc::clone(&self.context))
            .play_with_sync(game, setup)
            .await
    }

    fn resolve_conflict(&self, request: &SaveConflict, decision: Decision) {
        self.context.resolve_conflict(request, decision);
    }

    async fn retry_post_sync(&self, failure: &PostSyncFailure) -> Result<(), Self::Error> {
        self.context.retry_post_sync(failure).await
    }
}

#[derive(Clone)]
pub struct SyncOrchestratorFactory<A, L, F, T, Auth, R> {
    services: SyncServices<A, L, F, T, Auth, R>,
}

impl<A, L, F, T, Auth, R> SyncOrchestratorFactory<A, L, F, T, Auth, R>
where
    A: LocalSaveAccess + Clone + Send + Sync + 'static,
    A::Path: Clone + Send + Sync + 'static,
    A::Error: std::fmt::Debug + Send + Sync + 'static,
    <A::Path as LocalSavePath>::Error: std::fmt::Debug + Send + Sync + 'static,
    L: CloudFileListing + Clone + Send + Sync + 'static,
    L::Error: std::fmt::Debug + Send + Sync + 'static,
    F: CloudFolderStore + Clone + Send + Sync + 'static,
    F::Error: std::fmt::Debug + Send + Sync + 'static,
    T: LocalSaveFiles + Clone + Send + Sync + 'static,
    T::Error: std::fmt::Debug + Send + Sync + 'static,
    Auth: CloudAuthorization + Clone + Send + Sync + 'static,
    Auth::Error: std::fmt::Display + Send + Sync + 'static,
    R: GameLauncher + Clone + Send + Sync + 'static,
    R::Error: std::fmt::Debug + Send + Sync + 'static,
{
    pub fn new(services: SyncServices<A, L, F, T, Auth, R>) -> Self {
        Self { services }
    }
}

impl<A, L, F, T, Auth, R> SaveSynchronizerFactory for SyncOrchestratorFactory<A, L, F, T, Auth, R>
where
    A: LocalSaveAccess + Clone + Send + Sync + 'static,
    A::Path: Clone + Send + Sync + 'static,
    A::Error: std::fmt::Debug + Send + Sync + 'static,
    <A::Path as LocalSavePath>::Error: std::fmt::Debug + Send + Sync + 'static,
    L: CloudFileListing + Clone + Send + Sync + 'static,
    L::Error: std::fmt::Debug + Send + Sync + 'static,
    F: CloudFolderStore + Clone + Send + Sync + 'static,
    F::Error: std::fmt::Debug + Send + Sync + 'static,
    T: LocalSaveFiles + Clone + Send + Sync + 'static,
    T::Error: std::fmt::Debug + Send + Sync + 'static,
    Auth: CloudAuthorization + Clone + Send + Sync + 'static,
    Auth::Error: std::fmt::Display + Send + Sync + 'static,
    R: GameLauncher + Clone + Send + Sync + 'static,
    R::Error: std::fmt::Debug + Send + Sync + 'static,
{
    type Synchronizer = SyncOrchestrator<A, L, F, T, Auth, R>;

    fn create(
        &self,
        config: AppConfig,
        on_status: impl Fn(SyncEvent) + Send + Sync + 'static,
    ) -> Self::Synchronizer {
        let services = SyncServices {
            files: self.services.files.clone(),
            search: self.services.search.clone(),
            folders: self.services.folders.clone(),
            tracked_sync: self.services.tracked_sync.clone(),
            authorization: self.services.authorization.clone(),
            runner: self.services.runner.clone(),
        };
        SyncOrchestrator {
            context: Arc::new(SyncContext::new(config, services, on_status)),
        }
    }
}
