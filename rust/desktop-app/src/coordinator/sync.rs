use super::{
    DesktopCoordinator,
    composition::{BuiltOrchestrator, drive_connected},
    interaction::AsyncSaveSetupBridge,
};
use crate::events::AppEvent;
use emusync_data_local::DesktopSaveAccess;
use emusync_domain::{
    config::ConfigurationStore,
    execution::{GameLaunchRequest, GameLauncher},
    library::{Emulator, GameEntry, SaveTarget},
    sync::{EntrySyncStatus, LocalSaveAccess, SaveSynchronizer, SyncEvent, SyncGame},
};
use emusync_platform_desktop::GameRunner;
use emusync_presentation::model::{AppStatus, CloudSyncStatus, GameItem};
use std::{io, path::Path, sync::Arc};

pub fn resolve_effective_targets(
    entry: &GameEntry,
    rom_file: Option<&Path>,
) -> io::Result<Vec<SaveTarget>> {
    match entry {
        GameEntry::Emulator(emulator) => {
            if let Some(rom_name) = rom_file
                .and_then(|p| p.file_name())
                .and_then(|f| f.to_str())
            {
                let targets = DesktopSaveAccess.effective_targets_for_rom(emulator, rom_name)?;
                if !targets.is_empty() {
                    return Ok(targets);
                }
            }
            DesktopSaveAccess.effective_targets(entry)
        }
        GameEntry::Native(_) => DesktopSaveAccess.effective_targets(entry),
    }
}

pub struct SyncOperations;

impl SyncOperations {
    pub async fn execute_launch_game(coordinator: DesktopCoordinator, item: GameItem) {
        let config = match coordinator.config_manager.load() {
            Ok(c) => c,
            Err(e) => {
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: format!("Failed to load configuration: {e}"),
                }));
                coordinator.release_admission();
                return;
            }
        };

        // 1. Fetch current entry from config to avoid stale references
        let entry = match config
            .entries
            .iter()
            .find(|e| e.common().name == item.entry.common().name)
        {
            Some(e) => e.clone(),
            None => {
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: "Game category no longer exists in configuration.".into(),
                }));
                coordinator.release_admission();
                return;
            }
        };

        // 2. Resolve effective targets with error preservation
        let targets = match resolve_effective_targets(&entry, item.rom_file.as_deref()) {
            Ok(t) => t,
            Err(e) => {
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: format!("Failed to resolve save targets: {e}"),
                }));
                coordinator.release_admission();
                return;
            }
        };

        if !drive_connected(&config) {
            let effective_title = item.effective_title();
            let steam_launches = Arc::new(coordinator.platform.steam.clone());
            let runner = GameRunner::new(steam_launches);

            coordinator.send_event(AppEvent::StatusChanged(AppStatus::Syncing {
                message: format!("Preparing {effective_title}..."),
            }));

            let req = GameLaunchRequest {
                steam_app_id: entry.common().steam_app_id,
                entry: entry.clone(),
                rom_path: item
                    .rom_file
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
            };

            coordinator.send_event(AppEvent::StatusChanged(AppStatus::Playing));

            match runner.launch(req).await {
                Ok(result) => {
                    if result.exit_code != 0 {
                        coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                            message: format!(
                                "{effective_title} did not start or exited with error code {}.",
                                result.exit_code
                            ),
                        }));
                    } else {
                        coordinator.send_event(AppEvent::StatusChanged(AppStatus::Idle));
                        coordinator.send_toast(format!(
                            "Session finished ({}s)",
                            result.duration_ms / 1000
                        ));
                    }
                }
                Err(err) => {
                    coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                        message: format!("Launch failed: {err:?}"),
                    }));
                }
            }
            coordinator.release_admission();
            return;
        }

        let sync_game = SyncGame {
            name: item.name.clone(),
            entry: entry.clone(),
            rom_path: item
                .rom_file
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            effective_title: item.effective_title(),
            save_targets: targets,
        };

        let built = match coordinator.build_orchestrator(&config) {
            Ok(b) => b,
            Err(e) => {
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: format!("Failed to initialize cloud synchronization: {e}"),
                }));
                coordinator.release_admission();
                return;
            }
        };

        let BuiltOrchestrator {
            orchestrator,
            retained,
            last_event,
            ..
        } = built;

        let save_setup_bridge = AsyncSaveSetupBridge {
            controls: coordinator.interaction.clone(),
        };

        let play_res = orchestrator
            .play_with_sync(sync_game, &save_setup_bridge)
            .await;

        if retained.has_error()
            && let Some(err) = retained.quarantine_and_take_error().await
        {
            coordinator.record_transfer_error(err);
            coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                message: "Sync failed during game session.".into(),
            }));
            coordinator.release_admission();
            return;
        }

        match play_res {
            Ok(()) => {
                if matches!(*last_event.lock().unwrap(), Some(SyncEvent::Idle)) {
                    coordinator.check_sync_entry_background(entry);
                }
            }
            Err(err) => {
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: format!("Synchronization error: {err}"),
                }));
            }
        }

        coordinator.release_admission();
    }

    pub async fn execute_sync_entry(coordinator: DesktopCoordinator, entry: GameEntry) {
        let name = entry.common().name.clone();
        let config = match coordinator.config_manager.load() {
            Ok(c) => c,
            Err(e) => {
                coordinator.send_toast(format!("Failed to load config: {e}"));
                coordinator.release_admission();
                return;
            }
        };

        if !drive_connected(&config) {
            coordinator.send_toast("Google Drive is not configured.");
            coordinator.release_admission();
            return;
        }

        coordinator.send_event(AppEvent::SyncStatusUpdated {
            entry_name: name.clone(),
            status: CloudSyncStatus::Syncing,
        });
        coordinator.send_event(AppEvent::StatusChanged(AppStatus::Syncing {
            message: format!("Synchronizing '{name}'..."),
        }));

        let built = match coordinator.build_orchestrator(&config) {
            Ok(b) => b,
            Err(e) => {
                coordinator.send_event(AppEvent::SyncStatusUpdated {
                    entry_name: name.clone(),
                    status: CloudSyncStatus::Error,
                });
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: format!("Failed to initialize cloud synchronization: {e}"),
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

        let sync_res = orchestrator.sync_entry(&entry).await;
        let transfer_err = retained.quarantine_and_take_error().await;

        if let Some(err) = transfer_err {
            coordinator.record_transfer_error(err);
            coordinator.send_event(AppEvent::SyncStatusUpdated {
                entry_name: name,
                status: CloudSyncStatus::Error,
            });
            coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                message: "Sync failed during transfer. Recovery is required.".into(),
            }));
            coordinator.release_admission();
            return;
        }

        match sync_res {
            Ok(true) => {
                coordinator.send_event(AppEvent::SyncStatusUpdated {
                    entry_name: name.clone(),
                    status: CloudSyncStatus::InSync,
                });
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Idle));
                coordinator.send_toast(format!("'{name}' synchronized."));
            }
            Ok(false) => {
                coordinator.check_sync_entry_background(entry);
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Idle));
            }
            Err(err) => {
                coordinator.send_event(AppEvent::SyncStatusUpdated {
                    entry_name: name,
                    status: CloudSyncStatus::Error,
                });
                coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: format!("Sync failed: {err}"),
                }));
            }
        }

        coordinator.release_admission();
    }

    pub async fn execute_check_sync_entry(coordinator: DesktopCoordinator, entry: GameEntry) {
        let name = entry.common().name.clone();
        let config = match coordinator.config_manager.load() {
            Ok(c) => c,
            Err(_) => return,
        };

        if !drive_connected(&config) {
            coordinator.send_event(AppEvent::SyncStatusUpdated {
                entry_name: name,
                status: CloudSyncStatus::NotConfigured,
            });
            return;
        }

        coordinator.send_event(AppEvent::SyncStatusUpdated {
            entry_name: name.clone(),
            status: CloudSyncStatus::Checking,
        });

        // Use a dedicated check orchestrator that does NOT overwrite interactive controls
        let built = match coordinator.build_check_orchestrator(&config) {
            Ok(b) => b,
            Err(_) => {
                coordinator.send_event(AppEvent::SyncStatusUpdated {
                    entry_name: name,
                    status: CloudSyncStatus::Error,
                });
                return;
            }
        };

        let status = match built.orchestrator.check_entry(&entry).await {
            Ok(EntrySyncStatus::InSync) => CloudSyncStatus::InSync,
            Ok(EntrySyncStatus::OutOfSync) => CloudSyncStatus::OutOfSync,
            Ok(EntrySyncStatus::Conflict) => CloudSyncStatus::Conflict,
            Ok(EntrySyncStatus::NotConfigured) => CloudSyncStatus::NotConfigured,
            Ok(EntrySyncStatus::Idle) => CloudSyncStatus::Idle,
            Ok(EntrySyncStatus::Error) | Err(_) => CloudSyncStatus::Error,
        };

        coordinator.send_event(AppEvent::SyncStatusUpdated {
            entry_name: name,
            status,
        });
    }

    pub async fn execute_open_emulator(coordinator: DesktopCoordinator, emulator: Emulator) {
        let exec = emulator.common.executable_path.trim().to_string();
        if exec.is_empty() {
            coordinator.send_toast("No emulator executable configured.");
            coordinator.release_admission();
            return;
        }

        let steam_launches = Arc::new(coordinator.platform.steam.clone());
        let runner = GameRunner::new(steam_launches);

        if let Err(e) = runner.open_emulator(&emulator).await {
            coordinator.send_toast(format!("Failed to launch emulator: {e}"));
        }

        coordinator.release_admission();
    }
}
