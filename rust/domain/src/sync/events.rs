#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncGame {
    pub name: String,
    pub entry: crate::library::GameEntry,
    pub rom_path: Option<String>,
    pub effective_title: String,
    pub save_targets: Vec<crate::library::SaveTarget>,
}
/// Unique within the synchronizer lifetime, even when requests have equal contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveConflict {
    pub request_id: RequestId,
    pub local_modified_ms: i64,
    pub cloud_modified_ms: i64,
    pub game_name: String,
}
/// Cloud files that share a first path segment and that no local save folder holds yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavePlacementGroup {
    pub name: String,
    pub files: Vec<String>,
    /// Folder whose path resembles `name`; only a preselection for the user.
    pub suggested_folder: Option<usize>,
}
/// Asks which of a game's folders sharing one cloud location receives each group.
/// The answer holds one index into `folders` per group, or nothing when cancelled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavePlacementRequest {
    pub request_id: RequestId,
    pub game_name: String,
    pub folders: Vec<String>,
    pub groups: Vec<SavePlacementGroup>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostSyncFailure {
    pub request_id: RequestId,
    pub game_name: String,
    pub reason: String,
    pub completed_save_count: usize,
    pub pending_save_count: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncEvent {
    Idle,
    Syncing(String),
    Playing,
    Error(String),
    Conflict(SaveConflict),
    ChooseSaveFolders(SavePlacementRequest),
    PostSyncFailed(PostSyncFailure),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntrySyncStatus {
    Idle,
    InSync,
    OutOfSync,
    Conflict,
    NotConfigured,
    Error,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveSetupRequest {
    pub game: SyncGame,
    pub cloud_file_names: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveSetupResult {
    pub game: SyncGame,
    pub targets: Vec<crate::library::SaveTarget>,
}
