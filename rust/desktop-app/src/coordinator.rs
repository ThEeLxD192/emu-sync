//! Coordinates domain, application services, platform adapters, and Tokio background tasks.

use crate::{
    events::AppEvent, guarded_files::GuardedFiles, retained_files::RetainedFiles,
    scanner_helper::scan_entry_games,
};
use emusync_application::{
    library::{
        EntryEditResult, GameOverrideChanges, LoadLibrary, SteamRegistration, build_game_entry,
    },
    operation::OperationCoordinator,
    sync::{SyncOrchestrator, SyncOrchestratorFactory, SyncServices},
};
use emusync_data_drive::{
    DriveFolders, DriveHttpPolicy, DriveListing, DriveTransfer, InteractiveOAuth, TransferProgress,
};
use emusync_data_local::{
    ConfigManager, DesktopSaveAccess, RecoveryCredentials, RecoveryStore, SyncHistoryStore,
    TrackedFileSync,
};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, CredentialStore, Credentials},
    execution::{BrowserLauncher, GameLauncher, PathPicker, PickerType},
    library::{Emulator, GameEntry, SaveTarget, SteamLibrary},
    sync::{
        Decision, EntrySyncStatus, LocalSaveAccess, SaveConflict, SaveSetup, SaveSetupRequest,
        SaveSetupResult, SaveSynchronizer, SaveSynchronizerFactory, SyncEvent, SyncGame,
    },
};
use emusync_platform_desktop::{
    DesktopBrowserLauncher, DesktopSteamLibrary, FilePicker, GameRunner, SteamShortcutManager,
};
use emusync_presentation::{
    actions::UiAction,
    model::{
        AppStatus, CloudSyncStatus, GameItem, PathTarget, PickMode,
        SaveSetupRequest as UiSaveSetupRequest,
    },
};
use reqwest::Url;
use std::{
    future::Future,
    net::SocketAddr,
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex, mpsc::Sender},
};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

fn format_epoch_ms(ms: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map(|dt| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                dt.year(),
                dt.month() as u8,
                dt.day(),
                dt.hour(),
                dt.minute()
            )
        })
        .unwrap_or_else(|_| format!("{ms} ms"))
}

fn drive_connected(config: &AppConfig) -> bool {
    config
        .credentials
        .as_ref()
        .and_then(|credentials| credentials.refresh_token.as_deref())
        .is_some_and(|token| !token.trim().is_empty())
}

/// Endpoint configuration for Drive and OAuth, allowing mock injection during tests.
#[derive(Clone, Debug)]
pub struct DriveEndpoints {
    pub files_v3: Url,
    pub upload_v3: Url,
    pub files_v2: Url,
    pub upload_v2: Url,
    pub token_endpoint: Url,
    pub authorization_endpoint: Url,
    pub oauth_address: SocketAddr,
    pub redirect_uri: String,
}

impl Default for DriveEndpoints {
    fn default() -> Self {
        Self {
            files_v3: Url::parse("https://www.googleapis.com/drive/v3/files").expect("static URL"),
            upload_v3: Url::parse("https://www.googleapis.com/upload/drive/v3/files")
                .expect("static URL"),
            files_v2: Url::parse("https://www.googleapis.com/drive/v2/files").expect("static URL"),
            upload_v2: Url::parse("https://www.googleapis.com/upload/drive/v2/files")
                .expect("static URL"),
            token_endpoint: Url::parse("https://oauth2.googleapis.com/token").expect("static URL"),
            authorization_endpoint: Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
                .expect("static URL"),
            oauth_address: "127.0.0.1:8089".parse().expect("static address"),
            redirect_uri: emusync_data_drive::REDIRECT_URI.to_string(),
        }
    }
}

type DesktopSyncOrchestrator = SyncOrchestrator<
    DesktopSaveAccess,
    DriveListing,
    DriveFolders,
    RetainedFiles<GuardedFiles<DriveTransfer, ConfigManager>>,
    InteractiveOAuth<ConfigManager, DesktopBrowserLauncher>,
    GameRunner<DesktopSteamLibrary>,
>;

type DesktopRetainedFiles = RetainedFiles<GuardedFiles<DriveTransfer, ConfigManager>>;
type BuiltOrchestrator = (Arc<DesktopSyncOrchestrator>, DesktopRetainedFiles);
type ConflictResolver = Arc<dyn Fn(Decision) + Send + Sync>;
type RetryRunner =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static>;

struct DesktopSaveSetup;

impl SaveSetup for DesktopSaveSetup {
    type Error = String;

    async fn request(&self, request: SaveSetupRequest) -> Result<SaveSetupResult, Self::Error> {
        let targets = request.game.save_targets.clone();
        Ok(SaveSetupResult {
            game: request.game,
            targets,
        })
    }
}

/// Desktop application coordinator managing async operations and state events.
#[derive(Clone)]
pub struct DesktopCoordinator {
    config_manager: ConfigManager,
    steam: DesktopSteamLibrary,
    browser: Arc<DesktopBrowserLauncher>,
    picker: FilePicker,
    event_tx: Sender<AppEvent>,
    redraw: Arc<dyn Fn() + Send + Sync>,
    tokio_handle: Handle,
    endpoints: DriveEndpoints,
    current_conflict: Arc<Mutex<Option<SaveConflict>>>,
    active_conflict_resolver: Arc<Mutex<Option<ConflictResolver>>>,
    active_retry_failure: Arc<Mutex<Option<emusync_domain::sync::PostSyncFailure>>>,
    active_retry_runner: Arc<Mutex<Option<RetryRunner>>>,
    active_cancel: Arc<Mutex<Option<CancellationToken>>>,
    operations: Arc<OperationCoordinator>,
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

        Self {
            config_manager,
            steam,
            browser,
            picker,
            event_tx,
            redraw,
            tokio_handle,
            endpoints: DriveEndpoints::default(),
            current_conflict: Arc::new(Mutex::new(None)),
            active_conflict_resolver: Arc::new(Mutex::new(None)),
            active_retry_failure: Arc::new(Mutex::new(None)),
            active_retry_runner: Arc::new(Mutex::new(None)),
            active_cancel: Arc::new(Mutex::new(None)),
            operations: Arc::new(OperationCoordinator::new()),
        }
    }

    pub fn with_endpoints(mut self, endpoints: DriveEndpoints) -> Self {
        self.endpoints = endpoints;
        self
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
                    self.check_sync_entry(entry.clone());
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
            UiAction::CheckForUpdates { .. }
            | UiAction::DownloadUpdate(_)
            | UiAction::RestartApp(_) => {
                self.send_event(AppEvent::Toast(
                    "Updates are not yet available in this development build.".into(),
                ));
            }
            UiAction::ResolveConflict(decision) => {
                if let Some(resolver) = self.active_conflict_resolver.lock().unwrap().take() {
                    resolver(decision);
                }
                self.send_event(AppEvent::StatusChanged(AppStatus::Idle));
            }
            UiAction::RetryPostSync => {
                if let Some(runner) = self.active_retry_runner.lock().unwrap().take() {
                    self.tokio_handle.spawn(async move {
                        runner().await;
                    });
                } else {
                    self.send_event(AppEvent::StatusChanged(AppStatus::Idle));
                }
            }
            UiAction::ClearStatus => {
                if let Some(cancel) = self.active_cancel.lock().unwrap().take() {
                    cancel.cancel();
                }
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

    fn build_orchestrator(&self, config: &AppConfig) -> Result<BuiltOrchestrator, String> {
        let clients = DriveHttpPolicy::default()
            .build()
            .map_err(|e| format!("HTTP client configuration failed: {e}"))?;

        let listing = DriveListing::new(clients.metadata.clone(), self.endpoints.files_v3.clone());
        let folders = DriveFolders::new(clients.metadata.clone(), self.endpoints.files_v3.clone());

        let event_tx = self.event_tx.clone();
        let redraw = self.redraw.clone();
        let cancel = CancellationToken::new();
        *self.active_cancel.lock().unwrap() = Some(cancel.clone());

        let transfer_progress = {
            let tx = event_tx.clone();
            let redraw = redraw.clone();
            Arc::new(move |progress: TransferProgress| {
                let percent = progress
                    .total
                    .filter(|&t| t > 0)
                    .map(|total| (progress.transferred * 100) / total);
                let message = if let Some(pct) = percent {
                    format!("Syncing {} ({}%)", progress.name, pct)
                } else {
                    format!("Syncing {}", progress.name)
                };
                let _ = tx.send(AppEvent::StatusChanged(AppStatus::Syncing { message }));
                (redraw)();
            })
        };

        let drive_transfer = DriveTransfer::with_clients(
            clients.clone(),
            self.endpoints.files_v3.clone(),
            self.endpoints.upload_v3.clone(),
            self.endpoints.files_v2.clone(),
            self.endpoints.upload_v2.clone(),
            cancel,
            transfer_progress,
        );

        let history = Arc::new(
            SyncHistoryStore::new(self.config_manager.path())
                .map_err(|e| format!("Failed to open sync history: {e}"))?,
        );
        let recovery = Arc::new(
            RecoveryStore::new(self.config_manager.path())
                .map_err(|e| format!("Failed to open recovery store: {e}"))?,
        );
        let tracked =
            Arc::new(TrackedFileSync::new(drive_transfer, history).with_recovery(recovery));

        let snapshot = self
            .config_manager
            .snapshot(config)
            .map_err(|e| format!("Failed to snapshot credentials: {e}"))?;
        let credentials = RecoveryCredentials::new(
            Arc::new(self.config_manager.clone()),
            snapshot,
            config.clone(),
        );

        let guarded = GuardedFiles::new(
            tracked,
            credentials,
            |e: &emusync_data_drive::TransferError| e.leaves_remote_unchanged(),
        );
        let retained = RetainedFiles::new(guarded);

        let oauth = InteractiveOAuth::with_endpoints(
            clients.metadata,
            self.config_manager.clone(),
            self.browser.clone(),
            self.endpoints.token_endpoint.clone(),
            self.endpoints.authorization_endpoint.clone(),
            self.endpoints.oauth_address,
            self.endpoints.redirect_uri.clone(),
        );

        let runner = GameRunner::new(Arc::new(self.steam.clone()));

        let services = SyncServices {
            files: DesktopSaveAccess,
            search: listing,
            folders,
            tracked_sync: retained.clone(),
            authorization: oauth,
            runner,
        };

        let current_conflict = self.current_conflict.clone();
        let active_retry_failure = self.active_retry_failure.clone();
        let on_status = move |event: SyncEvent| {
            match &event {
                SyncEvent::Idle => {
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Idle));
                }
                SyncEvent::Syncing(msg) => {
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Syncing {
                        message: msg.clone(),
                    }));
                }
                SyncEvent::Playing => {
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Playing));
                }
                SyncEvent::Error(err) => {
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Error {
                        message: err.clone(),
                    }));
                }
                SyncEvent::Conflict(conflict) => {
                    *current_conflict.lock().unwrap() = Some(conflict.clone());
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Conflict {
                        local_date: format_epoch_ms(conflict.local_modified_ms),
                        cloud_date: format_epoch_ms(conflict.cloud_modified_ms),
                        game_name: conflict.game_name.clone(),
                    }));
                }
                SyncEvent::PostSyncFailed(failure) => {
                    *active_retry_failure.lock().unwrap() = Some(failure.clone());
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::PostSyncFailed {
                        game_name: failure.game_name.clone(),
                        reason: failure.reason.clone(),
                        completed_save_count: failure.completed_save_count,
                        pending_save_count: failure.pending_save_count,
                    }));
                }
            }
            (redraw)();
        };

        let orchestrator =
            Arc::new(SyncOrchestratorFactory::new(services).create(config.clone(), on_status));

        let orch_resolve = orchestrator.clone();
        let current_conflict_cb = self.current_conflict.clone();
        *self.active_conflict_resolver.lock().unwrap() = Some(Arc::new(move |decision| {
            if let Some(c) = current_conflict_cb.lock().unwrap().take() {
                orch_resolve.resolve_conflict(&c, decision);
            }
        }));

        let orch_retry = orchestrator.clone();
        let active_retry_cb = self.active_retry_failure.clone();
        *self.active_retry_runner.lock().unwrap() = Some(Arc::new(move || {
            let orch = orch_retry.clone();
            let failure = active_retry_cb.lock().unwrap().take();
            Box::pin(async move {
                if let Some(f) = failure {
                    let _ = orch.retry_post_sync(&f).await;
                }
            })
        }));

        Ok((orchestrator, retained))
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
        let config = match self.config_manager.load() {
            Ok(c) => c,
            Err(e) => {
                self.send_event(AppEvent::Toast(format!("Failed to load config: {e}")));
                return;
            }
        };

        let rename_needed = drive_connected(&config) && old_name != name;

        if !rename_needed {
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
            return;
        }

        self.send_event(AppEvent::StatusChanged(AppStatus::Syncing {
            message: format!(
                "Migrating cloud folder from '{}' to '{}'...",
                old_name, name
            ),
        }));

        let (orchestrator, _) = match self.build_orchestrator(&config) {
            Ok(o) => o,
            Err(e) => {
                self.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: format!("Cloud migration failed: {e}"),
                }));
                return;
            }
        };

        let coordinator = self.clone();
        self.tokio_handle.spawn(async move {
            match orchestrator.rename_entry(&old, &new).await {
                Ok(_) => {
                    match coordinator.config_manager.update(|mut cfg| {
                        if let Some(idx) =
                            cfg.entries.iter().position(|e| e.common().name == old_name)
                        {
                            cfg.entries[idx] = new.clone();
                        }
                        cfg
                    }) {
                        Ok(updated) => {
                            let scanned = scan_entry_games(&new);
                            coordinator.send_event(AppEvent::ConfigReloaded(updated));
                            coordinator.send_event(AppEvent::EntryGamesScanned {
                                entry_name: name,
                                games: scanned,
                            });
                            coordinator.send_event(AppEvent::StatusChanged(AppStatus::Idle));
                            coordinator.send_event(AppEvent::Toast(
                                "Category updated and cloud folders renamed.".into(),
                            ));
                        }
                        Err(err) => {
                            coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                                message: format!("Failed to update configuration: {err}"),
                            }));
                        }
                    }
                }
                Err(err) => {
                    coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                        message: format!(
                            "Cloud migration failed: {err}. Category was not modified."
                        ),
                    }));
                }
            }
        });
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
        let old_effective_title = item.effective_title();
        let new_effective_title = title.clone().unwrap_or_else(|| item.name.clone());

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
            targets: targets.clone(),
        };

        let updated_entry = build_game_entry(&sync_game, &changes);

        let config = match self.config_manager.load() {
            Ok(c) => c,
            Err(e) => {
                self.send_event(AppEvent::Toast(format!("Failed to load config: {e}")));
                return;
            }
        };

        let rename_needed = drive_connected(&config) && old_effective_title != new_effective_title;

        if !rename_needed {
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
                    let new_name = updated_entry.common().name.clone();
                    if let Some(entry) =
                        updated.entries.iter().find(|e| e.common().name == new_name)
                    {
                        let scanned = scan_entry_games(entry);
                        self.send_event(AppEvent::EntryGamesScanned {
                            entry_name: new_name,
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
            return;
        }

        self.send_event(AppEvent::StatusChanged(AppStatus::Syncing {
            message: format!(
                "Migrating cloud folder from '{}' to '{}'...",
                old_effective_title, new_effective_title
            ),
        }));

        let (orchestrator, _) = match self.build_orchestrator(&config) {
            Ok(o) => o,
            Err(e) => {
                self.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: format!("Cloud migration failed: {e}"),
                }));
                return;
            }
        };

        let coordinator = self.clone();
        let rom_path = item
            .rom_file
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        self.tokio_handle.spawn(async move {
            match orchestrator
                .rename_game(
                    &item.entry,
                    rom_path.as_deref(),
                    &old_effective_title,
                    &new_effective_title,
                    &targets,
                )
                .await
            {
                Ok(_) => {
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
                            if let Some(entry) = updated
                                .entries
                                .iter()
                                .find(|e| e.common().name == entry_name)
                            {
                                let scanned = scan_entry_games(entry);
                                coordinator.send_event(AppEvent::EntryGamesScanned {
                                    entry_name,
                                    games: scanned,
                                });
                            }
                            coordinator.send_event(AppEvent::ConfigReloaded(updated));
                            coordinator.send_event(AppEvent::StatusChanged(AppStatus::Idle));
                            coordinator.send_event(AppEvent::Toast(
                                "Game overrides saved and cloud folder renamed.".into(),
                            ));
                        }
                        Err(err) => {
                            coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                                message: format!("Failed to update configuration: {err}"),
                            }));
                        }
                    }
                }
                Err(err) => {
                    coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                        message: format!(
                            "Cloud folder migration failed: {err}. Game overrides were not saved."
                        ),
                    }));
                }
            }
        });
    }

    fn complete_save_setup(
        &self,
        request: UiSaveSetupRequest,
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
        if self.operations.is_busy() {
            return;
        }

        let exec = emulator.common.executable_path.trim().to_string();
        if exec.is_empty() {
            self.send_event(AppEvent::Toast("No emulator executable configured.".into()));
            return;
        }

        let steam_launches = Arc::new(self.steam.clone());
        let runner = GameRunner::new(steam_launches);
        let coordinator = self.clone();

        self.tokio_handle.spawn(async move {
            let _ = coordinator
                .operations
                .run_if_idle(async move {
                    if let Err(e) = runner.open_emulator(&emulator).await {
                        eprintln!("Failed to launch emulator: {e:?}");
                    }
                })
                .await;
        });
    }

    fn launch_game(&self, item: GameItem) {
        if self.operations.is_busy() {
            return;
        }

        let config = match self.config_manager.load() {
            Ok(c) => c,
            Err(e) => {
                self.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: format!("Failed to load configuration: {e}"),
                }));
                return;
            }
        };

        if !drive_connected(&config) {
            let effective_title = item.effective_title();
            let steam_launches = Arc::new(self.steam.clone());
            let runner = GameRunner::new(steam_launches);
            let event_tx = self.event_tx.clone();
            let redraw = self.redraw.clone();

            self.send_event(AppEvent::StatusChanged(AppStatus::Syncing {
                message: format!("Preparing {}...", effective_title),
            }));

            let coordinator = self.clone();
            self.tokio_handle.spawn(async move {
                let _ = coordinator
                    .operations
                    .run_if_idle(async move {
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
                    })
                    .await;
            });
            return;
        }

        let targets = match &item.entry {
            GameEntry::Emulator(emulator) => {
                if let Some(rom) = item.rom_file.as_ref().and_then(|p| p.to_str()) {
                    DesktopSaveAccess
                        .effective_targets_for_rom(emulator, rom)
                        .unwrap_or_default()
                } else {
                    DesktopSaveAccess
                        .effective_targets(&item.entry)
                        .unwrap_or_default()
                }
            }
            GameEntry::Native(_) => DesktopSaveAccess
                .effective_targets(&item.entry)
                .unwrap_or_default(),
        };

        let sync_game = SyncGame {
            name: item.name.clone(),
            entry: item.entry.clone(),
            rom_path: item
                .rom_file
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            effective_title: item.effective_title(),
            save_targets: targets,
        };

        let (orchestrator, _) = match self.build_orchestrator(&config) {
            Ok(o) => o,
            Err(e) => {
                self.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: format!("Failed to initialize cloud synchronization: {e}"),
                }));
                return;
            }
        };

        let coordinator = self.clone();
        let entry = item.entry.clone();
        self.tokio_handle.spawn(async move {
            let res = coordinator
                .operations
                .run_if_idle(async {
                    orchestrator
                        .play_with_sync(sync_game, &DesktopSaveSetup)
                        .await
                })
                .await;

            match res {
                Some(Ok(())) => {
                    coordinator.check_sync_entry(entry);
                }
                Some(Err(err)) => {
                    coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                        message: format!("Synchronization error: {err}"),
                    }));
                }
                None => {}
            }
        });
    }

    fn check_sync_entry(&self, entry: GameEntry) {
        if self.operations.is_busy() {
            return;
        }
        let name = entry.common().name.clone();
        let config = match self.config_manager.load() {
            Ok(c) => c,
            Err(_) => return,
        };

        if !drive_connected(&config) {
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

        let (orchestrator, _) = match self.build_orchestrator(&config) {
            Ok(o) => o,
            Err(_) => {
                self.send_event(AppEvent::SyncStatusUpdated {
                    entry_name: name,
                    status: CloudSyncStatus::Error,
                });
                return;
            }
        };

        let coordinator = self.clone();
        self.tokio_handle.spawn(async move {
            match orchestrator.check_entry(&entry).await {
                Ok(EntrySyncStatus::InSync) => {
                    coordinator.send_event(AppEvent::SyncStatusUpdated {
                        entry_name: name,
                        status: CloudSyncStatus::InSync,
                    });
                }
                Ok(EntrySyncStatus::OutOfSync) => {
                    coordinator.send_event(AppEvent::SyncStatusUpdated {
                        entry_name: name,
                        status: CloudSyncStatus::OutOfSync,
                    });
                }
                Ok(EntrySyncStatus::Conflict) => {
                    coordinator.send_event(AppEvent::SyncStatusUpdated {
                        entry_name: name,
                        status: CloudSyncStatus::Conflict,
                    });
                }
                Ok(EntrySyncStatus::NotConfigured) => {
                    coordinator.send_event(AppEvent::SyncStatusUpdated {
                        entry_name: name,
                        status: CloudSyncStatus::NotConfigured,
                    });
                }
                Ok(EntrySyncStatus::Idle) => {
                    coordinator.send_event(AppEvent::SyncStatusUpdated {
                        entry_name: name,
                        status: CloudSyncStatus::Idle,
                    });
                }
                Ok(EntrySyncStatus::Error) | Err(_) => {
                    coordinator.send_event(AppEvent::SyncStatusUpdated {
                        entry_name: name,
                        status: CloudSyncStatus::Error,
                    });
                }
            }
        });
    }

    fn sync_entry(&self, entry: GameEntry) {
        if self.operations.is_busy() {
            return;
        }

        let name = entry.common().name.clone();
        let config = match self.config_manager.load() {
            Ok(c) => c,
            Err(e) => {
                self.send_event(AppEvent::Toast(format!("Failed to load config: {e}")));
                return;
            }
        };

        if !drive_connected(&config) {
            self.send_event(AppEvent::Toast("Google Drive is not configured.".into()));
            return;
        }

        self.send_event(AppEvent::SyncStatusUpdated {
            entry_name: name.clone(),
            status: CloudSyncStatus::Syncing,
        });
        self.send_event(AppEvent::StatusChanged(AppStatus::Syncing {
            message: format!("Synchronizing '{}'...", name),
        }));

        let (orchestrator, _) = match self.build_orchestrator(&config) {
            Ok(o) => o,
            Err(e) => {
                self.send_event(AppEvent::SyncStatusUpdated {
                    entry_name: name.clone(),
                    status: CloudSyncStatus::Error,
                });
                self.send_event(AppEvent::StatusChanged(AppStatus::Error {
                    message: format!("Failed to initialize cloud synchronization: {e}"),
                }));
                return;
            }
        };

        let coordinator = self.clone();
        let entry_clone = entry.clone();
        self.tokio_handle.spawn(async move {
            let res = coordinator
                .operations
                .run_if_idle(orchestrator.sync_entry(&entry_clone))
                .await;

            match res {
                Some(Ok(true)) => {
                    coordinator.send_event(AppEvent::SyncStatusUpdated {
                        entry_name: name.clone(),
                        status: CloudSyncStatus::InSync,
                    });
                    coordinator.send_event(AppEvent::StatusChanged(AppStatus::Idle));
                    coordinator.send_event(AppEvent::Toast(format!("'{}' synchronized.", name)));
                }
                Some(Ok(false)) => {
                    coordinator.check_sync_entry(entry_clone);
                    coordinator.send_event(AppEvent::StatusChanged(AppStatus::Idle));
                }
                Some(Err(err)) => {
                    coordinator.send_event(AppEvent::SyncStatusUpdated {
                        entry_name: name.clone(),
                        status: CloudSyncStatus::Error,
                    });
                    coordinator.send_event(AppEvent::StatusChanged(AppStatus::Error {
                        message: format!("Sync failed: {err}"),
                    }));
                }
                None => {}
            }
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

        let config = match self.config_manager.update_credentials(|_| {
            Some(Credentials {
                client_id: id,
                client_secret: secret,
                refresh_token: None,
            })
        }) {
            Ok(cfg) => cfg,
            Err(e) => {
                self.send_event(AppEvent::Toast(format!("Failed to save credentials: {e}")));
                return;
            }
        };

        self.send_event(AppEvent::ConfigReloaded(config.clone()));
        self.send_event(AppEvent::Toast(
            "Opening browser for Google Drive authorization...".into(),
        ));

        let clients = match DriveHttpPolicy::default().build() {
            Ok(c) => c,
            Err(e) => {
                self.send_event(AppEvent::Toast(format!("HTTP client error: {e}")));
                return;
            }
        };

        let oauth = InteractiveOAuth::with_endpoints(
            clients.metadata,
            self.config_manager.clone(),
            self.browser.clone(),
            self.endpoints.token_endpoint.clone(),
            self.endpoints.authorization_endpoint.clone(),
            self.endpoints.oauth_address,
            self.endpoints.redirect_uri.clone(),
        );

        let coordinator = self.clone();
        self.tokio_handle.spawn(async move {
            match oauth.authorize_scoped(&config, true).await {
                Ok(_) => {
                    if let Ok(updated) = coordinator.config_manager.load() {
                        coordinator.send_event(AppEvent::ConfigReloaded(updated));
                    }
                    coordinator.send_event(AppEvent::Toast(
                        "Google Drive connected successfully.".into(),
                    ));
                }
                Err(err) => {
                    coordinator.send_event(AppEvent::Toast(format!(
                        "Google Drive authorization failed: {err}"
                    )));
                }
            }
        });
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

    fn pick_path(&self, target: PathTarget, mode: PickMode) {
        let picker = self.picker;
        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();

        let kind = match mode {
            PickMode::File => PickerType::File,
            PickMode::Directory => PickerType::Directory,
        };

        self.tokio_handle.spawn_blocking(move || {
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
