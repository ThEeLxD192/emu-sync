//! Full game launch lifecycle: pre-sync, process execution, offline mode fallback, and post-sync.

use super::context::SyncContext;
use super::legacy_placement::LegacyPlacement;
use super::manual::ManualFileSync;
use super::post_game::PostGameSync;
use super::target_mapping::project_directory_files;
use emusync_domain::execution::{GameLaunchRequest, GameLauncher};
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, LocalSaveAccess, LocalSaveFiles, LocalSavePath, SaveSetup,
    SaveSetupRequest, SyncEvent, SyncGame, SyncMessage, canonical_cloud_files,
};
use std::sync::Arc;

pub(crate) struct GameSyncLifecycle<A, L, F, T, Auth, R>
where
    A: LocalSaveAccess,
{
    context: Arc<SyncContext<A, L, F, T, Auth, R>>,
}

struct PlayPreparation {
    item: SyncGame,
    cloud_path: Vec<String>,
}

impl<A, L, F, T, Auth, R> GameSyncLifecycle<A, L, F, T, Auth, R>
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
    pub(crate) fn new(context: Arc<SyncContext<A, L, F, T, Auth, R>>) -> Self {
        Self { context }
    }

    pub(crate) async fn play_with_sync(
        &self,
        game: SyncGame,
        setup: &(impl SaveSetup + Sync),
    ) -> Result<(), String> {
        let cloud_path = self
            .context
            .resolver
            .get_cloud_path_segments_game(&game)
            .map_err(|e| format!("{e:?}"))?;

        let mut prepared = PlayPreparation {
            item: game,
            cloud_path,
        };

        let has_drive = self
            .context
            .config
            .credentials
            .as_ref()
            .and_then(|c| c.refresh_token.as_ref())
            .is_some();

        let ready = match self.prepare_play(&mut prepared, has_drive, setup).await {
            Ok(r) => r,
            Err(err) => {
                (self.context.on_status)(SyncEvent::Error(SyncMessage::Failed {
                    detail: err.to_string(),
                }));
                return Ok(());
            }
        };

        if ready {
            match self.launch_prepared(&mut prepared, has_drive, setup).await {
                Ok(true) => {
                    (self.context.on_status)(SyncEvent::Idle);
                }
                Ok(false) => {}
                Err(err) => {
                    (self.context.on_status)(SyncEvent::Error(SyncMessage::Failed {
                        detail: err.to_string(),
                    }));
                }
            }
        }

        Ok(())
    }

    async fn prepare_play(
        &self,
        prepared: &mut PlayPreparation,
        has_drive: bool,
        setup: &(impl SaveSetup + Sync),
    ) -> Result<bool, String> {
        if !has_drive {
            return Ok(true);
        }

        (self.context.on_status)(SyncEvent::Syncing(SyncMessage::CheckingCloudSaves));

        let token = match self
            .context
            .services
            .authorization
            .authorize(&self.context.config, false)
            .await
        {
            Ok(tok) => tok,
            Err(_) => {
                // Transport failures allow offline play!
                return Ok(true);
            }
        };

        if !self.ensure_save_setup(&token, prepared, setup).await? {
            return Ok(false);
        }

        if !prepared.item.save_targets.is_empty() {
            (self.context.on_status)(SyncEvent::Syncing(SyncMessage::SyncingGameSaves {
                game: prepared.item.effective_title.clone(),
            }));

            let folder_id = match self
                .context
                .services
                .folders
                .ensure_path(&token, &prepared.cloud_path)
                .await
            {
                Ok(f) => f,
                Err(_) => {
                    // Transport error allows offline play
                    return Ok(true);
                }
            };

            let raw_files = match self
                .context
                .services
                .search
                .list_files(&token, &folder_id)
                .await
            {
                Ok(f) => f,
                Err(_) => {
                    // Transport error allows offline play
                    return Ok(true);
                }
            };

            let cloud_files = match canonical_cloud_files(&raw_files) {
                Ok(cf) => cf,
                Err(dup) => {
                    return Err(format!("Unverified duplicate remote file: {}", dup.name));
                }
            };

            // Mapping errors must NOT allow offline play!
            LegacyPlacement::new(&self.context)
                .validate_restoring_unplaced(
                    &token,
                    &folder_id,
                    &prepared.item.save_targets,
                    &cloud_files,
                    &prepared.item.effective_title,
                )
                .await?;

            let manual = ManualFileSync::new(&self.context);
            for target in &prepared.item.save_targets {
                let local_path = self.context.services.files.path(&target.path);
                let reserved = self
                    .context
                    .mapping
                    .reserved_cloud_names(target, &prepared.item.save_targets, &cloud_files)
                    .map_err(|e| e.to_string())?;
                let is_dir = self
                    .context
                    .mapping
                    .is_directory(target, &cloud_files, &prepared.item.save_targets)
                    .map_err(|e| e.to_string())?;

                let sync_res = if is_dir {
                    let projected =
                        project_directory_files(&cloud_files, target.cloud_path.as_deref());
                    let target_cloud_files = self
                        .context
                        .mapping
                        .cloud_files_for_save_target(
                            &target.kind,
                            &local_path,
                            &projected,
                            &reserved,
                        )
                        .map_err(|e| e.to_string())?;
                    manual
                        .sync_directory_manual(
                            &token,
                            &local_path,
                            &folder_id,
                            &target_cloud_files,
                            &prepared.item.effective_title,
                            &reserved,
                            target.cloud_path.as_deref(),
                            &target.excluded_folders,
                            true,
                        )
                        .await
                } else {
                    let drive_name = self.context.mapping.file_cloud_name(target);
                    manual
                        .sync_file_manual(
                            &token,
                            &local_path,
                            &folder_id,
                            &cloud_files,
                            &prepared.item.effective_title,
                            &drive_name,
                            true,
                        )
                        .await
                };

                sync_res.map_err(|e| {
                    format!(
                        "Could not finish preparing saves for {}. Retry synchronization before playing: {e}",
                        prepared.item.effective_title
                    )
                })?;
            }
        }

        Ok(true)
    }

    async fn ensure_save_setup(
        &self,
        token: &str,
        prepared: &mut PlayPreparation,
        setup: &(impl SaveSetup + Sync),
    ) -> Result<bool, String> {
        if !prepared.item.save_targets.is_empty() {
            return Ok(true);
        }
        let folder_id = match self
            .context
            .services
            .folders
            .find_path(token, &prepared.cloud_path)
            .await
        {
            Ok(f) => f,
            Err(_) => {
                // Transport error allows offline play
                return Ok(true);
            }
        };

        let cloud_files = match folder_id {
            Some(id) => match self.context.services.search.list_files(token, &id).await {
                Ok(f) => f,
                Err(_) => {
                    // Transport error allows offline play
                    return Ok(true);
                }
            },
            None => Vec::new(),
        };

        if cloud_files.is_empty() {
            return Ok(true);
        }

        (self.context.on_status)(SyncEvent::Idle);
        let requested = setup
            .request(SaveSetupRequest {
                game: prepared.item.clone(),
                cloud_file_names: cloud_files.into_iter().map(|f| f.name).collect(),
            })
            .await
            .map_err(|_| "Save setup failed".to_string())?;

        if requested.targets.is_empty() {
            Ok(false)
        } else {
            prepared.item = requested.game;
            prepared.item.save_targets = requested.targets;
            prepared.cloud_path = self
                .context
                .resolver
                .get_cloud_path_segments_game(&prepared.item)
                .map_err(|e| format!("{e:?}"))?;
            Ok(true)
        }
    }

    async fn launch_prepared(
        &self,
        prepared: &mut PlayPreparation,
        has_drive: bool,
        setup: &(impl SaveSetup + Sync),
    ) -> Result<bool, String> {
        (self.context.on_status)(SyncEvent::Playing);
        let entry = &prepared.item.entry;
        let launch_req = GameLaunchRequest {
            entry: entry.clone(),
            rom_path: prepared.item.rom_path.clone(),
            steam_app_id: entry.common().steam_app_id,
        };
        let result = self
            .context
            .services
            .runner
            .launch(launch_req)
            .await
            .map_err(|e| format!("{e:?}"))?;

        if result.exit_code != 0 {
            (self.context.on_status)(SyncEvent::Error(SyncMessage::GameNotTracked {
                game: prepared.item.effective_title.clone(),
            }));
            return Ok(false);
        }

        if prepared.item.save_targets.is_empty()
            && let Ok(requested) = setup
                .request(SaveSetupRequest {
                    game: prepared.item.clone(),
                    cloud_file_names: Vec::new(),
                })
                .await
        {
            prepared.item = requested.game;
            prepared.item.save_targets = requested.targets;
        }

        if !has_drive {
            return Ok(true);
        }

        PostGameSync::new(Arc::clone(&self.context))
            .upload_after_play(&prepared.item, &prepared.item.save_targets)
            .await
    }
}
