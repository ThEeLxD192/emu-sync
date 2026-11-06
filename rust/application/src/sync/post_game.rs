//! Post-game synchronization of created and modified saves, failure reporting, and request-identified retries.

use super::context::SyncContext;
use super::manual::{ManualFileSync, PlannedSave, remote_to_state};
use super::target_mapping::{directory_cloud_name, project_directory_files};
use emusync_domain::execution::GameLauncher;
use emusync_domain::library::{GameEntry, SaveTarget, SaveTargetType};
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, LocalSaveAccess, LocalSaveFiles, LocalSavePath,
    PostSyncFailure, RemoteFile, SyncEvent, SyncGame, canonical_cloud_files,
};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveUploadTarget<P> {
    pub file: P,
    pub kind: SaveTargetType,
    pub drive_name: Option<String>,
    pub failure_reason: Option<String>,
    pub reserved_drive_names: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostSyncResult<P> {
    pub completed_count: usize,
    pub failed_targets: Vec<SaveUploadTarget<P>>,
    pub blocked_reasons: Vec<String>,
}

impl<P> Default for PostSyncResult<P> {
    fn default() -> Self {
        Self {
            completed_count: 0,
            failed_targets: Vec::new(),
            blocked_reasons: Vec::new(),
        }
    }
}

pub struct PostSaveSync<'a, A, L, F, T, Auth, R>
where
    A: LocalSaveAccess,
{
    context: &'a SyncContext<A, L, F, T, Auth, R>,
}

impl<'a, A, L, F, T, Auth, R> PostSaveSync<'a, A, L, F, T, Auth, R>
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

    #[allow(clippy::too_many_arguments)]
    pub async fn sync(
        &self,
        token: &str,
        entry: &GameEntry,
        local_save_path: &A::Path,
        target_type: &SaveTargetType,
        cloud_path_segments: &[String],
        drive_name_override: Option<&str>,
        known_file_sibling_names: &HashSet<String>,
        reserved_sibling_names: &HashSet<String>,
        cloud_path: Option<&str>,
    ) -> Result<PostSyncResult<A::Path>, String> {
        let drive_name = drive_name_override
            .or(cloud_path)
            .map(str::to_string)
            .unwrap_or_else(|| local_save_path.name().to_string());

        if let Some(blocked) = self.blocked_result(
            local_save_path,
            target_type,
            &drive_name,
            known_file_sibling_names,
        ) {
            return Ok(blocked);
        }

        let folder_id = self
            .context
            .services
            .folders
            .ensure_path(token, cloud_path_segments)
            .await
            .map_err(|e| format!("{e:?}"))?;

        let raw_cloud_files = self
            .context
            .services
            .search
            .list_files(token, &folder_id)
            .await
            .map_err(|e| format!("{e:?}"))?;
        let cloud_files = canonical_cloud_files(&raw_cloud_files)
            .map_err(|e| format!("Duplicate cloud file: {}", e.name))?;

        let all_reserved_names: HashSet<String> = known_file_sibling_names
            .union(reserved_sibling_names)
            .cloned()
            .collect();

        let is_directory = self
            .context
            .mapping
            .is_directory_save_target(
                target_type,
                local_save_path,
                &cloud_files,
                &all_reserved_names,
                cloud_path,
            )
            .map_err(|e| e.to_string())?;

        let active_cloud_files = if is_directory {
            project_directory_files(&cloud_files, cloud_path)
        } else {
            cloud_files
        };

        if is_directory {
            self.upload_directory(
                token,
                local_save_path,
                &folder_id,
                &active_cloud_files,
                &entry.common().name,
                cloud_path,
                known_file_sibling_names,
                reserved_sibling_names,
            )
            .await
        } else {
            let planned = PlannedSave {
                local: local_save_path.clone(),
                remote: active_cloud_files
                    .into_iter()
                    .find(|f| f.name == drive_name),
                folder_id,
                drive_name,
            };
            Ok(self
                .upload_file(token, planned, &entry.common().name, all_reserved_names)
                .await)
        }
    }

    fn blocked_result(
        &self,
        local: &A::Path,
        kind: &SaveTargetType,
        name: &str,
        siblings: &HashSet<String>,
    ) -> Option<PostSyncResult<A::Path>> {
        if !local.exists().unwrap_or(false) {
            return Some(PostSyncResult::default());
        }
        if *kind == SaveTargetType::File && siblings.contains(name) {
            return Some(PostSyncResult {
                completed_count: 0,
                failed_targets: Vec::new(),
                blocked_reasons: vec![format!(
                    "The save name '{name}' is already used by another configured file target."
                )],
            });
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    async fn upload_directory(
        &self,
        token: &str,
        root: &A::Path,
        folder_id: &str,
        cloud_files: &[RemoteFile],
        display_name: &str,
        prefix: Option<&str>,
        siblings: &HashSet<String>,
        reserved: &HashSet<String>,
    ) -> Result<PostSyncResult<A::Path>, String> {
        let mut completed = 0;
        let mut failed = Vec::new();
        let mut blocked = Vec::new();

        let all_reserved: HashSet<String> = siblings
            .union(reserved)
            .map(|name| directory_cloud_name(prefix, name))
            .collect();

        let local_files = if root.is_directory().unwrap_or(false) {
            root.walk_top_down().unwrap_or_default()
        } else {
            Vec::new()
        };

        for file in local_files {
            if file.is_file().unwrap_or(false)
                && let Ok(relative) = file.relative_to(root)
            {
                if siblings.contains(&relative) {
                    blocked.push(format!(
                        "The directory save '{relative}' conflicts with another configured file target. Rename or reconfigure one of the saves before syncing it."
                    ));
                } else if !reserved.contains(&relative) {
                    let planned = PlannedSave {
                        local: file,
                        remote: cloud_files.iter().find(|f| f.name == relative).cloned(),
                        folder_id: folder_id.to_string(),
                        drive_name: directory_cloud_name(prefix, &relative),
                    };
                    let res = self
                        .upload_file(token, planned, display_name, all_reserved.clone())
                        .await;
                    completed += res.completed_count;
                    failed.extend(res.failed_targets);
                }
            }
        }

        Ok(PostSyncResult {
            completed_count: completed,
            failed_targets: failed,
            blocked_reasons: blocked,
        })
    }

    async fn upload_file(
        &self,
        token: &str,
        file: PlannedSave<A::Path>,
        display_name: &str,
        reserved: HashSet<String>,
    ) -> PostSyncResult<A::Path> {
        match self.sync_single_file_post(token, &file, display_name).await {
            Ok(()) => PostSyncResult {
                completed_count: 1,
                failed_targets: Vec::new(),
                blocked_reasons: Vec::new(),
            },
            Err(err) => PostSyncResult {
                completed_count: 0,
                failed_targets: vec![SaveUploadTarget {
                    file: file.local,
                    kind: SaveTargetType::File,
                    drive_name: Some(file.drive_name),
                    failure_reason: Some(err),
                    reserved_drive_names: reserved,
                }],
                blocked_reasons: Vec::new(),
            },
        }
    }

    async fn sync_single_file_post(
        &self,
        token: &str,
        file: &PlannedSave<A::Path>,
        display_name: &str,
    ) -> Result<(), String> {
        let remote_state = file.remote.as_ref().map(remote_to_state);
        let decision = self
            .context
            .services
            .tracked_sync
            .plan(
                file.local.path(),
                remote_state.as_ref(),
                &file.folder_id,
                &file.drive_name,
            )
            .await
            .map_err(|e| format!("{e:?}"))?;

        let choice = if decision == Decision::Conflict
            && let Some(r) = file.remote.as_ref()
        {
            let local_mod = file.local.last_modified_ms().unwrap_or(0);
            let cloud_mod = (r.modified / 1_000_000) as i64;
            self.context
                .request_conflict(local_mod, cloud_mod, display_name.to_string())
                .await
        } else {
            decision
        };

        ManualFileSync::new(self.context)
            .apply_file_decision(token, file, choice)
            .await
    }
}

struct UploadSummary<P> {
    completed_count: usize,
    configured: Vec<SaveTarget>,
    files: Vec<SaveUploadTarget<P>>,
    blocked_reasons: Vec<String>,
    first_failure_reason: Option<String>,
}

impl<P> UploadSummary<P> {
    fn new(completed: usize, blocked: Vec<String>) -> Self {
        Self {
            completed_count: completed,
            configured: Vec::new(),
            files: Vec::new(),
            blocked_reasons: blocked,
            first_failure_reason: None,
        }
    }

    fn accept(&mut self, result: PostSyncResult<P>) {
        self.completed_count += result.completed_count;
        if self.first_failure_reason.is_none() {
            self.first_failure_reason = result
                .failed_targets
                .iter()
                .find_map(|t| t.failure_reason.clone());
        }
        self.files.extend(result.failed_targets);
        self.blocked_reasons.extend(result.blocked_reasons);
    }
}

pub struct PostGameSync<A, L, F, T, Auth, R>
where
    A: LocalSaveAccess,
{
    context: Arc<SyncContext<A, L, F, T, Auth, R>>,
}

impl<A, L, F, T, Auth, R> PostGameSync<A, L, F, T, Auth, R>
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
    pub fn new(context: Arc<SyncContext<A, L, F, T, Auth, R>>) -> Self {
        Self { context }
    }

    pub async fn upload_after_play(
        &self,
        item: &SyncGame,
        sync_targets: &[SaveTarget],
    ) -> Result<bool, String> {
        let summary = UploadSummary::new(0, Vec::new());
        self.upload_batch(item, sync_targets, &[], summary, sync_targets)
            .await
    }

    async fn upload_batch(
        &self,
        item: &SyncGame,
        sync_targets: &[SaveTarget],
        retry_targets: &[SaveUploadTarget<A::Path>],
        mut summary: UploadSummary<A::Path>,
        mapping_targets: &[SaveTarget],
    ) -> Result<bool, String> {
        (self.context.on_status)(SyncEvent::Syncing(format!(
            "Uploading saves for {}...",
            item.effective_title
        )));

        let auth_res = self
            .context
            .services
            .authorization
            .authorize(&self.context.config, false)
            .await;

        match auth_res {
            Ok(token) => {
                let cloud_path = self
                    .context
                    .resolver
                    .get_cloud_path_segments_game(item)
                    .map_err(|e| format!("{e:?}"))?;
                let folder_res = self
                    .context
                    .services
                    .folders
                    .ensure_path(&token, &cloud_path)
                    .await;
                match folder_res {
                    Ok(folder_id) => {
                        let list_res = self
                            .context
                            .services
                            .search
                            .list_files(&token, &folder_id)
                            .await;
                        match list_res {
                            Ok(raw_files) => match canonical_cloud_files(&raw_files) {
                                Ok(cloud_files) => {
                                    if let Err(mapping_err) =
                                        self.context.mapping.validate_save_target_mappings(
                                            mapping_targets,
                                            &cloud_files,
                                        )
                                    {
                                        (self.context.on_status)(SyncEvent::Error(
                                            mapping_err.to_string(),
                                        ));
                                        return Ok(false);
                                    }
                                    let saves = PostSaveSync::new(&self.context);
                                    for target in retry_targets {
                                        if target.file.exists().unwrap_or(false) {
                                            match saves
                                                .sync(
                                                    &token,
                                                    &item.entry,
                                                    &target.file,
                                                    &target.kind,
                                                    &cloud_path,
                                                    target.drive_name.as_deref(),
                                                    &target.reserved_drive_names,
                                                    &HashSet::new(),
                                                    None,
                                                )
                                                .await
                                            {
                                                Ok(res) => summary.accept(res),
                                                Err(err) => {
                                                    if summary.first_failure_reason.is_none() {
                                                        summary.first_failure_reason =
                                                            Some(err.clone());
                                                    }
                                                    summary.files.push(target.clone());
                                                }
                                            }
                                        }
                                    }

                                    for target in sync_targets {
                                        let local = self.context.services.files.path(&target.path);
                                        if !local.exists().unwrap_or(false) {
                                            continue;
                                        }
                                        let siblings = self
                                            .context
                                            .mapping
                                            .file_sibling_cloud_names(
                                                target,
                                                mapping_targets,
                                                &cloud_files,
                                            )
                                            .unwrap_or_default();
                                        let reserved = self
                                            .context
                                            .mapping
                                            .reserved_cloud_names(
                                                target,
                                                mapping_targets,
                                                &cloud_files,
                                            )
                                            .unwrap_or_default();
                                        match saves
                                            .sync(
                                                &token,
                                                &item.entry,
                                                &local,
                                                &target.kind,
                                                &cloud_path,
                                                None,
                                                &siblings,
                                                &reserved,
                                                target.cloud_path.as_deref(),
                                            )
                                            .await
                                        {
                                            Ok(res) => summary.accept(res),
                                            Err(err) => {
                                                if summary.first_failure_reason.is_none() {
                                                    summary.first_failure_reason =
                                                        Some(err.clone());
                                                }
                                                summary.configured.push(target.clone());
                                            }
                                        }
                                    }
                                }
                                Err(dup) => {
                                    summary.first_failure_reason =
                                        Some(format!("Duplicate cloud file: {}", dup.name));
                                    summary.configured.extend(sync_targets.iter().cloned());
                                }
                            },
                            Err(e) => {
                                summary.first_failure_reason = Some(format!("{e:?}"));
                                summary.configured.extend(sync_targets.iter().cloned());
                            }
                        }
                    }
                    Err(e) => {
                        summary.first_failure_reason = Some(format!("{e:?}"));
                        summary.configured.extend(sync_targets.iter().cloned());
                    }
                }
            }
            Err(e) => {
                summary.first_failure_reason = Some(e.to_string());
                summary.configured.extend(sync_targets.iter().cloned());
                summary.files.extend(retry_targets.iter().cloned());
            }
        }

        self.finish_upload(item, summary, mapping_targets).await
    }

    async fn finish_upload(
        &self,
        item: &SyncGame,
        summary: UploadSummary<A::Path>,
        mapping_targets: &[SaveTarget],
    ) -> Result<bool, String> {
        let mut blocked = summary.blocked_reasons.clone();
        blocked.sort();
        blocked.dedup();

        if summary.configured.is_empty() && summary.files.is_empty() {
            if !blocked.is_empty() {
                (self.context.on_status)(SyncEvent::Error(blocked.join("\n")));
                Ok(false)
            } else {
                (self.context.on_status)(SyncEvent::Idle);
                Ok(true)
            }
        } else {
            self.report_failure(item, summary, mapping_targets, blocked);
            Ok(false)
        }
    }

    fn report_failure(
        &self,
        item: &SyncGame,
        summary: UploadSummary<A::Path>,
        mapping_targets: &[SaveTarget],
        blocked: Vec<String>,
    ) {
        let mut reasons = Vec::new();
        if let Some(r) = summary
            .first_failure_reason
            .filter(|s| !s.trim().is_empty())
        {
            reasons.push(r);
        }
        reasons.extend(blocked.clone());
        reasons.sort();
        reasons.dedup();

        let reason = if reasons.is_empty() {
            "Unknown Drive error".to_string()
        } else {
            reasons.join("\n")
        };

        let failure = PostSyncFailure {
            request_id: self.context.next_request_id(),
            game_name: item.effective_title.clone(),
            reason,
            completed_save_count: summary.completed_count,
            pending_save_count: summary.configured.len() + summary.files.len() + blocked.len(),
        };

        let item_clone = item.clone();
        let configured_clone = summary.configured.clone();
        let files_clone = summary.files.clone();
        let mapping_clone = mapping_targets.to_vec();
        let completed = summary.completed_count;
        let blocked_clone = blocked.clone();

        let context_clone = Arc::clone(&self.context);
        // Register retry closure
        self.context
            .report_post_sync_failure(failure, move || async move {
                let post_game = PostGameSync::new(context_clone);
                let sum = UploadSummary {
                    completed_count: completed,
                    configured: Vec::new(),
                    files: Vec::new(),
                    blocked_reasons: blocked_clone,
                    first_failure_reason: None,
                };
                post_game
                    .upload_batch(
                        &item_clone,
                        &configured_clone,
                        &files_clone,
                        sum,
                        &mapping_clone,
                    )
                    .await
                    .map(|_| ())
            });
    }
}
