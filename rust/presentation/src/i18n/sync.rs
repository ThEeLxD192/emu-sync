//! Synchronization messages put into words.

use super::{Texts, fill};
use emusync_domain::sync::SyncMessage;

/// `message` in the language of `t`.
pub fn sync_text(t: &Texts, message: &SyncMessage) -> String {
    use SyncMessage as M;
    match message {
        M::CheckingCloudSaves => t.sync_checking_cloud_saves.to_owned(),
        M::ConnectingDrive => t.sync_connecting_drive.to_owned(),
        M::SyncingGameSaves { game } => fill(t.sync_syncing_game, &[("game", game)]),
        M::UploadingGameSaves { game } => fill(t.sync_uploading_game, &[("game", game)]),
        M::RestoringCloudSaves { game } => fill(t.sync_restoring_game, &[("game", game)]),
        M::SyncingTarget { name } => fill(t.sync_syncing_target, &[("name", name)]),
        M::CheckingCloudFolder { title } => fill(t.sync_checking_folder, &[("title", title)]),
        M::RenamingCloudFolder { title } => fill(t.sync_renaming_folder, &[("title", title)]),
        M::CheckingCloudFiles { title } => fill(t.sync_checking_files, &[("title", title)]),
        M::MovingCloudSaves { name } => fill(t.sync_moving_saves, &[("name", name)]),
        M::Failed { detail } => fill(t.sync_failed, &[("detail", detail)]),
        M::GameNotTracked { game } => fill(t.sync_game_not_tracked, &[("game", game)]),
        M::CategoryNameInvalid => t.sync_category_name_invalid.to_owned(),
        M::DriveAuthFailed { detail } => fill(t.sync_drive_auth_failed, &[("detail", detail)]),
        M::CloudRenameFailed { detail } => fill(t.sync_cloud_rename_failed, &[("detail", detail)]),
        M::SharedCloudFolder { folder } => fill(t.sync_shared_cloud_folder, &[("folder", folder)]),
        M::DriveNotConnected => t.sync_drive_not_connected.to_owned(),
        M::DriveConnectFailed { detail } => {
            fill(t.sync_drive_connect_failed, &[("detail", detail)])
        }
        M::SaveNameTaken { name } => fill(t.sync_save_name_taken, &[("name", name)]),
        M::DirectorySaveConflict { path } => {
            fill(t.sync_directory_save_conflict, &[("path", path)])
        }
        M::UnknownDriveError => t.sync_unknown_drive_error.to_owned(),
        M::InvalidCloudPath { path } => fill(t.sync_invalid_cloud_path, &[("path", path)]),
        M::ConflictingLocalDestinations => t.sync_conflicting_local_destinations.to_owned(),
        M::FileIsParentOfAnother => t.sync_file_is_parent.to_owned(),
        M::OverlappingDirectoryScopes => t.sync_overlapping_scopes.to_owned(),
        M::ConflictingRemoteToLocalPaths => t.sync_conflicting_remote_paths.to_owned(),
        M::UnplacedLegacyFiles { folders, files } => fill(
            t.sync_unplaced_legacy,
            &[
                ("folders", &listing(t, t.sync_configured_folders, folders)),
                ("files", &listing(t, t.sync_unmatched_cloud_files, files)),
            ],
        ),
        M::LocalAccessFailed { detail } => fill(t.sync_local_access_failed, &[("detail", detail)]),
        M::Detail(detail) => detail.clone(),
        M::Several(messages) => messages
            .iter()
            .map(|message| sync_text(t, message))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// "Label:" and up to five bulleted items, then how many more there are.
fn listing(t: &Texts, label: &str, items: &[String]) -> String {
    let shown: Vec<String> = items
        .iter()
        .take(5)
        .map(|item| format!("• {item}"))
        .collect();
    let more = if items.len() > 5 {
        format!(
            "\n{}",
            fill(t.sync_more_items, &[("count", &(items.len() - 5))])
        )
    } else {
        String::new()
    };
    format!("{label}:\n{}{more}", shown.join("\n"))
}
