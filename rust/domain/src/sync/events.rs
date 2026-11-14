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
    pub reason: SyncMessage,
    pub completed_save_count: usize,
    pub pending_save_count: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncEvent {
    Idle,
    Syncing(SyncMessage),
    Playing,
    Error(SyncMessage),
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

/// What a synchronization tells the user, in no particular language: the interface puts
/// it into words. Values that come from elsewhere (names, paths, lower-level errors) are
/// carried as they are.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncMessage {
    // Progress.
    CheckingCloudSaves,
    ConnectingDrive,
    SyncingGameSaves {
        game: String,
    },
    UploadingGameSaves {
        game: String,
    },
    RestoringCloudSaves {
        game: String,
    },
    /// One save target of a category is being synchronized.
    SyncingTarget {
        name: String,
    },
    CheckingCloudFolder {
        title: String,
    },
    RenamingCloudFolder {
        title: String,
    },
    CheckingCloudFiles {
        title: String,
    },
    MovingCloudSaves {
        name: String,
    },
    // Problems.
    /// A step failed; `detail` is the lower-level error.
    Failed {
        detail: String,
    },
    /// The game did not start or could not be followed, so nothing was uploaded.
    GameNotTracked {
        game: String,
    },
    CategoryNameInvalid,
    DriveAuthFailed {
        detail: String,
    },
    CloudRenameFailed {
        detail: String,
    },
    /// A category's cloud folder is also another category's.
    SharedCloudFolder {
        folder: String,
    },
    DriveNotConnected,
    DriveConnectFailed {
        detail: String,
    },
    /// A file save's name is also another configured file save's.
    SaveNameTaken {
        name: String,
    },
    /// A file inside a folder save is also a configured file save.
    DirectorySaveConflict {
        path: String,
    },
    UnknownDriveError,
    // Save paths that cannot be matched with the cloud files.
    InvalidCloudPath {
        path: String,
    },
    ConflictingLocalDestinations,
    FileIsParentOfAnother,
    OverlappingDirectoryScopes,
    ConflictingRemoteToLocalPaths,
    /// Older save folders share a cloud location and these cloud files are in none.
    UnplacedLegacyFiles {
        folders: Vec<String>,
        files: Vec<String>,
    },
    LocalAccessFailed {
        detail: String,
    },
    /// A lower-level error with no message of its own.
    Detail(String),
    /// Several problems, one per line.
    Several(Vec<SyncMessage>),
}

impl SyncMessage {
    /// One message for `messages`, sorted and without repeats; a single one is itself.
    pub fn several(mut messages: Vec<SyncMessage>) -> SyncMessage {
        messages.sort();
        messages.dedup();
        if messages.len() == 1 {
            messages.remove(0)
        } else {
            SyncMessage::Several(messages)
        }
    }
}
