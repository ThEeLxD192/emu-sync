use super::{
    DesktopCoordinator,
    composition::{BuiltOrchestrator, drive_connected},
    platform::PlatformServices,
};
use crate::events::AppEvent;
use emusync_application::library::{
    GameOverrideChanges, build_game_entry, preserve_shared_cloud_root,
};
use emusync_data_local::ConfigManager;
use emusync_domain::{
    config::{AppConfig, ConfigurationStore},
    library::{GameEntry, SaveTarget, SteamLibrary},
    sync::{SaveSynchronizer, SyncGame},
};
use emusync_presentation::i18n::fill;
use emusync_presentation::model::{AppStatus, GameItem};

pub struct LibraryManager {
    pub config_manager: ConfigManager,
    pub platform: PlatformServices,
}

impl LibraryManager {
    pub fn add_entry(&self, coordinator: &DesktopCoordinator, entry: GameEntry) {
        let current = match self.config_manager.load() {
            Ok(c) => c,
            Err(e) => {
                coordinator.send_toast(fill(
                    coordinator.t().msg_config_load_failed,
                    &[("error", &e)],
                ));
                return;
            }
        };

        if entry.common().name.trim().is_empty()
            || current
                .entries
                .iter()
                .any(|e| e.common().name == entry.common().name)
        {
            coordinator.send_toast(coordinator.t().msg_category_name_unique);
            return;
        }

        let name = entry.common().name.clone();
        match self.config_manager.update(|mut cfg| {
            if !cfg.entries.iter().any(|e| e.common().name == name) {
                cfg.entries.push(entry.clone());
            }
            cfg
        }) {
            Ok(updated) => {
                coordinator.send_event(AppEvent::ConfigReloaded(updated));
                let _ = self.platform.scan_entry_games_async(entry);
                coordinator.send_toast(coordinator.t().msg_category_added);
            }
            Err(err) => {
                coordinator.send_toast(fill(
                    coordinator.t().msg_category_add_failed,
                    &[("error", &err)],
                ));
            }
        }
    }

    pub fn edit_entry(coordinator: &DesktopCoordinator, old: GameEntry, new: GameEntry) {
        let old_name = old.common().name.clone();
        let config = match coordinator.config_manager.load() {
            Ok(c) => c,
            Err(e) => {
                coordinator.send_toast(fill(
                    coordinator.t().msg_config_load_failed,
                    &[("error", &e)],
                ));
                coordinator.release_admission();
                return;
            }
        };

        // 1. Validate presence and uniqueness
        let Some(current_old) = config.entries.iter().find(|e| e.common().name == old_name) else {
            coordinator.send_toast(coordinator.t().msg_category_missing);
            coordinator.release_admission();
            return;
        };
        let new_name = new.common().name.trim().to_string();
        if new_name.is_empty() {
            coordinator.send_toast(coordinator.t().msg_category_name_empty);
            coordinator.release_admission();
            return;
        }
        if config
            .entries
            .iter()
            .any(|e| e.common().name != old_name && e.common().name == new_name)
        {
            coordinator.send_toast(coordinator.t().msg_category_name_taken);
            coordinator.release_admission();
            return;
        }

        let mut normalized_new = new.clone();
        preserve_shared_cloud_root(&config, current_old, &mut normalized_new);

        let rename_needed = drive_connected(&config)
            && emusync_domain::sync::cloud_path_segments(current_old, None, None, &[], &[])
                != emusync_domain::sync::cloud_path_segments(&normalized_new, None, None, &[], &[]);

        if !rename_needed {
            let res = coordinator.config_manager.update(|mut cfg| {
                if let Some(idx) = cfg.entries.iter().position(|e| e.common().name == old_name) {
                    cfg.entries[idx] = normalized_new.clone();
                }
                cfg
            });
            match res {
                Ok(updated) => {
                    if normalized_new.common().steam_app_id.is_some() {
                        let _ = coordinator.platform.steam.register(&normalized_new);
                    }
                    let _ = coordinator
                        .platform
                        .scan_entry_games_async(normalized_new.clone());
                    coordinator.send_event(AppEvent::ConfigReloaded(updated));
                    coordinator.send_toast(coordinator.t().msg_category_updated);
                }
                Err(err) => {
                    coordinator.send_toast(fill(
                        coordinator.t().msg_category_update_failed,
                        &[("error", &err)],
                    ));
                }
            }
            coordinator.release_admission();
            return;
        }

        coordinator.send_event(AppEvent::StatusChanged(AppStatus::Syncing {
            message: fill(
                coordinator.t().msg_migrating_folder,
                &[("old", &old_name), ("new", &new_name)],
            ),
        }));

        let coord = coordinator.clone();
        let old = current_old.clone();
        let handle = coordinator.tokio_handle.spawn(async move {
            Self::execute_cloud_migration_entry(coord, old, normalized_new, config).await;
        });
        coordinator.track_task(handle);
    }

    async fn execute_cloud_migration_entry(
        coordinator: DesktopCoordinator,
        old: GameEntry,
        normalized_new: GameEntry,
        config: AppConfig,
    ) {
        let old_name = old.common().name.clone();
        let new_name = normalized_new.common().name.clone();

        let built = match coordinator.build_orchestrator(&config) {
            Ok(b) => b,
            Err(e) => {
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: fill(coordinator.t().msg_migration_failed, &[("error", &e)]),
                }));
                coordinator.release_admission();
                return;
            }
        };

        let BuiltOrchestrator {
            orchestrator,
            retained,
            ..
        } = built;

        let migration_res = orchestrator.rename_entry(&old, &normalized_new).await;
        let transfer_err = retained.quarantine_and_take_error().await;

        if let Some(err) = transfer_err {
            coordinator.record_transfer_error(err);
            coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                message: coordinator.t().msg_migration_failed_transfer.into(),
            }));
            coordinator.release_admission();
            return;
        }

        match migration_res {
            Ok(true) => {
                match coordinator.config_manager.update(|mut cfg| {
                    if let Some(idx) = cfg.entries.iter().position(|e| e.common().name == old_name)
                    {
                        cfg.entries[idx] = normalized_new.clone();
                    }
                    cfg
                }) {
                    Ok(updated) => {
                        if normalized_new.common().steam_app_id.is_some() {
                            let _ = coordinator.platform.steam.register(&normalized_new);
                        }
                        let _ = coordinator
                            .platform
                            .scan_entry_games_async(normalized_new.clone());
                        coordinator.send_event(AppEvent::ConfigReloaded(updated));
                        coordinator.send_event(AppEvent::StatusChanged(AppStatus::Idle));
                        coordinator.send_toast(coordinator.t().msg_category_renamed);
                    }
                    Err(err) => {
                        coordinator.record_pending_recovery(fill(
                            coordinator.t().msg_remote_renamed_local_failed,
                            &[("new", &new_name), ("error", &err)],
                        ));
                        coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                            message: fill(
                                coordinator.t().msg_renamed_save_failed,
                                &[("error", &err)],
                            ),
                        }));
                    }
                }
            }
            Ok(false) => {
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: coordinator.t().msg_migration_failed_unchanged.into(),
                }));
            }
            Err(err) => {
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: fill(
                        coordinator.t().msg_migration_error_unchanged,
                        &[("error", &err)],
                    ),
                }));
            }
        }

        coordinator.release_admission();
    }

    pub fn delete_entry(&self, coordinator: &DesktopCoordinator, entry: GameEntry) {
        let name = entry.common().name.clone();

        let is_registered = self
            .platform
            .steam
            .is_registered(&entry)
            .unwrap_or(entry.common().steam_app_id.is_some());
        if is_registered {
            if let Err(err) = self.platform.steam.unregister(&entry) {
                coordinator.send_toast(fill(
                    coordinator.t().msg_steam_unregister_failed,
                    &[("error", &err)],
                ));
                coordinator.release_admission();
                return;
            }
            coordinator.send_event(AppEvent::SteamRegistrationUpdated {
                entry_name: name.clone(),
                registered: false,
            });
        }

        match self.config_manager.update(|mut cfg| {
            cfg.entries.retain(|e| e.common().name != name);
            cfg
        }) {
            Ok(updated) => {
                coordinator.send_event(AppEvent::ConfigReloaded(updated));
                coordinator.send_toast(fill(
                    coordinator.t().msg_category_deleted,
                    &[("name", &name)],
                ));
            }
            Err(err) => {
                coordinator.send_toast(fill(
                    coordinator.t().msg_category_delete_failed,
                    &[("error", &err)],
                ));
            }
        }
        coordinator.release_admission();
    }

    pub fn reorder_entries(&self, coordinator: &DesktopCoordinator, entries: Vec<GameEntry>) {
        match self.config_manager.update(|mut cfg| {
            let mut ordered = Vec::new();
            for target in &entries {
                if let Some(existing) = cfg
                    .entries
                    .iter()
                    .find(|e| e.common().name == target.common().name)
                    && !ordered
                        .iter()
                        .any(|e: &GameEntry| e.common().name == existing.common().name)
                {
                    ordered.push(existing.clone());
                }
            }
            for candidate in &cfg.entries {
                if !ordered
                    .iter()
                    .any(|e| e.common().name == candidate.common().name)
                {
                    ordered.push(candidate.clone());
                }
            }
            cfg.entries = ordered;
            cfg
        }) {
            Ok(updated) => {
                coordinator.send_event(AppEvent::ConfigReloaded(updated));
            }
            Err(err) => {
                coordinator
                    .send_toast(fill(coordinator.t().msg_reorder_failed, &[("error", &err)]));
            }
        }
        coordinator.release_admission();
    }

    pub fn edit_game_override(
        coordinator: &DesktopCoordinator,
        item: GameItem,
        title: Option<String>,
        targets: Vec<SaveTarget>,
        cover_path: Option<String>,
    ) {
        let entry_name = item.entry.common().name.clone();
        let config = match coordinator.config_manager.load() {
            Ok(c) => c,
            Err(e) => {
                coordinator.send_toast(fill(
                    coordinator.t().msg_config_load_failed,
                    &[("error", &e)],
                ));
                coordinator.release_admission();
                return;
            }
        };

        let Some(current_entry) = config
            .entries
            .iter()
            .find(|e| e.common().name == entry_name)
        else {
            coordinator.send_toast(coordinator.t().msg_category_missing);
            coordinator.release_admission();
            return;
        };

        let rom_path_str = item
            .rom_file
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        let current_game = SyncGame {
            name: item.name.clone(),
            entry: current_entry.clone(),
            rom_path: rom_path_str.clone(),
            effective_title: item.effective_title(),
            save_targets: targets.clone(),
        };

        let changes = GameOverrideChanges {
            title: title.clone(),
            cover_path,
            targets: targets.clone(),
        };

        let updated_entry = build_game_entry(&current_game, &changes);
        let old_effective_title = item.effective_title();
        let new_effective_title = match &updated_entry {
            GameEntry::Emulator(emu) => {
                let rom_name = item
                    .rom_file
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .and_then(|f| f.to_str())
                    .unwrap_or(&item.name);
                emu.effective_title(rom_name, &item.name)
            }
            GameEntry::Native(nat) => nat.common.name.clone(),
        };

        let is_native = matches!(current_entry, GameEntry::Native(_));
        if is_native {
            let new_name = updated_entry.common().name.clone();
            if new_name != entry_name && config.entries.iter().any(|e| e.common().name == new_name)
            {
                coordinator.send_toast(coordinator.t().msg_category_name_taken);
                coordinator.release_admission();
                return;
            }
        }

        let title_changed = old_effective_title != new_effective_title;
        let rename_needed = drive_connected(&config)
            && if is_native {
                emusync_domain::sync::cloud_path_segments(current_entry, None, None, &[], &[])
                    != emusync_domain::sync::cloud_path_segments(
                        &updated_entry,
                        None,
                        None,
                        &[],
                        &[],
                    )
            } else {
                title_changed
            };

        if !rename_needed {
            match coordinator.config_manager.update(|mut cfg| {
                if let Some(e) = cfg
                    .entries
                    .iter_mut()
                    .find(|e| e.common().name == entry_name)
                {
                    *e = updated_entry.clone();
                }
                cfg
            }) {
                Ok(updated) => {
                    let _ = coordinator
                        .platform
                        .scan_entry_games_async(updated_entry.clone());
                    coordinator.send_event(AppEvent::ConfigReloaded(updated));
                    coordinator.send_toast(coordinator.t().msg_overrides_saved);
                }
                Err(err) => {
                    coordinator.send_toast(fill(
                        coordinator.t().msg_overrides_failed,
                        &[("error", &err)],
                    ));
                }
            }
            coordinator.release_admission();
            return;
        }

        coordinator.send_event(AppEvent::StatusChanged(AppStatus::Syncing {
            message: fill(
                coordinator.t().msg_migrating_folder,
                &[("old", &old_effective_title), ("new", &new_effective_title)],
            ),
        }));

        let coord = coordinator.clone();
        let current_entry = current_entry.clone();
        let handle = coordinator.tokio_handle.spawn(async move {
            Self::execute_cloud_migration_game_override(
                coord,
                item,
                current_entry,
                updated_entry,
                old_effective_title,
                new_effective_title,
                targets,
                config,
                is_native,
            )
            .await;
        });
        coordinator.track_task(handle);
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_cloud_migration_game_override(
        coordinator: DesktopCoordinator,
        item: GameItem,
        current_entry: GameEntry,
        updated_entry: GameEntry,
        old_effective_title: String,
        new_effective_title: String,
        targets: Vec<SaveTarget>,
        config: AppConfig,
        is_native: bool,
    ) {
        let entry_name = item.entry.common().name.clone();
        let built = match coordinator.build_orchestrator(&config) {
            Ok(b) => b,
            Err(e) => {
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: fill(coordinator.t().msg_migration_failed, &[("error", &e)]),
                }));
                coordinator.release_admission();
                return;
            }
        };

        let BuiltOrchestrator {
            orchestrator,
            retained,
            ..
        } = built;

        let migration_res = if is_native {
            orchestrator
                .rename_entry(&current_entry, &updated_entry)
                .await
        } else {
            let rom_filename = item
                .rom_file
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|f| f.to_str());
            orchestrator
                .rename_game(
                    &updated_entry,
                    rom_filename,
                    &old_effective_title,
                    &new_effective_title,
                    &targets,
                )
                .await
        };

        let transfer_err = retained.quarantine_and_take_error().await;
        if let Some(err) = transfer_err {
            coordinator.record_transfer_error(err);
            coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                message: coordinator.t().msg_migration_failed_transfer.into(),
            }));
            coordinator.release_admission();
            return;
        }

        match migration_res {
            Ok(true) => {
                match coordinator.config_manager.update(|mut cfg| {
                    if let Some(e) = cfg
                        .entries
                        .iter_mut()
                        .find(|e| e.common().name == entry_name)
                    {
                        *e = updated_entry.clone();
                    }
                    cfg
                }) {
                    Ok(updated) => {
                        let _ = coordinator
                            .platform
                            .scan_entry_games_async(updated_entry.clone());
                        coordinator.send_event(AppEvent::ConfigReloaded(updated));
                        coordinator.send_event(AppEvent::StatusChanged(AppStatus::Idle));
                        coordinator.send_toast(coordinator.t().msg_overrides_renamed);
                    }
                    Err(err) => {
                        coordinator.record_pending_recovery(fill(
                            coordinator.t().msg_game_remote_renamed_local_failed,
                            &[("new", &new_effective_title), ("error", &err)],
                        ));
                        coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                            message: fill(
                                coordinator.t().msg_renamed_save_failed,
                                &[("error", &err)],
                            ),
                        }));
                    }
                }
            }
            Ok(false) => {
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: coordinator.t().msg_game_migration_failed_unchanged.into(),
                }));
            }
            Err(err) => {
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: fill(
                        coordinator.t().msg_game_migration_error_unchanged,
                        &[("error", &err)],
                    ),
                }));
            }
        }

        coordinator.release_admission();
    }
}
