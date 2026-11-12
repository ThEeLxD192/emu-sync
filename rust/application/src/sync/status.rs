//! Non-mutating observation of entry synchronization status.

use super::context::SyncContext;
use super::manual::remote_to_state;
use super::target_mapping::{
    SaveTargetMappingError, directory_cloud_name, project_directory_files,
};
use super::target_resolver::ResolvedSaveTarget;
use emusync_domain::execution::GameLauncher;
use emusync_domain::library::{GameEntry, SaveTarget};
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, EntrySyncStatus, LocalSaveAccess, LocalSaveFiles,
    LocalSavePath, RemoteFile, canonical_cloud_files, same_local_name,
};
use std::collections::{BTreeMap, HashSet};

pub(crate) struct SyncStatusChecker<'a, A, L, F, T, Auth, R>
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
    pub(crate) const fn new(context: &'a SyncContext<A, L, F, T, Auth, R>) -> Self {
        Self { context }
    }

    pub(crate) async fn check_entry_sync_status(
        &self,
        entry: &GameEntry,
    ) -> Result<EntrySyncStatus, String> {
        let creds = self.context.config.credentials.as_ref();
        if !creds
            .and_then(|c| c.refresh_token.as_deref())
            .is_some_and(|token| !token.trim().is_empty())
        {
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

        let mut targets_by_folder: BTreeMap<&[String], Vec<&ResolvedSaveTarget<A::Path>>> =
            BTreeMap::new();
        for t in targets {
            targets_by_folder
                .entry(t.cloud_path_segments.as_slice())
                .or_default()
                .push(t);
        }

        for (path, folder_targets) in targets_by_folder {
            let folder_id = match self.context.services.folders.find_path(&token, path).await {
                Ok(folder) => folder,
                Err(_) => return EntrySyncStatus::Error,
            };

            let cloud_files = match &folder_id {
                Some(id) => {
                    let files = match self.context.services.search.list_files(&token, id).await {
                        Ok(files) => files,
                        Err(_) => return EntrySyncStatus::Error,
                    };
                    match canonical_cloud_files(&files) {
                        Ok(files) => files,
                        Err(_) => return EntrySyncStatus::Error,
                    }
                }
                None => Vec::new(),
            };

            let configured: Vec<SaveTarget> =
                folder_targets.iter().map(|t| t.configured()).collect();
            match self
                .context
                .mapping
                .validate_save_target_mappings(&configured, &cloud_files)
            {
                Ok(()) => {}
                // Syncing restores these cloud files into their folders; until then the
                // folders cannot be compared file by file.
                Err(SaveTargetMappingError::UnplacedLegacyFiles(_)) => {
                    observation.record(Decision::DownloadCloud);
                    continue;
                }
                Err(_) => return EntrySyncStatus::Error,
            }

            for (target, cfg) in folder_targets.into_iter().zip(&configured) {
                if self
                    .observe_target(
                        target,
                        cfg,
                        folder_id.as_deref().unwrap_or(""),
                        &cloud_files,
                        &configured,
                        &mut observation,
                    )
                    .await
                    .is_err()
                {
                    return EntrySyncStatus::Error;
                }
            }
        }

        observation.status()
    }

    async fn observe_target(
        &self,
        target: &ResolvedSaveTarget<A::Path>,
        cfg: &SaveTarget,
        folder_id: &str,
        cloud_files: &[RemoteFile],
        configured: &[SaveTarget],
        observation: &mut StatusObservation,
    ) -> Result<(), ()> {
        let reserved = self
            .context
            .mapping
            .reserved_cloud_names(cfg, configured, cloud_files)
            .map_err(|_| ())?;

        let is_dir = self
            .context
            .mapping
            .is_directory(cfg, cloud_files, configured)
            .map_err(|_| ())?;

        if is_dir {
            let projected = project_directory_files(cloud_files, target.cloud_path.as_deref());
            let remote_files = self
                .context
                .mapping
                .cloud_files_for_save_target(&target.kind, &target.file, &projected, &reserved)
                .map_err(|_| ())?;
            self.observe_directory(target, folder_id, &remote_files, &reserved, observation)
                .await?;
        } else {
            let name = self.context.mapping.file_cloud_name(cfg);
            let remote = cloud_files
                .iter()
                .find(|cf| same_local_name(&cf.name, &name));
            let remote_state = remote.map(remote_to_state);
            let decision = self
                .context
                .services
                .tracked_sync
                .plan(target.file.path(), remote_state.as_ref(), folder_id, &name)
                .await
                .map_err(|_| ())?;
            observation.record(decision);
        }
        Ok(())
    }

    async fn observe_directory(
        &self,
        target: &ResolvedSaveTarget<A::Path>,
        folder_id: &str,
        remote_files: &[RemoteFile],
        reserved: &HashSet<String>,
        observation: &mut StatusObservation,
    ) -> Result<(), ()> {
        let root = &target.file;

        for remote in remote_files {
            if target.excludes(&remote.name) {
                continue;
            }
            let local = root
                .resolve_remote(&remote.name)
                .map_err(|_| ())?
                .ok_or(())?;
            let drive_name = directory_cloud_name(target.cloud_path.as_deref(), &remote.name);
            let state = remote_to_state(remote);
            let decision = self
                .context
                .services
                .tracked_sync
                .plan(local.path(), Some(&state), folder_id, &drive_name)
                .await
                .map_err(|_| ())?;
            observation.record(decision);
        }

        let local_files =
            if root.exists().map_err(|_| ())? && root.is_directory().map_err(|_| ())? {
                root.walk_top_down().map_err(|_| ())?
            } else {
                Vec::new()
            };

        if !local_files.is_empty() {
            let remote_names: HashSet<&str> =
                remote_files.iter().map(|rf| rf.name.as_str()).collect();
            for local in local_files {
                if local.is_file().map_err(|_| ())? {
                    let relative = local.relative_to(root).map_err(|_| ())?;
                    if !reserved.contains(&relative)
                        && !remote_names.contains(relative.as_str())
                        && !target.excludes(&relative)
                    {
                        observation.record(Decision::UploadLocal);
                    }
                }
            }
        }
        Ok(())
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
