//! Restores cloud files of older save folders that share one cloud location when no local
//! folder holds them yet, so a new device can sync them instead of stopping.

use super::context::SyncContext;
use super::manual::{ManualFileSync, PlannedSave};
use super::target_mapping::{SaveTargetMappingError, UnplacedLegacyFiles};
use emusync_domain::library::SaveTarget;
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, LocalSaveAccess, LocalSaveFiles, LocalSavePath,
    RemoteFile, SyncEvent, canonical_cloud_files,
};

const PLACEMENT_CANCELLED: &str = "Cloud saves were not restored because no folder was chosen. These folders were not synchronized.";

pub(crate) struct LegacyPlacement<'a, A, L, F, T, Auth, R>
where
    A: LocalSaveAccess,
{
    context: &'a SyncContext<A, L, F, T, Auth, R>,
}

impl<'a, A, L, F, T, Auth, R> LegacyPlacement<'a, A, L, F, T, Auth, R>
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
    Auth: emusync_domain::config::CloudAuthorization + Clone + Send + Sync + 'static,
    Auth::Error: std::fmt::Display + Send + Sync + 'static,
    R: emusync_domain::execution::GameLauncher + Clone + Send + Sync + 'static,
    R::Error: std::fmt::Debug + Send + Sync + 'static,
{
    pub(crate) fn new(context: &'a SyncContext<A, L, F, T, Auth, R>) -> Self {
        Self { context }
    }

    /// Validates `targets`; cloud files that no older folder holds are first downloaded
    /// into the folder the local layout implies or, failing that, the one the user picks.
    pub(crate) async fn validate_restoring_unplaced(
        &self,
        token: &str,
        folder_id: &str,
        targets: &[SaveTarget],
        cloud_files: &[RemoteFile],
        game_name: &str,
    ) -> Result<(), String> {
        let unplaced = match self
            .context
            .mapping
            .validate_save_target_mappings(targets, cloud_files)
        {
            Ok(()) => return Ok(()),
            Err(SaveTargetMappingError::UnplacedLegacyFiles(unplaced)) => unplaced,
            Err(error) => return Err(error.to_string()),
        };
        let placements = self.placements(&unplaced, game_name).await?;

        (self.context.on_status)(SyncEvent::Syncing(format!(
            "Restoring cloud saves for {game_name}..."
        )));
        let remote_files = canonical_cloud_files(cloud_files)
            .map_err(|e| format!("Unverified duplicate remote file: {}", e.name))?;
        let manual = ManualFileSync::new(self.context);
        for (name, folder) in placements {
            let root_path = &unplaced.folders[folder];
            let remote = remote_files
                .iter()
                .find(|file| file.name == name)
                .ok_or_else(|| format!("Cloud file {name} is no longer available."))?;
            let local = self
                .context
                .services
                .files
                .path(root_path)
                .resolve_remote(&name)
                .map_err(|e| format!("{e:?}"))?
                .ok_or_else(|| {
                    format!("Cloud file {name} cannot be restored inside {root_path}.")
                })?;
            // Only missing files are placed; anything at the destination stays untouched.
            if local.exists().map_err(|e| format!("{e:?}"))? {
                return Err(format!(
                    "Cannot restore {name}: {} already exists. These folders were not synchronized.",
                    local.path()
                ));
            }
            let planned = PlannedSave {
                local,
                remote: Some(remote.clone()),
                folder_id: folder_id.to_string(),
                drive_name: name.clone(),
                directory: Some((root_path.clone(), name.clone())),
            };
            manual
                .apply_file_decision(token, &planned, Decision::DownloadCloud)
                .await?;
        }

        self.context
            .mapping
            .validate_save_target_mappings(targets, cloud_files)
            .map_err(|error| error.to_string())
    }

    /// Inferred destinations plus the user's answer for every group left.
    async fn placements(
        &self,
        unplaced: &UnplacedLegacyFiles,
        game_name: &str,
    ) -> Result<Vec<(String, usize)>, String> {
        let mut placements = unplaced.inferred.clone();
        if unplaced.groups.is_empty() {
            return Ok(placements);
        }
        let choice = self
            .context
            .request_save_placement(
                game_name.to_string(),
                unplaced.folders.clone(),
                unplaced.groups.clone(),
            )
            .await
            .ok_or_else(|| PLACEMENT_CANCELLED.to_string())?;
        let answered = choice.len() == unplaced.groups.len()
            && choice.iter().all(|&folder| folder < unplaced.folders.len());
        if !answered {
            return Err(PLACEMENT_CANCELLED.to_string());
        }
        for (group, folder) in unplaced.groups.iter().zip(choice) {
            placements.extend(group.files.iter().map(|name| (name.clone(), folder)));
        }
        Ok(placements)
    }
}
