//! UI actions and events communicating user intentions to background handlers.

use crate::{
    forms::game_edit_form::RestoreChoice,
    model::{GameItem, PathTarget, PickMode, SaveSetupRequest},
};
use emusync_domain::{
    library::{Emulator, GameEntry, SaveTarget},
    sync::Decision,
    update::UpdateInfo,
};
use std::path::PathBuf;

/// Commands issued by the presentation layer for application/background execution.
#[derive(Clone, Debug, PartialEq)]
pub enum UiAction {
    SelectEntry(Option<GameEntry>),
    LaunchGame(GameItem),
    OpenEmulator(Emulator),
    SyncEntry(GameEntry),
    CheckSyncEntry(GameEntry),
    AddEntry(GameEntry),
    EditEntry {
        old: Box<GameEntry>,
        new: Box<GameEntry>,
    },
    DeleteEntry(GameEntry),
    ReorderEntries(Vec<GameEntry>),
    /// Opens the game dialog once the game's own save targets have been read.
    EditGame(GameItem),
    EditGameOverride {
        item: GameItem,
        title: Option<String>,
        targets: Vec<SaveTarget>,
        cover_path: Option<String>,
    },
    CompleteSaveSetup {
        request: Box<SaveSetupRequest>,
        targets: Vec<SaveTarget>,
        title: Option<String>,
        cover_path: Option<String>,
        /// Restore from the cloud into this folder instead of `targets`.
        restore: Option<RestoreChoice>,
    },
    /// Resolves where each cloud file would be restored inside `destination`.
    PreviewRestorePaths {
        destination: String,
        names: Vec<String>,
    },
    ToggleSteam(GameEntry),
    ConnectDrive {
        client_id: String,
        client_secret: String,
    },
    UnlinkDrive,
    CheckForUpdates {
        manual: bool,
    },
    DownloadUpdate(UpdateInfo),
    RestartApp(PathBuf),
    ResolveConflict(Decision),
    RetryPostSync,
    ClearStatus,
    OpenConfigDirectory,
    PickPath {
        target: PathTarget,
        mode: PickMode,
    },
    /// Detects whether a typed save path is a file or a directory, off the UI thread.
    DetectSaveTargetType {
        index: usize,
        path: String,
    },
    Exit,
}
