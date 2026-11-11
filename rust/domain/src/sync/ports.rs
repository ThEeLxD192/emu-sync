use super::decision::{Decision, RemoteState, Timestamp};
use super::events::{
    EntrySyncStatus, PostSyncFailure, SaveConflict, SavePlacementRequest, SaveSetupRequest,
    SaveSetupResult, SyncEvent, SyncGame,
};
use super::remote::RemoteFile;

/// Cancellation is cooperative: implementations finish cleanup and remote rollback before returning.
pub trait CloudSaveTransfer {
    type Error;
    fn download(
        &self,
        token: &str,
        file_id: &str,
        destination: &str,
        modified: Option<Timestamp>,
        progress_name: &str,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn upload(
        &self,
        token: &str,
        source: &str,
        remote_name: &str,
        parent: &str,
    ) -> impl std::future::Future<Output = Result<RemoteState, Self::Error>> + Send;
    /// Replace only the observed revision; precondition failure must not fall back to unconditional upload.
    fn replace(
        &self,
        token: &str,
        observed: &RemoteState,
        source: &str,
        remote_name: &str,
    ) -> impl std::future::Future<Output = Result<RemoteState, Self::Error>> + Send;
}
pub trait CloudFileListing {
    type Error;
    fn list_files(
        &self,
        token: &str,
        folder: &str,
    ) -> impl std::future::Future<Output = Result<Vec<RemoteFile>, Self::Error>> + Send;
    fn list_children(
        &self,
        token: &str,
        folder: &str,
    ) -> impl std::future::Future<Output = Result<Vec<RemoteFile>, Self::Error>> + Send;
}
pub trait CloudFolderStore {
    type Error;
    fn ensure_path(
        &self,
        token: &str,
        segments: &[String],
    ) -> impl std::future::Future<Output = Result<String, Self::Error>> + Send;
    fn find_path(
        &self,
        token: &str,
        segments: &[String],
    ) -> impl std::future::Future<Output = Result<Option<String>, Self::Error>> + Send;
    fn rename_folder(
        &self,
        token: &str,
        folder: &str,
        name: &str,
        add_parent: Option<&str>,
        remove_parent: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn move_file(
        &self,
        token: &str,
        file: &str,
        add_parent: &str,
        remove_parent: &str,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
}
/// Filesystem observations and safe resolution are supplied by a local adapter.
pub trait LocalSavePath: Sized {
    type Error;
    fn path(&self) -> &str;
    fn name(&self) -> &str;
    fn absolute_path(&self) -> Result<String, Self::Error>;
    fn normalized_path(&self) -> Result<String, Self::Error>;
    fn ancestors(&self) -> Result<Vec<String>, Self::Error>;
    fn is_file(&self) -> Result<bool, Self::Error>;
    fn is_directory(&self) -> Result<bool, Self::Error>;
    fn exists(&self) -> Result<bool, Self::Error>;
    fn last_modified_ms(&self) -> Result<i64, Self::Error>;
    fn resolve(&self, relative: &str) -> Result<Self, Self::Error>;
    fn resolve_remote(&self, name: &str) -> Result<Option<Self>, Self::Error>;
    fn walk_top_down(&self) -> Result<Vec<Self>, Self::Error>;
    /// Always uses '/' separators.
    fn relative_to(&self, root: &Self) -> Result<String, Self::Error>;
}
pub trait LocalSaveAccess {
    type Error;
    type Path: LocalSavePath;
    fn path(&self, path: &str) -> Self::Path;
    fn effective_targets(
        &self,
        entry: &crate::library::GameEntry,
    ) -> Result<Vec<crate::library::SaveTarget>, Self::Error>;
    fn effective_targets_for_rom(
        &self,
        entry: &crate::library::Emulator,
        rom: &str,
    ) -> Result<Vec<crate::library::SaveTarget>, Self::Error>;
}
/// A remote file projected beneath a user-selected local directory.
#[derive(Clone, Copy, Debug)]
pub struct DirectoryDownload<'a> {
    pub token: &'a str,
    pub remote: &'a RemoteState,
    pub local: &'a str,
    pub root: &'a str,
    pub relative: &'a str,
    pub folder: &'a str,
    pub remote_name: &'a str,
}

/// Local history, snapshots and backup handling around transfers.
pub trait LocalSaveFiles {
    type Error;
    fn plan(
        &self,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
    ) -> impl std::future::Future<Output = Result<Decision, Self::Error>> + Send;
    fn remember(
        &self,
        local: &str,
        remote: &RemoteState,
        folder: &str,
        remote_name: &str,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    fn download(
        &self,
        token: &str,
        remote: &RemoteState,
        local: &str,
        folder: &str,
        remote_name: &str,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    /// Carries the selected root separately from the remote-derived relative path.
    fn download_in_directory(
        &self,
        request: DirectoryDownload<'_>,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send
    where
        Self: Sync,
    {
        async move {
            self.download(
                request.token,
                request.remote,
                request.local,
                request.folder,
                request.remote_name,
            )
            .await
        }
    }
    fn upload(
        &self,
        token: &str,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
}
pub trait SaveSetup {
    type Error;
    fn request(
        &self,
        request: SaveSetupRequest,
    ) -> impl std::future::Future<Output = Result<SaveSetupResult, Self::Error>> + Send;
}
/// The owner must not drop a running operation on cancellation; it must await its cleanup.
pub trait SaveSynchronizer {
    type Error;
    fn check_entry(
        &self,
        entry: &crate::library::GameEntry,
    ) -> impl std::future::Future<Output = Result<EntrySyncStatus, Self::Error>> + Send;
    fn sync_entry(
        &self,
        entry: &crate::library::GameEntry,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send;
    fn rename_game(
        &self,
        entry: &crate::library::GameEntry,
        rom: Option<&str>,
        old_title: &str,
        new_title: &str,
        targets: &[crate::library::SaveTarget],
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send;
    fn rename_entry(
        &self,
        old: &crate::library::GameEntry,
        new: &crate::library::GameEntry,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send;
    fn play_with_sync(
        &self,
        game: SyncGame,
        setup: &(impl SaveSetup + Sync),
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
    /// Only the exact pending request is resolved; completed requests are ignored.
    fn resolve_conflict(&self, request: &SaveConflict, decision: Decision);
    /// Answers a pending folder choice with one folder index per group, or `None` to
    /// cancel; like conflicts, only the exact pending request is resolved.
    fn resolve_save_placement(&self, request: &SavePlacementRequest, choice: Option<Vec<usize>>);
    /// A stale failure must not replay a newer batch.
    fn retry_post_sync(
        &self,
        failure: &PostSyncFailure,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
}

pub trait SaveSynchronizerFactory {
    type Synchronizer: SaveSynchronizer;
    fn create(
        &self,
        config: crate::config::AppConfig,
        on_status: impl Fn(SyncEvent) + Send + Sync + 'static,
    ) -> Self::Synchronizer;
}
