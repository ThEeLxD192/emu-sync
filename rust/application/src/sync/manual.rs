//! Manual and interactive file and entry synchronization with Google Drive.

use super::context::SyncContext;
use super::legacy_placement::LegacyPlacement;
use super::target_mapping::{directory_cloud_name, project_directory_files};
use emusync_domain::library::GameEntry;
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, DirectoryDownload, LocalSaveAccess,
    LocalSaveFiles, LocalSavePath, RemoteFile, RemoteState, SyncEvent, canonical_cloud_files,
};
use std::collections::{BTreeMap, HashSet};

pub struct PlannedSave<P> {
    pub local: P,
    pub remote: Option<RemoteFile>,
    pub folder_id: String,
    pub drive_name: String,
    /// User-selected root and the relative name derived from the directory listing.
    pub directory: Option<(String, String)>,
}

pub struct ManualFileSync<'a, A, L, F, T, Auth, R>
where
    A: LocalSaveAccess,
{
    context: &'a SyncContext<A, L, F, T, Auth, R>,
}

impl<'a, A, L, F, T, Auth, R> ManualFileSync<'a, A, L, F, T, Auth, R>
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
    pub fn new(context: &'a SyncContext<A, L, F, T, Auth, R>) -> Self {
        Self { context }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn sync_directory_manual(
        &self,
        token: &str,
        local_save_path: &A::Path,
        folder_id: &str,
        cloud_files: &[RemoteFile],
        display_name: &str,
        reserved_sibling_names: &HashSet<String>,
        cloud_prefix: Option<&str>,
        before_play: bool,
    ) -> Result<(), String> {
        let actions = self
            .directory_actions(
                local_save_path,
                folder_id,
                cloud_files,
                reserved_sibling_names,
                cloud_prefix,
            )
            .await?;

        let choice = self.directory_choice(&actions, display_name).await;

        for action in actions {
            let decision = self.directory_decision(&action, choice, before_play);
            if let Some(dec) = decision {
                self.apply_file_decision(token, &action.file, dec).await?;
            }
        }
        Ok(())
    }

    async fn directory_actions(
        &self,
        root: &A::Path,
        folder_id: &str,
        cloud_files: &[RemoteFile],
        reserved: &HashSet<String>,
        prefix: Option<&str>,
    ) -> Result<Vec<DirectoryAction<A::Path>>, String> {
        let mut files: BTreeMap<String, DirectoryFileEntry<A::Path>> = BTreeMap::new();

        if root.is_directory().unwrap_or(false)
            && let Ok(walked) = root.walk_top_down()
        {
            for file in walked {
                if file.is_file().unwrap_or(false)
                    && let Ok(rel) = file.relative_to(root)
                    && !reserved.contains(&rel)
                {
                    files.entry(rel).or_default().local = Some(file);
                }
            }
        }

        let canonical_remote = canonical_cloud_files(cloud_files)
            .map_err(|e| format!("Unverified duplicate remote file: {}", e.name))?;

        for file in canonical_remote {
            if let Ok(Some(resolved)) = root.resolve_remote(&file.name) {
                let name = file.name.clone();
                files.entry(name).or_default().remote = Some((file, resolved));
            }
        }

        let mut actions = Vec::with_capacity(files.len());
        let root_path = root.path().to_owned();

        for (name, entry) in files {
            let (local, remote) = match (entry.local, entry.remote) {
                (Some(local), Some((remote, _))) => (local, Some(remote)),
                (Some(local), None) => (local, None),
                (None, Some((remote, resolved))) => (resolved, Some(remote)),
                (None, None) => continue,
            };

            let drive_name = directory_cloud_name(prefix, &name);
            let remote_state = remote.as_ref().map(remote_to_state);
            let decision = self
                .context
                .services
                .tracked_sync
                .plan(local.path(), remote_state.as_ref(), folder_id, &drive_name)
                .await
                .map_err(|e| format!("{e:?}"))?;

            actions.push(DirectoryAction {
                file: PlannedSave {
                    local,
                    remote,
                    folder_id: folder_id.to_string(),
                    drive_name,
                    directory: Some((root_path.clone(), name)),
                },
                decision,
            });
        }
        Ok(actions)
    }

    async fn directory_choice(
        &self,
        actions: &[DirectoryAction<A::Path>],
        display_name: &str,
    ) -> Option<Decision> {
        let conflict = actions.iter().find(|a| a.decision == Decision::Conflict);
        let mixed = actions.iter().any(|a| a.decision == Decision::UploadLocal)
            && actions
                .iter()
                .any(|a| a.decision == Decision::DownloadCloud);

        if conflict.is_some() || mixed {
            let representative =
                conflict.or_else(|| actions.iter().find(|a| a.file.remote.is_some()))?;
            let remote = representative.file.remote.as_ref()?;
            let local_mod = representative.file.local.last_modified_ms().unwrap_or(0);
            let cloud_mod = (remote.modified / 1_000_000) as i64;
            Some(
                self.context
                    .request_conflict(local_mod, cloud_mod, display_name.to_string())
                    .await,
            )
        } else {
            None
        }
    }

    fn directory_decision(
        &self,
        action: &DirectoryAction<A::Path>,
        choice: Option<Decision>,
        before_play: bool,
    ) -> Option<Decision> {
        let decision = match action.decision {
            Decision::InSync => Some(Decision::InSync),
            _ => match choice {
                Some(Decision::DownloadCloud) => {
                    if action.file.remote.is_some() {
                        Some(Decision::DownloadCloud)
                    } else {
                        None
                    }
                }
                Some(Decision::UploadLocal) => {
                    if action.file.local.is_file().unwrap_or(false) {
                        Some(Decision::UploadLocal)
                    } else {
                        None
                    }
                }
                _ => Some(action.decision),
            },
        };
        let deferred_upload =
            before_play && choice.is_none() && decision == Some(Decision::UploadLocal);
        if deferred_upload { None } else { decision }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn sync_file_manual(
        &self,
        token: &str,
        local_save_path: &A::Path,
        folder_id: &str,
        cloud_files: &[RemoteFile],
        display_name: &str,
        drive_name: &str,
        before_play: bool,
    ) -> Result<(), String> {
        let canonical_remote = canonical_cloud_files(cloud_files)
            .map_err(|e| format!("Unverified duplicate remote file: {}", e.name))?;
        let remote = canonical_remote.into_iter().find(|f| f.name == drive_name);
        let remote_state = remote.as_ref().map(remote_to_state);
        let decision = self
            .context
            .services
            .tracked_sync
            .plan(
                local_save_path.path(),
                remote_state.as_ref(),
                folder_id,
                drive_name,
            )
            .await
            .map_err(|e| format!("{e:?}"))?;

        let choice = if decision == Decision::Conflict
            && let Some(r) = remote.as_ref()
        {
            let local_mod = local_save_path.last_modified_ms().unwrap_or(0);
            let cloud_mod = (r.modified / 1_000_000) as i64;
            self.context
                .request_conflict(local_mod, cloud_mod, display_name.to_string())
                .await
        } else {
            decision
        };

        if !(before_play && decision == Decision::UploadLocal) {
            let planned = PlannedSave {
                local: local_save_path.clone(),
                remote,
                folder_id: folder_id.to_string(),
                drive_name: drive_name.to_string(),
                directory: None,
            };
            self.apply_file_decision(token, &planned, choice).await?;
        }
        Ok(())
    }

    pub async fn apply_file_decision(
        &self,
        token: &str,
        file: &PlannedSave<A::Path>,
        decision: Decision,
    ) -> Result<(), String> {
        let local_path = file.local.path();
        let folder_id = &file.folder_id;
        let drive_name = &file.drive_name;
        let remote_state = file.remote.as_ref().map(remote_to_state);

        match decision {
            Decision::DownloadCloud => {
                if let Some(r) = &remote_state {
                    let tracked = &self.context.services.tracked_sync;
                    if let Some((root, relative)) = &file.directory {
                        tracked
                            .download_in_directory(DirectoryDownload {
                                token,
                                remote: r,
                                local: local_path,
                                root,
                                relative,
                                folder: folder_id,
                                remote_name: drive_name,
                            })
                            .await
                            .map_err(|e| format!("{e:?}"))?;
                    } else {
                        tracked
                            .download(token, r, local_path, folder_id, drive_name)
                            .await
                            .map_err(|e| format!("{e:?}"))?;
                    }
                }
            }
            Decision::UploadLocal => {
                if file.local.is_file().unwrap_or(false) {
                    self.context
                        .services
                        .tracked_sync
                        .upload(
                            token,
                            local_path,
                            remote_state.as_ref(),
                            folder_id,
                            drive_name,
                        )
                        .await
                        .map_err(|e| format!("{e:?}"))?;
                }
            }
            Decision::InSync => {
                if file.local.is_file().unwrap_or(false)
                    && let Some(r) = &remote_state
                {
                    self.context
                        .services
                        .tracked_sync
                        .remember(local_path, r, folder_id, drive_name)
                        .await
                        .map_err(|e| format!("{e:?}"))?;
                }
            }
            Decision::Conflict => {
                return Err("A conflict must be resolved before applying a save".into());
            }
        }
        Ok(())
    }
}

struct DirectoryFileEntry<P> {
    local: Option<P>,
    remote: Option<(RemoteFile, P)>,
}

impl<P> Default for DirectoryFileEntry<P> {
    fn default() -> Self {
        Self {
            local: None,
            remote: None,
        }
    }
}

struct DirectoryAction<P> {
    file: PlannedSave<P>,
    decision: Decision,
}

pub(crate) fn remote_to_state(remote: &RemoteFile) -> RemoteState {
    RemoteState {
        id: remote.id.clone(),
        modified: remote.modified,
        sha256: remote.sha256.clone(),
        version: remote.version.clone(),
    }
}

pub struct ManualEntrySync<'a, A, L, F, T, Auth, R>
where
    A: LocalSaveAccess,
{
    context: &'a SyncContext<A, L, F, T, Auth, R>,
}

impl<'a, A, L, F, T, Auth, R> ManualEntrySync<'a, A, L, F, T, Auth, R>
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
    pub fn new(context: &'a SyncContext<A, L, F, T, Auth, R>) -> Self {
        Self { context }
    }

    pub async fn sync_entry(&self, entry: &GameEntry) -> Result<bool, String> {
        let Some(token) = self.connect().await else {
            return Ok(false);
        };

        let targets = self
            .context
            .resolver
            .get_save_targets(entry)
            .map_err(|e| format!("{e:?}"))?;
        let manual = ManualFileSync::new(self.context);

        let mut targets_by_folder: BTreeMap<Vec<String>, Vec<_>> = BTreeMap::new();
        for mut t in targets {
            let folder_segments = std::mem::take(&mut t.cloud_path_segments);
            targets_by_folder
                .entry(folder_segments)
                .or_default()
                .push(t);
        }

        for (cloud_path, folder_targets) in targets_by_folder {
            let folder_id = self
                .context
                .services
                .folders
                .ensure_path(&token, &cloud_path)
                .await
                .map_err(|e| format!("{e:?}"))?;
            let raw_files = self
                .context
                .services
                .search
                .list_files(&token, &folder_id)
                .await
                .map_err(|e| format!("{e:?}"))?;
            let cloud_files = canonical_cloud_files(&raw_files)
                .map_err(|e| format!("Duplicate cloud file: {}", e.name))?;
            let configured_targets: Vec<_> =
                folder_targets.iter().map(|t| t.configured()).collect();
            LegacyPlacement::new(self.context)
                .validate_restoring_unplaced(
                    &token,
                    &folder_id,
                    &configured_targets,
                    &cloud_files,
                    folder_targets
                        .first()
                        .map_or(entry.common().name.as_str(), |t| t.display_name.as_str()),
                )
                .await?;

            for (target, configured) in folder_targets.into_iter().zip(&configured_targets) {
                (self.context.on_status)(SyncEvent::Syncing(format!(
                    "Syncing {}...",
                    target.display_name
                )));
                let reserved = self
                    .context
                    .mapping
                    .reserved_cloud_names(configured, &configured_targets, &cloud_files)
                    .map_err(|e| e.to_string())?;
                let is_dir = self
                    .context
                    .mapping
                    .is_directory(configured, &cloud_files, &configured_targets)
                    .map_err(|e| e.to_string())?;

                if is_dir {
                    let projected =
                        project_directory_files(&cloud_files, target.cloud_path.as_deref());
                    let target_cloud_files = self
                        .context
                        .mapping
                        .cloud_files_for_save_target(
                            &target.kind,
                            &target.file,
                            &projected,
                            &reserved,
                        )
                        .map_err(|e| e.to_string())?;
                    manual
                        .sync_directory_manual(
                            &token,
                            &target.file,
                            &folder_id,
                            &target_cloud_files,
                            &target.display_name,
                            &reserved,
                            target.cloud_path.as_deref(),
                            false,
                        )
                        .await?;
                } else {
                    let drive_name = self.context.mapping.file_cloud_name(configured);
                    manual
                        .sync_file_manual(
                            &token,
                            &target.file,
                            &folder_id,
                            &cloud_files,
                            &target.display_name,
                            &drive_name,
                            false,
                        )
                        .await?;
                }
            }
        }

        (self.context.on_status)(SyncEvent::Idle);
        Ok(true)
    }

    async fn connect(&self) -> Option<String> {
        let creds = self.context.config.credentials.as_ref();
        if creds.and_then(|c| c.refresh_token.as_ref()).is_none() {
            (self.context.on_status)(SyncEvent::Error(
                "Google Drive is not connected. Click 'Connect Drive' in the top bar to sign in."
                    .into(),
            ));
            return None;
        }
        (self.context.on_status)(SyncEvent::Syncing("Connecting to Google Drive...".into()));
        match self
            .context
            .services
            .authorization
            .authorize(&self.context.config, false)
            .await
        {
            Ok(token) => Some(token),
            Err(err) => {
                (self.context.on_status)(SyncEvent::Error(format!(
                    "Could not connect to Google Drive ({err}). Please reconnect your account by clicking 'Connect Drive'."
                )));
                None
            }
        }
    }
}
