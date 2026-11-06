//! Non-mutating observation of entry synchronization status.

use super::context::SyncContext;
use super::manual::remote_to_state;
use super::target_mapping::{directory_cloud_name, project_directory_files};
use super::target_resolver::ResolvedSaveTarget;
use emusync_domain::execution::GameLauncher;
use emusync_domain::library::{GameEntry, SaveTarget};
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, EntrySyncStatus, LocalSaveAccess, LocalSaveFiles,
    LocalSavePath, RemoteFile, canonical_cloud_files,
};
use std::collections::{BTreeMap, HashSet};

pub struct SyncStatusChecker<'a, A, L, F, T, Auth, R>
where
    A: LocalSaveAccess,
{
    context: &'a SyncContext<A, L, F, T, Auth, R>,
}

impl<'a, A, L, F, T, Auth, R> SyncStatusChecker<'a, A, L, F, T, Auth, R>
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

    pub async fn check_entry_sync_status(
        &self,
        entry: &GameEntry,
    ) -> Result<EntrySyncStatus, String> {
        let creds = self.context.config.credentials.as_ref();
        if creds.and_then(|c| c.refresh_token.as_ref()).is_none() {
            return Ok(EntrySyncStatus::NotConfigured);
        }
        let targets = match self.context.resolver.get_save_targets(entry) {
            Ok(t) => t,
            Err(_) => return Ok(EntrySyncStatus::Error),
        };
        if targets.is_empty() {
            return Ok(EntrySyncStatus::Idle);
        }
        Ok(self.read_status(&targets).await)
    }

    async fn read_status(&self, targets: &[ResolvedSaveTarget<A::Path>]) -> EntrySyncStatus {
        let token = match self
            .context
            .services
            .authorization
            .authorize(&self.context.config, false)
            .await
        {
            Ok(t) => t,
            Err(_) => return EntrySyncStatus::Error,
        };

        let mut observation = StatusObservation::default();

        let mut targets_by_folder: BTreeMap<Vec<String>, Vec<&ResolvedSaveTarget<A::Path>>> =
            BTreeMap::new();
        for t in targets {
            targets_by_folder
                .entry(t.cloud_path_segments.clone())
                .or_default()
                .push(t);
        }

        for (path, folder_targets) in targets_by_folder {
            let folder_id = self
                .context
                .services
                .folders
                .find_path(&token, &path)
                .await
                .ok()
                .flatten();

            let cloud_files = match &folder_id {
                Some(id) => self
                    .context
                    .services
                    .search
                    .list_files(&token, id)
                    .await
                    .ok()
                    .and_then(|f| canonical_cloud_files(&f).ok())
                    .unwrap_or_default(),
                None => Vec::new(),
            };

            let configured: Vec<SaveTarget> =
                folder_targets.iter().map(|t| t.configured()).collect();
            if self
                .context
                .mapping
                .validate_save_target_mappings(&configured, &cloud_files)
                .is_err()
            {
                return EntrySyncStatus::Error;
            }

            for target in folder_targets {
                self.observe_target(
                    target,
                    folder_id.as_deref().unwrap_or(""),
                    &cloud_files,
                    &configured,
                    &mut observation,
                )
                .await;
            }
        }

        observation.status()
    }

    async fn observe_target(
        &self,
        target: &ResolvedSaveTarget<A::Path>,
        folder_id: &str,
        cloud_files: &[RemoteFile],
        configured: &[SaveTarget],
        observation: &mut StatusObservation,
    ) {
        let reserved = match self.context.mapping.reserved_cloud_names(
            &target.configured(),
            configured,
            cloud_files,
        ) {
            Ok(r) => r,
            Err(_) => return,
        };

        let is_dir = self
            .context
            .mapping
            .is_directory(&target.configured(), cloud_files, configured)
            .unwrap_or(false);

        if is_dir {
            let remote_files = project_directory_files(cloud_files, target.cloud_path.as_deref());
            self.observe_directory(target, folder_id, &remote_files, &reserved, observation)
                .await;
        } else {
            let name = self.context.mapping.file_cloud_name(&target.configured());
            let remote = cloud_files.iter().find(|cf| cf.name == name);
            let remote_state = remote.map(remote_to_state);
            if let Ok(decision) = self
                .context
                .services
                .tracked_sync
                .plan(target.file.path(), remote_state.as_ref(), folder_id, &name)
                .await
            {
                observation.record(decision);
            }
        }
    }

    async fn observe_directory(
        &self,
        target: &ResolvedSaveTarget<A::Path>,
        folder_id: &str,
        remote_files: &[RemoteFile],
        reserved: &HashSet<String>,
        observation: &mut StatusObservation,
    ) {
        let root = &target.file;
        let local_files = if root.exists().unwrap_or(false) && root.is_directory().unwrap_or(false)
        {
            root.walk_top_down().unwrap_or_default()
        } else {
            Vec::new()
        };

        for remote in remote_files {
            if let Ok(Some(local)) = root.resolve_remote(&remote.name) {
                let drive_name = directory_cloud_name(target.cloud_path.as_deref(), &remote.name);
                let state = remote_to_state(remote);
                if let Ok(decision) = self
                    .context
                    .services
                    .tracked_sync
                    .plan(local.path(), Some(&state), folder_id, &drive_name)
                    .await
                {
                    observation.record(decision);
                }
            }
        }

        for local in local_files {
            if local.is_file().unwrap_or(false)
                && let Ok(relative) = local.relative_to(root)
                && !reserved.contains(&relative)
                && !remote_files.iter().any(|rf| rf.name == relative)
            {
                observation.record(Decision::UploadLocal);
            }
        }
    }
}

#[derive(Default)]
struct StatusObservation {
    local_changed: bool,
    remote_changed: bool,
    conflict: bool,
}

impl StatusObservation {
    fn record(&mut self, decision: Decision) {
        match decision {
            Decision::DownloadCloud => self.remote_changed = true,
            Decision::UploadLocal => self.local_changed = true,
            Decision::Conflict => self.conflict = true,
            Decision::InSync => {}
        }
    }

    fn status(&self) -> EntrySyncStatus {
        if self.conflict || (self.local_changed && self.remote_changed) {
            EntrySyncStatus::Conflict
        } else if self.local_changed || self.remote_changed {
            EntrySyncStatus::OutOfSync
        } else {
            EntrySyncStatus::InSync
        }
    }
}
