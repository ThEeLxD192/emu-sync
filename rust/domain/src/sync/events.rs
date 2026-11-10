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
