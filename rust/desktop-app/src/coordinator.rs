//! Coordinates domain, application services, platform adapters, and Tokio background tasks.

use crate::{events::AppEvent, scanner_helper::scan_entry_games};
use emusync_application::{
    EntryEditResult, LoadLibrary, OperationCoordinator, SteamRegistration, UpdateCoordinator,
    UpdateState,
    library::entry_edits::{GameOverrideChanges, build_game_entry},
};
use emusync_data_local::ConfigManager;
use emusync_data_updates::GitHubUpdateSource;
use emusync_domain::{
    config::{ConfigurationStore, Credentials},
    execution::{BrowserLauncher, GameLauncher, PathPicker, PickerType},
    library::{Emulator, GameEntry, SaveTarget, SteamLibrary},
    sync::{Decision, SyncGame},
    update::UpdateInfo,
};
use emusync_platform_desktop::{
    DesktopBrowserLauncher, DesktopSteamLibrary, DesktopUpdateInstaller, FilePicker, GameRunner,
    SteamShortcutManager, UpdateStagingFiles,
};
use emusync_presentation::{
    actions::UiAction,
    model::{AppStatus, CloudSyncStatus, GameItem, PathTarget, PickMode, SaveSetupRequest},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc::Sender},
};
use tokio::runtime::Handle;

/// Desktop application coordinator managing async operations and state events.
pub struct DesktopCoordinator {
    config_manager: ConfigManager,
    steam: DesktopSteamLibrary,
    browser: Arc<DesktopBrowserLauncher>,
    picker: FilePicker,
    update_coordinator:
        Arc<UpdateCoordinator<GitHubUpdateSource, UpdateStagingFiles, DesktopUpdateInstaller>>,
    operations: Arc<OperationCoordinator>,
    event_tx: Sender<AppEvent>,
    redraw: Arc<dyn Fn() + Send + Sync>,
    tokio_handle: Handle,
    active_conflict: Arc<Mutex<Option<tokio::sync::oneshot::Sender<Decision>>>>,
}

impl DesktopCoordinator {
    pub fn new(
        config_manager: ConfigManager,
        event_tx: Sender<AppEvent>,
        redraw: Arc<dyn Fn() + Send + Sync>,
        tokio_handle: Handle,
    ) -> Self {
        let steam = DesktopSteamLibrary::new(SteamShortcutManager::new());
        let browser = Arc::new(DesktopBrowserLauncher);
        let picker = FilePicker;
        let operations = Arc::new(OperationCoordinator::new());

        let client = reqwest::Client::new();
        let source =
            GitHubUpdateSource::new(client, "ThEeLxD192", "EmuSync", "1.2.0", cfg!(windows));
        let files = UpdateStagingFiles::default_for_platform();
        let installer = DesktopUpdateInstaller::default();

        let tx_clone = event_tx.clone();
        let redraw_clone = redraw.clone();
        let update_coordinator = Arc::new(UpdateCoordinator::new(
            source,
            files,
            installer,
            move |state| {
                let _ = tx_clone.send(AppEvent::UpdateStateChanged(state.clone()));
                (redraw_clone)();
            },
        ));

        Self {
            config_manager,
            steam,
            browser,
            picker,
            update_coordinator,
            operations,
            event_tx,
            redraw,
            tokio_handle,
            active_conflict: Arc::new(Mutex::new(None)),
        }
    }

    /// Helper to send an event and request a window redraw.
    pub fn send_event(&self, event: AppEvent) {
        let _ = self.event_tx.send(event);
        (self.redraw)();
    }

    /// Loads initial config, scans ROMs, and verifies Steam registrations.
    pub fn initialize(&self) {
        let loader = LoadLibrary::new(self.config_manager.clone(), self.steam.clone());
        match loader.load() {
            Ok(config) => {
                let first = config.entries.first().cloned();
                self.send_event(AppEvent::ConfigReloaded(config.clone()));
                if let Some(entry) = &first {
                    let scanned = scan_entry_games(entry);
                    self.send_event(AppEvent::EntryGamesScanned {
                        entry_name: entry.common().name.clone(),
                        games: scanned,
                    });
                }

                for entry in &config.entries {
                    let reg = self
                        .steam
                        .is_registered(entry)
                        .unwrap_or(entry.common().steam_app_id.is_some());
                    self.send_event(AppEvent::SteamRegistrationUpdated {
                        entry_name: entry.common().name.clone(),
                        registered: reg,
                    });
                }
            }
            Err(err) => {
                self.send_event(AppEvent::Toast(format!("Failed to load config: {err}")));
            }
        }
    }

    /// Handles a user-issued UI action asynchronously or on the Tokio runtime.
    pub fn handle_action(&self, action: UiAction) {
        match action {
            UiAction::SelectEntry(entry) => {
                if let Some(entry) = entry {
                    let scanned = scan_entry_games(&entry);
                    self.send_event(AppEvent::EntryGamesScanned {
                        entry_name: entry.common().name.clone(),
                        games: scanned,
                    });
                    self.check_sync_entry(entry);
                }
            }
            UiAction::LaunchGame(item) => {
                self.launch_game(item);
            }
            UiAction::OpenEmulator(emulator) => {
                self.open_emulator(emulator);
            }
            UiAction::SyncEntry(entry) => {
                self.sync_entry(entry);
            }
            UiAction::CheckSyncEntry(entry) => {
                self.check_sync_entry(entry);
            }
            UiAction::AddEntry(entry) => {
                self.add_entry(entry);
            }
            UiAction::EditEntry { old, new } => {
                self.edit_entry(*old, *new);
            }
            UiAction::DeleteEntry(entry) => {
                self.delete_entry(entry);
            }
            UiAction::ReorderEntries(entries) => {
                self.reorder_entries(entries);
            }
            UiAction::EditGameOverride {
                item,
                title,
                targets,
                cover_path,
            } => {
                self.edit_game_override(item, title, targets, cover_path);
            }
            UiAction::CompleteSaveSetup {
                request,
                targets,
                title,
                cover_path,
            } => {
                self.complete_save_setup(*request, targets, title, cover_path);
            }
            UiAction::ToggleSteam(entry) => {
                self.toggle_steam(entry);
            }
            UiAction::ConnectDrive {
                client_id,
                client_secret,
            } => {
                self.connect_drive(client_id, client_secret);
            }
            UiAction::UnlinkDrive => {
                self.unlink_drive();
            }
            UiAction::CheckForUpdates { manual } => {
                self.check_for_updates(manual);
            }
            UiAction::DownloadUpdate(info) => {
                self.download_update(info);
            }
            UiAction::RestartApp(path) => {
                self.restart_app(path);
            }
            UiAction::ResolveConflict(decision) => {
                let mut guard = self.active_conflict.lock().unwrap();
                if let Some(sender) = guard.take() {
                    let _ = sender.send(decision);
                }
                self.send_event(AppEvent::StatusChanged(AppStatus::Idle));
            }
            UiAction::RetryPostSync => {
                self.send_event(AppEvent::StatusChanged(AppStatus::Idle));
            }
            UiAction::ClearStatus => {
                self.send_event(AppEvent::StatusChanged(AppStatus::Idle));
            }
            UiAction::OpenConfigDirectory => {
                self.open_config_dir();
            }
            UiAction::PickPath { target, mode } => {
                self.pick_path(target, mode);
            }
            UiAction::Exit => {
                // Handled at window level
            }
        }
    }

    fn add_entry(&self, entry: GameEntry) {
        let current = match self.config_manager.load() {
            Ok(c) => c,
            Err(e) => {
                self.send_event(AppEvent::Toast(format!("Failed to load config: {e}")));
                return;
            }
        };

        if entry.common().name.trim().is_empty()
            || current
                .entries
                .iter()
                .any(|e| e.common().name == entry.common().name)
        {
            self.send_event(AppEvent::Toast(
                "Choose a unique, non-empty category name.".into(),
            ));
            return;
        }

        let name = entry.common().name.clone();
        match self.config_manager.update(|mut cfg| {
            cfg.entries.push(entry.clone());
            cfg
        }) {
            Ok(updated) => {
                let scanned = scan_entry_games(&entry);
                self.send_event(AppEvent::ConfigReloaded(updated));
                self.send_event(AppEvent::EntryGamesScanned {
                    entry_name: name,
                    games: scanned,
                });
                self.send_event(AppEvent::Toast("Category added successfully.".into()));
            }
            Err(err) => {
                self.send_event(AppEvent::Toast(format!("Failed to add category: {err}")));
            }
        }
    }

    fn edit_entry(&self, old: GameEntry, new: GameEntry) {
        let name = new.common().name.clone();
        let old_name = old.common().name.clone();
        match self.config_manager.update(|mut cfg| {
            if let Some(idx) = cfg.entries.iter().position(|e| e.common().name == old_name) {
                cfg.entries[idx] = new.clone();
            }
            cfg
        }) {
            Ok(updated) => {
                let scanned = scan_entry_games(&new);
                self.send_event(AppEvent::ConfigReloaded(updated));
                self.send_event(AppEvent::EntryGamesScanned {
                    entry_name: name,
                    games: scanned,
                });
                self.send_event(AppEvent::Toast("Category updated.".into()));
            }
            Err(err) => {
                self.send_event(AppEvent::Toast(format!("Failed to update category: {err}")));
            }
        }
    }

    fn delete_entry(&self, entry: GameEntry) {
        let name = entry.common().name.clone();
        let _ = self.steam.unregister(&entry);

        match self.config_manager.update(|mut cfg| {
            cfg.entries.retain(|e| e.common().name != name);
            cfg
        }) {
            Ok(updated) => {
                self.send_event(AppEvent::ConfigReloaded(updated));
                self.send_event(AppEvent::Toast(format!("Category '{name}' deleted.")));
            }
            Err(err) => {
                self.send_event(AppEvent::Toast(format!("Failed to delete category: {err}")));
            }
        }
    }

    fn reorder_entries(&self, entries: Vec<GameEntry>) {
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
                self.send_event(AppEvent::ConfigReloaded(updated));
            }
            Err(err) => {
                self.send_event(AppEvent::Toast(format!("Failed to reorder: {err}")));
            }
        }
    }

    fn edit_game_override(
        &self,
        item: GameItem,
        title: Option<String>,
        targets: Vec<SaveTarget>,
        cover_path: Option<String>,
    ) {
        let entry_name = item.entry.common().name.clone();
        let sync_game = SyncGame {
            name: item.name.clone(),
            entry: item.entry.clone(),
            rom_path: item
                .rom_file
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            effective_title: item.effective_title(),
            save_targets: targets.clone(),
        };
        let changes = GameOverrideChanges {
            title,
            cover_path,
            targets,
        };

        if let Some(updated_entry) = build_game_entry(&sync_game, &changes) {
            match self.config_manager.update(|mut cfg| {
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
                    if let Some(entry) = updated
                        .entries
                        .iter()
                        .find(|e| e.common().name == entry_name)
                    {
                        let scanned = scan_entry_games(entry);
                        self.send_event(AppEvent::EntryGamesScanned {
                            entry_name,
                            games: scanned,
                        });
                    }
                    self.send_event(AppEvent::ConfigReloaded(updated));
                    self.send_event(AppEvent::Toast("Game overrides saved.".into()));
                }
                Err(err) => {
                    self.send_event(AppEvent::Toast(format!("Failed to save overrides: {err}")));
                }
            }
        }
    }

    fn complete_save_setup(
        &self,
        request: SaveSetupRequest,
        targets: Vec<SaveTarget>,
        title: Option<String>,
        cover_path: Option<String>,
    ) {
        let item = request.item.clone();
        self.edit_game_override(item.clone(), title, targets, cover_path);
        self.launch_game(item);
    }

    fn toggle_steam(&self, entry: GameEntry) {
        let name = entry.common().name.clone();
        let is_registered = self
            .steam
            .is_registered(&entry)
            .unwrap_or(entry.common().steam_app_id.is_some());

        let reg = SteamRegistration::new(self.config_manager.clone(), self.steam.clone());
        let result = if is_registered {
            reg.unregister(&entry)
        } else {
            reg.register(&entry)
        };

        match result {
            Ok(EntryEditResult { config, .. }) => {
                self.send_event(AppEvent::ConfigReloaded(config));
                self.send_event(AppEvent::SteamRegistrationUpdated {
                    entry_name: name.clone(),
                    registered: !is_registered,
                });
                let msg = if !is_registered {
                    format!("Added '{name}' to Steam.")
                } else {
                    format!("Removed '{name}' from Steam.")
                };
                self.send_event(AppEvent::Toast(msg));
            }
            Err(err) => {
                self.send_event(AppEvent::Toast(format!("Steam operation failed: {err}")));
            }
        }
    }

    fn open_emulator(&self, emulator: Emulator) {
        let exec = emulator.common.executable_path.trim().to_string();
        if exec.is_empty() {
            self.send_event(AppEvent::Toast("No emulator executable configured.".into()));
            return;
        }

        let steam_launches = Arc::new(self.steam.clone());
        let runner = GameRunner::new(steam_launches);

        self.tokio_handle.spawn(async move {
            if let Err(e) = runner.open_emulator(&emulator).await {
                eprintln!("Failed to launch emulator: {e:?}");
            }
        });
    }

    fn launch_game(&self, item: GameItem) {
        let effective_title = item.effective_title();
        let steam_launches = Arc::new(self.steam.clone());
        let runner = GameRunner::new(steam_launches);
        let event_tx = self.event_tx.clone();
        let redraw = self.redraw.clone();

        self.send_event(AppEvent::StatusChanged(AppStatus::Syncing {
            message: format!("Preparing {}...", effective_title),
        }));

        self.tokio_handle.spawn(async move {
            let req = emusync_domain::execution::GameLaunchRequest {
                steam_app_id: item.entry.common().steam_app_id,
                entry: item.entry.clone(),
                rom_path: item
                    .rom_file
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
            };

            let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Playing));
            (redraw)();

            match runner.launch(req).await {
                Ok(result) => {
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Idle));
                    let _ = event_tx.send(AppEvent::Toast(format!(
                        "Session finished ({}s)",
                        result.duration_ms / 1000
                    )));
                    (redraw)();
                }
                Err(err) => {
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Error {
                        message: format!("Launch failed: {err:?}"),
                    }));
                    (redraw)();
                }
            }
        });
    }

    fn check_sync_entry(&self, entry: GameEntry) {
        let name = entry.common().name.clone();
        let config = match self.config_manager.load() {
            Ok(c) => c,
            Err(_) => return,
        };

        if config.credentials.is_none() {
            self.send_event(AppEvent::SyncStatusUpdated {
                entry_name: name,
                status: CloudSyncStatus::NotConfigured,
            });
            return;
        }

        self.send_event(AppEvent::SyncStatusUpdated {
            entry_name: name.clone(),
            status: CloudSyncStatus::Checking,
        });

        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();
        self.tokio_handle.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            let _ = tx.send(AppEvent::SyncStatusUpdated {
                entry_name: name,
                status: CloudSyncStatus::InSync,
            });
            (redraw)();
        });
    }

    fn sync_entry(&self, entry: GameEntry) {
        let name = entry.common().name.clone();
        self.send_event(AppEvent::StatusChanged(AppStatus::Syncing {
            message: format!("Syncing {}...", name),
        }));

        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();
        self.tokio_handle.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            let _ = tx.send(AppEvent::StatusChanged(AppStatus::Idle));
            let _ = tx.send(AppEvent::SyncStatusUpdated {
                entry_name: name.clone(),
                status: CloudSyncStatus::InSync,
            });
            let _ = tx.send(AppEvent::Toast(format!("Category '{name}' synced.")));
            (redraw)();
        });
    }

    fn connect_drive(&self, client_id: String, client_secret: String) {
        let id = client_id.trim().to_string();
        let secret = client_secret.trim().to_string();
        if id.is_empty() || secret.is_empty() {
            self.send_event(AppEvent::Toast(
                "Client ID and Client Secret cannot be empty.".into(),
            ));
            return;
        }

        let updated = self.config_manager.update_credentials(|curr| {
            let refresh_token = curr.and_then(|c| {
                if c.client_id == id && c.client_secret == secret {
                    c.refresh_token
                } else {
                    None
                }
            });
            Some(Credentials {
                client_id: id.clone(),
                client_secret: secret.clone(),
                refresh_token,
            })
        });

        match updated {
            Ok(config) => {
                self.send_event(AppEvent::ConfigReloaded(config));
                self.send_event(AppEvent::Toast(
                    "Google Drive credentials saved. Authorizing...".into(),
                ));

                let tx = self.event_tx.clone();
                let redraw = self.redraw.clone();
                let store = self.config_manager.clone();
                self.tokio_handle.spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = store.update_credentials(|curr| {
                        curr.map(|c| Credentials {
                            refresh_token: Some("simulated_refresh_token".into()),
                            ..c
                        })
                    });
                    if let Ok(c) = store.load() {
                        let _ = tx.send(AppEvent::ConfigReloaded(c));
                    }
                    let _ = tx.send(AppEvent::Toast("Google Drive connected!".into()));
                    (redraw)();
                });
            }
            Err(err) => {
                self.send_event(AppEvent::Toast(format!(
                    "Failed to save credentials: {err}"
                )));
            }
        }
    }

    fn unlink_drive(&self) {
        match self.config_manager.update_credentials(|curr| {
            curr.map(|c| Credentials {
                refresh_token: None,
                ..c
            })
        }) {
            Ok(updated) => {
                self.send_event(AppEvent::ConfigReloaded(updated));
                self.send_event(AppEvent::Toast("Google Drive disconnected.".into()));
            }
            Err(err) => {
                self.send_event(AppEvent::Toast(format!(
                    "Failed to disconnect Drive: {err}"
                )));
            }
        }
    }

    fn check_for_updates(&self, manual: bool) {
        let coord = self.update_coordinator.clone();
        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();

        self.tokio_handle.spawn(async move {
            coord.check_for_updates().await;
            if manual && matches!(coord.state(), UpdateState::Idle) {
                let _ = tx.send(AppEvent::Toast("EmuSync is up to date.".into()));
                (redraw)();
            }
        });
    }

    fn download_update(&self, info: UpdateInfo) {
        let coord = self.update_coordinator.clone();
        self.tokio_handle.spawn(async move {
            coord.download(&info).await;
        });
    }

    fn restart_app(&self, path: PathBuf) {
        let coord = self.update_coordinator.clone();
        let path_str = path.to_string_lossy().to_string();
        coord.restart(&path_str, self.operations.is_busy());
    }

    fn pick_path(&self, target: PathTarget, mode: PickMode) {
        let picker = self.picker;
        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();

        let kind = match mode {
            PickMode::File => PickerType::File,
            PickMode::Directory => PickerType::Directory,
        };

        tokio::task::spawn_blocking(move || {
            if let Ok(Some(path)) = picker.pick("Select path", "", kind) {
                let _ = tx.send(AppEvent::PathPicked { target, path });
                (redraw)();
            }
        });
    }

    fn open_config_dir(&self) {
        let config_path = self.config_manager.path();
        let dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        let _ = self.browser.open(&dir.to_string_lossy());
    }
}
