//! Cloud entry and game rename migration workflows.

use super::context::SyncContext;
use super::manual::ManualFileSync;
use super::target_mapping::project_directory_files;
use emusync_domain::execution::GameLauncher;
use emusync_domain::library::{GameEntry, SaveTarget};
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, LocalSaveAccess, LocalSaveFiles, LocalSavePath, SyncEvent,
    canonical_cloud_files,
};

pub struct CloudEntryMigration<'a, A, L, F, T, Auth, R>
where
    A: LocalSaveAccess,
{
    context: &'a SyncContext<A, L, F, T, Auth, R>,
}

impl<'a, A, L, F, T, Auth, R> CloudEntryMigration<'a, A, L, F, T, Auth, R>
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
    R: GameLauncher + Clone + Send + Sync + 'static,
    R::Error: std::fmt::Debug + Send + Sync + 'static,
{
    pub fn new(context: &'a SyncContext<A, L, F, T, Auth, R>) -> Self {
        Self { context }
    }

    pub async fn rename_game_cloud_data(
        &self,
        entry: &GameEntry,
        rom_name: Option<&str>,
        old_title: &str,
        new_title: &str,
        local_save_targets: &[SaveTarget],
    ) -> Result<bool, String> {
        if self
            .context
            .config
            .credentials
            .as_ref()
            .and_then(|c| c.refresh_token.as_ref())
            .is_none()
        {
            return Ok(true);
        }

        let old_cloud_path = self
            .context
            .resolver
            .get_cloud_path_segments_entry(entry, rom_name, Some(old_title))
            .map_err(|e| format!("{e:?}"))?;
        let new_cloud_path = self
            .context
            .resolver
            .get_cloud_path_segments_entry(entry, rom_name, Some(new_title))
            .map_err(|e| format!("{e:?}"))?;

        if old_cloud_path == new_cloud_path {
            return Ok(true);
        }

        (self.context.on_status)(SyncEvent::Syncing(format!(
            "Checking cloud folder for {new_title}..."
        )));

        let token = match self
            .context
            .services
            .authorization
            .authorize(&self.context.config, false)
            .await
        {
            Ok(tok) => tok,
            Err(err) => {
                (self.context.on_status)(SyncEvent::Error(format!(
                    "Drive authentication failed: {err}"
                )));
                return Ok(false);
            }
        };

        let old_folder_id = self
            .context
            .services
            .folders
            .find_path(&token, &old_cloud_path)
            .await
            .map_err(|e| format!("{e:?}"))?;
        let new_folder_id = self
            .context
            .services
            .folders
            .find_path(&token, &new_cloud_path)
            .await
            .map_err(|e| format!("{e:?}"))?;

        match (old_folder_id, new_folder_id) {
            (Some(_), None) => {
                (self.context.on_status)(SyncEvent::Syncing(format!(
                    "Renaming cloud folder to {new_title}..."
                )));
                if let Err(err) = self
                    .context
                    .migrator
                    .migrate(&token, &old_cloud_path, &new_cloud_path)
                    .await
                {
                    (self.context.on_status)(SyncEvent::Error(format!(
                        "Cloud rename error: {err}"
                    )));
                    return Ok(false);
                }
                (self.context.on_status)(SyncEvent::Idle);
                Ok(true)
            }
            (old_id, Some(new_id)) => {
                (self.context.on_status)(SyncEvent::Syncing(format!(
                    "Checking cloud files in {new_title}..."
                )));
                if let Some(old) = old_id
                    && old != new_id
                    && let Err(err) = self
                        .context
                        .migrator
                        .migrate(&token, &old_cloud_path, &new_cloud_path)
                        .await
                {
                    (self.context.on_status)(SyncEvent::Error(format!(
                        "Cloud rename error: {err}"
                    )));
                    return Ok(false);
                }
                self.sync_renamed_folder(&token, entry, new_title, local_save_targets, &new_id)
                    .await?;
                (self.context.on_status)(SyncEvent::Idle);
                Ok(true)
            }
            (None, None) => {
                (self.context.on_status)(SyncEvent::Idle);
                Ok(true)
            }
        }
    }

    async fn sync_renamed_folder(
        &self,
        token: &str,
        entry: &GameEntry,
        new_title: &str,
        local_save_targets: &[SaveTarget],
        new_folder_id: &str,
    ) -> Result<(), String> {
        let raw_files = self
            .context
            .services
            .search
            .list_files(token, new_folder_id)
            .await
            .map_err(|e| format!("{e:?}"))?;
        let cloud_files = canonical_cloud_files(&raw_files)
            .map_err(|e| format!("Duplicate cloud file: {}", e.name))?;

        self.context
            .mapping
            .validate_save_target_mappings(local_save_targets, &cloud_files)
            .map_err(|e| e.to_string())?;

        let manual = ManualFileSync::new(self.context);
        for target in local_save_targets {
            let save_path = self.context.services.files.path(&target.path);
            let reserved = self
                .context
                .mapping
                .reserved_cloud_names(target, local_save_targets, &cloud_files)
                .map_err(|e| e.to_string())?;
            let is_dir = self
                .context
                .mapping
                .is_directory(target, &cloud_files, local_save_targets)
                .map_err(|e| e.to_string())?;

            if is_dir {
                let projected = project_directory_files(&cloud_files, target.cloud_path.as_deref());
                let target_cloud_files = self
                    .context
                    .mapping
                    .cloud_files_for_save_target(&target.kind, &save_path, &projected, &reserved)
                    .map_err(|e| e.to_string())?;
                manual
                    .sync_directory_manual(
                        token,
                        entry,
                        &save_path,
                        new_folder_id,
                        &target_cloud_files,
                        new_title,
                        &reserved,
                        target.cloud_path.as_deref(),
                        false,
                    )
                    .await?;
            } else {
                let drive_name = self.context.mapping.file_cloud_name(target);
                manual
                    .sync_file_manual(
                        token,
                        entry,
                        &save_path,
                        new_folder_id,
                        &cloud_files,
                        new_title,
                        &drive_name,
                        false,
                    )
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn rename_entry_cloud_data(
        &self,
        old_entry: &GameEntry,
        new_entry: &GameEntry,
    ) -> Result<bool, String> {
        let old_path = self.entry_path(old_entry);
        let new_path = self.entry_path(new_entry);

        if old_path == new_path {
            return Ok(true);
        }
        if !self.can_migrate_entry(old_entry, &old_path) {
            return Ok(false);
        }
        if self
            .context
            .config
            .credentials
            .as_ref()
            .and_then(|c| c.refresh_token.as_ref())
            .is_none()
        {
            return Ok(true);
        }

        (self.context.on_status)(SyncEvent::Syncing(format!(
            "Moving cloud saves for {}...",
            new_entry.common().name
        )));

        let token = match self
            .context
            .services
            .authorization
            .authorize(&self.context.config, false)
            .await
        {
            Ok(tok) => tok,
            Err(err) => {
                (self.context.on_status)(SyncEvent::Error(format!(
                    "Drive authentication failed: {err}"
                )));
                return Ok(false);
            }
        };

        if let Err(err) = self
            .context
            .migrator
            .migrate(&token, &old_path, &new_path)
            .await
        {
            (self.context.on_status)(SyncEvent::Error(format!("Cloud rename error: {err}")));
            return Ok(false);
        }

        (self.context.on_status)(SyncEvent::Idle);
        Ok(true)
    }

    fn entry_path(&self, entry: &GameEntry) -> Vec<String> {
        match entry {
            GameEntry::Native(_) => self
                .context
                .resolver
                .get_cloud_path_segments_entry(entry, None, None)
                .unwrap_or_else(|_| vec![entry.common().effective_cloud_folder().to_string()]),
            GameEntry::Emulator(emu) => vec![emu.common.effective_cloud_folder().to_string()],
        }
    }

    fn can_migrate_entry(&self, old_entry: &GameEntry, old_path: &[String]) -> bool {
        let entry_index = self
            .context
            .config
            .entries
            .iter()
            .position(|e| e.common().name == old_entry.common().name);

        let shared = self
            .context
            .config
            .entries
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != entry_index)
            .any(|(_, other)| {
                (old_path.len() == 1 && other.common().effective_cloud_folder() == old_path[0])
                    || self.entry_path(other) == old_path
                    || self
                        .context
                        .resolver
                        .get_save_targets(other)
                        .map(|targets| targets.iter().any(|t| t.cloud_path_segments == old_path))
                        .unwrap_or(false)
            });

        if shared {
            (self.context.on_status)(SyncEvent::Error(format!(
                "The cloud folder '{}' is shared with another entry. Keep the existing cloud folder when editing this entry to protect their saves.",
                old_path.join("/")
            )));
            return false;
        }

        true
    }
}
