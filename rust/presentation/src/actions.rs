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
#[derive(Clone, PartialEq)]
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
    /// The save setup dialog was dismissed without saving.
    CancelSaveSetup {
        /// The game whose pending request was dismissed.
        game_name: String,
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

/// Hand-written so the Drive client secret never reaches logs.
impl std::fmt::Debug for UiAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SelectEntry(v) => f.debug_tuple("SelectEntry").field(v).finish(),
            Self::LaunchGame(v) => f.debug_tuple("LaunchGame").field(v).finish(),
            Self::OpenEmulator(v) => f.debug_tuple("OpenEmulator").field(v).finish(),
            Self::SyncEntry(v) => f.debug_tuple("SyncEntry").field(v).finish(),
            Self::CheckSyncEntry(v) => f.debug_tuple("CheckSyncEntry").field(v).finish(),
            Self::AddEntry(v) => f.debug_tuple("AddEntry").field(v).finish(),
            Self::EditEntry { old, new } => f
                .debug_struct("EditEntry")
                .field("old", old)
                .field("new", new)
                .finish(),
            Self::DeleteEntry(v) => f.debug_tuple("DeleteEntry").field(v).finish(),
            Self::ReorderEntries(v) => f.debug_tuple("ReorderEntries").field(v).finish(),
            Self::EditGame(v) => f.debug_tuple("EditGame").field(v).finish(),
            Self::EditGameOverride {
                item,
                title,
                targets,
                cover_path,
            } => f
                .debug_struct("EditGameOverride")
                .field("item", item)
                .field("title", title)
                .field("targets", targets)
                .field("cover_path", cover_path)
                .finish(),
            Self::CompleteSaveSetup {
                request,
                targets,
                title,
                cover_path,
                restore,
            } => f
                .debug_struct("CompleteSaveSetup")
                .field("request", request)
                .field("targets", targets)
                .field("title", title)
                .field("cover_path", cover_path)
                .field("restore", restore)
                .finish(),
            Self::CancelSaveSetup { game_name } => f
                .debug_struct("CancelSaveSetup")
                .field("game_name", game_name)
                .finish(),
            Self::PreviewRestorePaths { destination, names } => f
                .debug_struct("PreviewRestorePaths")
                .field("destination", destination)
                .field("names", names)
                .finish(),
            Self::ToggleSteam(v) => f.debug_tuple("ToggleSteam").field(v).finish(),
            Self::ConnectDrive { client_id, .. } => f
                .debug_struct("ConnectDrive")
                .field("client_id", client_id)
                .field("client_secret", &"<redacted>")
                .finish(),
            Self::UnlinkDrive => f.write_str("UnlinkDrive"),
            Self::CheckForUpdates { manual } => f
                .debug_struct("CheckForUpdates")
                .field("manual", manual)
                .finish(),
            Self::DownloadUpdate(v) => f.debug_tuple("DownloadUpdate").field(v).finish(),
            Self::RestartApp(v) => f.debug_tuple("RestartApp").field(v).finish(),
            Self::ResolveConflict(v) => f.debug_tuple("ResolveConflict").field(v).finish(),
            Self::RetryPostSync => f.write_str("RetryPostSync"),
            Self::ClearStatus => f.write_str("ClearStatus"),
            Self::OpenConfigDirectory => f.write_str("OpenConfigDirectory"),
            Self::PickPath { target, mode } => f
                .debug_struct("PickPath")
                .field("target", target)
                .field("mode", mode)
                .finish(),
            Self::DetectSaveTargetType { index, path } => f
                .debug_struct("DetectSaveTargetType")
                .field("index", index)
                .field("path", path)
                .finish(),
            Self::Exit => f.write_str("Exit"),
        }
    }
}
