//! Coordinates domain, application services, platform adapters, and Tokio background tasks.

pub mod composition;
pub mod interaction;
pub mod library;
pub mod platform;
pub mod sync;
pub mod updates;

pub use composition::DriveEndpoints;
pub use interaction::{ConflictResolver, InteractionControls, PendingSaveSetup, RetryRunner};
pub use platform::PlatformServices;
pub use sync::resolve_effective_targets;
pub use updates::{UpdateServices, UpdateSourceConfig};

use crate::{
    AuthorizedTransferFailure, CloseState, OperationFailure, OperationSupervisor,
    PersistedRecoveryStatus, RecoveryError, RecoveryStatus,
    drive_recovery::{
        reconcile_conditional_upload, reconcile_created_upload, reconcile_persisted_uploads,
        reconcile_tracked_upload_with_listing,
    },
    events::{AppEvent, EventSender},
    target_types::{resolve_entry_target_kinds, resolve_target_kinds},
};
use composition::{BuiltOrchestrator, drive_connected, format_epoch_ms};
use emusync_application::{UpdateState, library::LoadLibrary};
use emusync_data_drive::{
    DriveHttpClients, DriveHttpPolicy, InteractiveOAuth, ResumableUpload, TransferProgress,
};
use emusync_data_local::{
    ConfigManager, LocalSaveTargets, RecoveryCredentials, RecoveryStore, TrackedError,
};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, CredentialStore, Credentials, Language},
    execution::PathPicker,
    library::{GameEntry, SaveTarget, SteamLibrary},
    sync::{
        Decision, PostSyncFailure, SaveConflict, SavePlacementRequest, SaveSetupResult,
        SaveSynchronizer, SyncEvent, SyncGame,
    },
};
use emusync_platform_desktop::{
    DesktopBrowserLauncher, DesktopSteamLibrary, FilePicker, SteamShortcutManager,
};
use emusync_presentation::{
    actions::UiAction,
    i18n::sync_text,
    model::{AppStatus, SaveSetupRequest as UiSaveSetupRequest},
};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::Sender,
};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

const UPDATES_PORTABLE: &str = "This portable copy does not update itself. Download the latest portable ZIP from the project's releases and replace the folder.";
const UPDATES_DISABLED: &str = "Updates are disabled in development builds. Set EMUSYNC_UPDATE_BASE_URL to a test release server.";

/// Desktop application coordinator managing async operations and state events.
#[derive(Clone)]
pub struct DesktopCoordinator {
    pub config_manager: ConfigManager,
    pub event_tx: EventSender,
    pub redraw: Arc<dyn Fn() + Send + Sync>,
    pub tokio_handle: Handle,
    pub endpoints: DriveEndpoints,
    pub platform: PlatformServices,
    pub interaction: InteractionControls,
    pub supervisor:
        Arc<tokio::sync::Mutex<OperationSupervisor<AuthorizedTransferFailure<ConfigManager>>>>,
    pub closing: Arc<AtomicBool>,
    pub active_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    pub pending_recovery: Arc<Mutex<Option<String>>>,
    pub is_admitted: Arc<AtomicBool>,
    pub updates: UpdateServices,
    /// Drive HTTP clients, built on first use and shared by every operation so their
    /// connections are reused instead of opened again for each check or sync.
    drive_http: Arc<OnceLock<DriveHttpClients>>,
    /// The interface language, for the messages this coordinator sends.
    pub language: crate::CurrentLanguage,
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
        let picker = Arc::new(FilePicker);
        let scan_revision = Arc::new(AtomicU64::new(0));
        let interaction = InteractionControls::new(event_tx.clone(), redraw.clone());
        let event_tx = EventSender::new(event_tx, redraw.clone());

        let platform = PlatformServices {
            config_manager: config_manager.clone(),
            steam,
            browser,
            picker,
            event_tx: event_tx.clone(),
            redraw: redraw.clone(),
            tokio_handle: tokio_handle.clone(),
            scan_revision,
            steam_revision: Arc::new(AtomicU64::new(0)),
        };

        let updates = UpdateServices::new(
            UpdateSourceConfig::for_this_build(),
            event_tx.clone(),
            tokio_handle.clone(),
        );

        Self {
            config_manager,
            event_tx,
            redraw,
            tokio_handle,
            endpoints: DriveEndpoints::default(),
            platform,
            interaction,
            supervisor: Arc::new(tokio::sync::Mutex::new(OperationSupervisor::new())),
            closing: Arc::new(AtomicBool::new(false)),
            active_tasks: Arc::new(Mutex::new(Vec::new())),
            pending_recovery: Arc::new(Mutex::new(None)),
            is_admitted: Arc::new(AtomicBool::new(false)),
            updates,
            drive_http: Arc::new(OnceLock::new()),
            language: crate::CurrentLanguage::default(),
        }
    }

    /// Replaces where releases are checked and downloaded from.
    pub fn with_update_source(mut self, source: UpdateSourceConfig) -> Self {
        self.updates =
            UpdateServices::new(source, self.event_tx.clone(), self.tokio_handle.clone());
        self
    }

    /// Remembers the interface language, which the UI already shows. Only the language
    /// changes: the rest of the configuration is kept as stored, and nothing is reloaded.
    fn save_language(&self, language: Language) {
        self.language.set(language);
        let config = self.config_manager.clone();
        let coordinator = self.clone();
        let handle = self.tokio_handle.spawn(async move {
            let saved = tokio::task::spawn_blocking(move || {
                config.update(|mut current| {
                    current.language = language;
                    current
                })
            })
            .await;
            if !matches!(saved, Ok(Ok(_))) {
                coordinator.send_toast("Could not save the language.");
            }
        });
        self.track_task(handle);
    }

    /// The shared Drive HTTP clients, built on first use. A failed build is not kept, so
    /// the next operation tries again.
    fn drive_clients(&self) -> Result<DriveHttpClients, reqwest::Error> {
        if let Some(clients) = self.drive_http.get() {
            return Ok(clients.clone());
        }
        let clients = DriveHttpPolicy::default().build()?;
        Ok(self.drive_http.get_or_init(|| clients).clone())
    }

    pub fn with_endpoints(mut self, endpoints: DriveEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    /// Replaces the native file chooser, so tests can script its answers.
    pub fn with_path_picker(
        mut self,
        picker: Arc<dyn PathPicker<Error = std::io::Error> + Send + Sync>,
    ) -> Self {
        self.platform.picker = picker;
        self
    }

    pub fn with_supervisor(
        mut self,
        supervisor: Arc<
            tokio::sync::Mutex<OperationSupervisor<AuthorizedTransferFailure<ConfigManager>>>,
        >,
    ) -> Self {
        self.supervisor = supervisor;
        self
    }

    pub fn supervisor(
        &self,
    ) -> Arc<tokio::sync::Mutex<OperationSupervisor<AuthorizedTransferFailure<ConfigManager>>>>
    {
        self.supervisor.clone()
    }

    pub fn send_event(&self, event: AppEvent) {
        let _ = self.event_tx.send(event);
        (self.redraw)();
    }

    pub fn send_toast(&self, message: impl AsRef<str>) {
        self.send_event(AppEvent::Toast(message.as_ref().to_string()));
    }

    pub fn is_closing(&self) -> bool {
        self.closing.load(Ordering::Acquire)
    }

    pub fn try_acquire_admission(&self) -> bool {
        self.try_acquire_admission_internal(false)
    }

    pub fn try_acquire_admission_for_connect(&self) -> bool {
        self.try_acquire_admission_internal(true)
    }

    fn try_acquire_admission_internal(&self, allow_pending_recovery: bool) -> bool {
        if self.is_closing() {
            return false;
        }
        if !allow_pending_recovery && self.pending_recovery.lock().unwrap().is_some() {
            return false;
        }
        if let Ok(guard) = self.supervisor.try_lock() {
            if guard.is_running() || (!allow_pending_recovery && guard.failure().is_some()) {
                return false;
            }
        } else {
            return false;
        }

        self.is_admitted
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn release_admission(&self) {
        self.is_admitted.store(false, Ordering::SeqCst);
    }

    pub fn track_task(&self, handle: tokio::task::JoinHandle<()>) {
        let mut tasks = self.active_tasks.lock().unwrap();
        tasks.retain(|t| !t.is_finished());
        tasks.push(handle);
    }

    pub fn record_pending_recovery(&self, reason: String) {
        *self.pending_recovery.lock().unwrap() = Some(reason);
    }

    pub fn clear_pending_recovery(&self) {
        *self.pending_recovery.lock().unwrap() = None;
    }

    pub fn record_transfer_error(&self, error: TrackedError<emusync_data_drive::TransferError>) {
        let config = self.config_manager.load().unwrap_or_default();
        let snapshot = match self.config_manager.snapshot(&config) {
            Ok(s) => s,
            Err(_) => return,
        };
        let credentials =
            RecoveryCredentials::new(Arc::new(self.config_manager.clone()), snapshot, config);
        let failure = AuthorizedTransferFailure::new(error, credentials);

        if let Ok(mut guard) = self.supervisor.try_lock() {
            guard.retain_failure(OperationFailure::Operation(failure));
        }
    }

    /// Checks for a newer release; a manual check also reports when there is none.
    pub fn check_for_updates(&self, manual: bool) {
        if !self.updates.is_enabled() {
            if manual {
                self.send_toast(self.updates.disabled_message());
            }
            return;
        }
        let coordinator = self.clone();
        self.updates.check(move |state| {
            if !manual {
                return;
            }
            match state {
                UpdateState::Idle => coordinator.send_toast("EmuSync is up to date."),
                UpdateState::Error(message) => coordinator.send_toast(message),
                _ => {}
            }
        });
    }

    /// Installs the downloaded update and closes; never while a sync is running.
    pub fn restart_for_update(&self, path: std::path::PathBuf) {
        if !self.try_acquire_admission() {
            self.send_toast("Finish the current operation before restarting to update.");
            return;
        }
        let coordinator = self.clone();
        let handle = self.tokio_handle.spawn(async move {
            // When it is not applied the update state already explains why.
            coordinator
                .updates
                .restart(path.to_string_lossy().into_owned())
                .await;
            coordinator.release_admission();
        });
        self.track_task(handle);
    }

    pub fn begin_close(&self) {
        self.closing.store(true, Ordering::SeqCst);
        self.updates.abort();
        self.interaction.cancel_active();
        if let Ok(guard) = self.supervisor.try_lock() {
            guard.cancel();
        }
    }

    pub fn poll_close(&self) -> CloseState {
        if self.is_admitted.load(Ordering::SeqCst) {
            return CloseState::Pending;
        }

        // Clean up finished tasks
        {
            let mut tasks = self.active_tasks.lock().unwrap();
            tasks.retain(|t| !t.is_finished());
            if !tasks.is_empty() {
                return CloseState::Pending;
            }
        }

        let Ok(guard) = self.supervisor.try_lock() else {
            return CloseState::Pending;
        };

        guard.cancel();
        if guard.is_running() {
            return CloseState::Pending;
        }

        // Check if there is any pending recovery recorded
        if self.pending_recovery.lock().unwrap().is_some() {
            self.closing.store(false, Ordering::SeqCst);
            return CloseState::RecoveryRequired;
        }

        if guard.failure().is_some() {
            self.closing.store(false, Ordering::SeqCst);
            return CloseState::RecoveryRequired;
        }

        // Ready when no tasks, no failure, no pending recovery
        CloseState::Ready
    }

    /// Loads initial config, scans ROMs, verifies Steam registrations, and checks RecoveryStore.
    pub fn initialize(&self) {
        // Startup check, silent unless a newer release exists.
        self.check_for_updates(false);
        let loader = LoadLibrary::new(self.config_manager.clone(), self.platform.steam.clone());
        match loader.load() {
            Ok(config) => {
                self.language.set(config.language);
                let first = config.entries.first().cloned();
                self.send_event(AppEvent::ConfigReloaded(config.clone()));

                if let Some(entry) = &first {
                    let _ = self.platform.scan_entry_games_async(entry.clone());
                    self.check_sync_entry_background(entry.clone());
                }

                for entry in &config.entries {
                    let reg = self
                        .platform
                        .steam
                        .is_registered(entry)
                        .unwrap_or(entry.common().steam_app_id.is_some());
                    self.send_event(AppEvent::SteamRegistrationUpdated {
                        entry_name: entry.common().name.clone(),
                        registered: reg,
                    });
                }

                // Check RecoveryStore on startup
                if let Ok(store) = RecoveryStore::new(self.config_manager.path())
                    && let Ok(entries) = store.list()
                    && !entries.is_empty()
                {
                    self.record_pending_recovery(format!(
                        "Pending recovery of {} unconfirmed upload(s).",
                        entries.len()
                    ));
                    self.send_toast(format!(
                        "Found {} pending sync recovery record(s).",
                        entries.len()
                    ));
                    if drive_connected(&config) {
                        let coord = self.clone();
                        let handle = self.tokio_handle.spawn(async move {
                            if let Ok(statuses) = coord.reconcile_persisted().await {
                                let all_recovered = !statuses.is_empty()
                                    && statuses.iter().all(|(_, s)| {
                                        matches!(s, PersistedRecoveryStatus::Recovered)
                                    });
                                if all_recovered {
                                    coord.clear_pending_recovery();
                                }
                            }
                        });
                        self.track_task(handle);
                    }
                }
            }
            Err(err) => {
                self.send_toast(format!("Failed to load config: {err}"));
            }
        }
    }

    pub fn handle_action(&self, action: UiAction) {
        match action {
            UiAction::SelectEntry(entry) => {
                if let Some(entry) = entry {
                    let _ = self.platform.scan_entry_games_async(entry.clone());
                    self.check_sync_entry_background(entry);
                }
            }
            UiAction::LaunchGame(item) => {
                if !self.try_acquire_admission() {
                    self.send_toast("Cannot launch game: another operation is in progress or requires recovery.");
                    return;
                }
                let coord = self.clone();
                let handle = self.tokio_handle.spawn(async move {
                    sync::SyncOperations::execute_launch_game(coord, item).await;
                });
                self.track_task(handle);
            }
            UiAction::OpenEmulator(emulator) => {
                if !self.try_acquire_admission() {
                    self.send_toast("Cannot launch emulator: another operation is in progress.");
                    return;
                }
                let coord = self.clone();
                let handle = self.tokio_handle.spawn(async move {
                    sync::SyncOperations::execute_open_emulator(coord, emulator).await;
                });
                self.track_task(handle);
            }
            UiAction::SyncEntry(entry) => {
                if !self.try_acquire_admission() {
                    self.send_toast("Cannot synchronize: another operation is in progress or requires recovery.");
                    return;
                }
                let coord = self.clone();
                let handle = self.tokio_handle.spawn(async move {
                    sync::SyncOperations::execute_sync_entry(coord, entry).await;
                });
                self.track_task(handle);
            }
            UiAction::CheckSyncEntry(entry) => {
                self.check_sync_entry_background(entry);
            }
            UiAction::AddEntry(entry) => {
                if !self.try_acquire_admission() {
                    self.send_toast("Cannot add category: another operation is in progress or requires recovery.");
                    return;
                }
                let library = library::LibraryManager {
                    config_manager: self.config_manager.clone(),
                    platform: self.platform.clone(),
                };
                let entry = resolve_entry_target_kinds(entry, &LocalSaveTargets);
                library.add_entry(self, entry);
                self.release_admission();
            }
            UiAction::EditEntry { old, new } => {
                if !self.try_acquire_admission() {
                    self.send_toast("Cannot edit category: another operation is in progress or requires recovery.");
                    return;
                }
                let new = resolve_entry_target_kinds(*new, &LocalSaveTargets);
                library::LibraryManager::edit_entry(self, *old, new);
            }
            UiAction::DeleteEntry(entry) => {
                if !self.try_acquire_admission() {
                    self.send_toast("Cannot delete category: another operation is in progress.");
                    return;
                }
                let library = library::LibraryManager {
                    config_manager: self.config_manager.clone(),
                    platform: self.platform.clone(),
                };
                library.delete_entry(self, entry);
            }
            UiAction::SetLanguage(language) => self.save_language(language),
            UiAction::ReorderEntries(entries) => {
                if !self.try_acquire_admission() {
                    self.send_toast("Cannot reorder categories: another operation is in progress.");
                    return;
                }
                let library = library::LibraryManager {
                    config_manager: self.config_manager.clone(),
                    platform: self.platform.clone(),
                };
                library.reorder_entries(self, entries);
            }
            UiAction::EditGameOverride {
                item,
                title,
                targets,
                cover_path,
            } => {
                if !self.try_acquire_admission() {
                    self.send_toast(
                        "Cannot edit game: another operation is in progress or requires recovery.",
                    );
                    return;
                }
                let targets = resolve_target_kinds(targets, &LocalSaveTargets);
                library::LibraryManager::edit_game_override(self, item, title, targets, cover_path);
            }
            UiAction::CancelSaveSetup { request_id } => {
                self.interaction.cancel_save_setup_for(request_id)
            }
            UiAction::CompleteSaveSetup {
                request,
                targets,
                title,
                cover_path,
                restore,
            } => {
                // Claim the request first: without a live one nothing is written.
                let Some(pending) = self.interaction.take_save_setup(request.id) else {
                    return;
                };
                let targets = match restore {
                    None => targets,
                    // Rebuilt here rather than trusting the form, as Kotlin's `result`.
                    Some(choice) => match platform::restore_targets(
                        &choice.destination,
                        choice.whole_folder,
                        &request.cloud_file_names,
                    ) {
                        Ok(targets) if !targets.is_empty() => targets,
                        Ok(_) | Err(_) => {
                            self.send_toast(
                                "No valid restore location for the cloud files in that folder.",
                            );
                            pending.cancel();
                            return;
                        }
                    },
                };
                let targets = resolve_target_kinds(targets, &LocalSaveTargets);
                self.complete_save_setup(pending, *request, targets, title, cover_path);
            }
            UiAction::EditGame(item) => {
                let handle = self.platform.load_game_edit(item);
                self.track_task(handle);
            }
            UiAction::PreviewRestorePaths {
                request_id,
                destination,
                names,
            } => {
                let handle = self
                    .platform
                    .preview_restore_paths(request_id, destination, names);
                self.track_task(handle);
            }
            UiAction::ToggleSteam(entry) => {
                if !self.try_acquire_admission() {
                    self.send_toast("Cannot toggle Steam: another operation is in progress or requires recovery.");
                    return;
                }
                self.platform.toggle_steam(entry);
                self.release_admission();
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
            UiAction::CheckForUpdates { manual } => self.check_for_updates(manual),
            UiAction::DownloadUpdate(info) => {
                if self.updates.is_enabled() {
                    self.updates.download(info);
                } else {
                    self.send_toast(self.updates.disabled_message());
                }
            }
            UiAction::RestartApp(path) => self.restart_for_update(path),
            UiAction::ResolveConflict(decision) => {
                self.interaction.resolve_conflict(decision);
            }
            UiAction::ResolveSaveFolders(choice) => {
                self.interaction.resolve_placement(choice);
            }
            UiAction::RetryPostSync => {
                if let Some((failure, runner)) = self.interaction.take_retry() {
                    if !self.try_acquire_admission() {
                        self.send_toast(
                            "Cannot retry sync: operation in progress or recovery required.",
                        );
                        self.interaction.set_retry(failure, runner);
                        return;
                    }
                    let coord = self.clone();
                    let handle = self.tokio_handle.spawn(async move {
                        runner(failure).await;
                        coord.release_admission();
                    });
                    self.track_task(handle);
                }
            }
            UiAction::ClearStatus => {
                self.interaction.cancel_active();
                // Clear pending status only if no active transfer or recovery is holding exclusion
                if !self.is_admitted.load(Ordering::Acquire)
                    && self.pending_recovery.lock().unwrap().is_none()
                {
                    self.send_event(AppEvent::StatusChanged(AppStatus::Idle));
                }
            }
            UiAction::OpenConfigDirectory => {
                self.platform.open_config_dir();
            }
            UiAction::PickPath {
                request_id,
                target,
                mode,
                initial_path,
            } => {
                let handle = self
                    .platform
                    .pick_path(request_id, target, mode, initial_path);
                self.track_task(handle);
            }
            UiAction::DetectSaveTargetType { index, path } => {
                let handle = self.platform.detect_save_target_type(index, path);
                self.track_task(handle);
            }
            UiAction::Exit => {
                self.begin_close();
            }
        }
    }

    pub fn check_sync_entry_background(&self, entry: GameEntry) {
        if self.is_closing() {
            return;
        }
        let coord = self.clone();
        let handle = self.tokio_handle.spawn(async move {
            sync::SyncOperations::execute_check_sync_entry(coord, entry).await;
        });
        self.track_task(handle);
    }

    /// Writes the setup and answers `pending`, which the caller already claimed with
    /// `take_save_setup`; every path answers it exactly once.
    pub fn complete_save_setup(
        &self,
        pending: PendingSaveSetup,
        request: UiSaveSetupRequest,
        targets: Vec<SaveTarget>,
        title: Option<String>,
        cover_path: Option<String>,
    ) {
        let item = request.item.clone();
        let entry_name = item.entry.common().name.clone();

        let config = match self.config_manager.load() {
            Ok(c) => c,
            Err(e) => {
                self.send_toast(format!("Failed to load config: {e}"));
                pending.cancel();
                return;
            }
        };

        let Some(current_entry) = config
            .entries
            .iter()
            .find(|e| e.common().name == entry_name)
        else {
            self.send_toast("Category no longer exists.");
            pending.cancel();
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

        let changes = emusync_application::library::GameOverrideChanges {
            title,
            cover_path,
            targets: targets.clone(),
        };

        let updated_entry = emusync_application::library::build_game_entry(&current_game, &changes);

        // Commit configuration
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
                self.send_event(AppEvent::ConfigReloaded(updated));
                let _ = self.platform.scan_entry_games_async(updated_entry.clone());
                let updated_game = SyncGame {
                    name: item.name.clone(),
                    entry: updated_entry,
                    rom_path: rom_path_str,
                    effective_title: current_game.effective_title,
                    save_targets: targets.clone(),
                };
                // Respond to the waiting SaveSetup future so it continues its single execution
                pending.complete(SaveSetupResult {
                    game: updated_game,
                    targets,
                });
                (self.redraw)();
            }
            Err(err) => {
                self.send_toast(format!("Failed to save game setup: {err}"));
                pending.cancel();
            }
        }
    }

    pub fn build_orchestrator(&self, config: &AppConfig) -> Result<BuiltOrchestrator, String> {
        let cancel = CancellationToken::new();
        self.interaction.set_cancel_token(cancel.clone());

        let event_tx = self.event_tx.clone();
        let redraw = self.redraw.clone();
        let transfer_progress: Arc<dyn Fn(TransferProgress) + Send + Sync> = {
            let tx = event_tx.clone();
            let red = redraw.clone();
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
                (red)();
            })
        };

        let interaction = self.interaction.clone();
        let retry_runner_slot: Arc<Mutex<Option<RetryRunner>>> = Arc::new(Mutex::new(None));
        let retry_runner_slot_status = retry_runner_slot.clone();
        let interaction_status = interaction.clone();

        let language = self.language.clone();
        let on_status = move |event: SyncEvent| {
            match &event {
                SyncEvent::Idle => {
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Idle));
                }
                SyncEvent::Syncing(msg) => {
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Syncing {
                        message: sync_text(language.texts(), msg),
                    }));
                }
                SyncEvent::Playing => {
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Playing));
                }
                SyncEvent::Error(err) => {
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Error {
                        message: sync_text(language.texts(), err),
                    }));
                }
                SyncEvent::Conflict(conflict) => {
                    interaction_status.on_conflict(conflict.clone());
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::Conflict {
                        local_date: format_epoch_ms(conflict.local_modified_ms),
                        cloud_date: format_epoch_ms(conflict.cloud_modified_ms),
                        game_name: conflict.game_name.clone(),
                    }));
                }
                SyncEvent::ChooseSaveFolders(request) => {
                    interaction_status.on_placement(request.clone());
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::ChooseSaveFolders {
                        game_name: request.game_name.clone(),
                        folders: request.folders.clone(),
                        groups: request.groups.clone(),
                    }));
                }
                SyncEvent::PostSyncFailed(failure) => {
                    if let Some(runner) = retry_runner_slot_status.lock().unwrap().as_ref() {
                        interaction_status.set_retry(failure.clone(), runner.clone());
                    }
                    let _ = event_tx.send(AppEvent::StatusChanged(AppStatus::PostSyncFailed {
                        game_name: failure.game_name.clone(),
                        reason: sync_text(language.texts(), &failure.reason),
                        completed_save_count: failure.completed_save_count,
                        pending_save_count: failure.pending_save_count,
                    }));
                }
            }
            (redraw)();
        };

        let built = composition::build_orchestrator(
            self.drive_clients()
                .map_err(|e| format!("HTTP client configuration failed: {e}"))?,
            &self.config_manager,
            &self.platform.steam,
            &self.platform.browser,
            &self.endpoints,
            config,
            cancel,
            transfer_progress,
            on_status,
        )?;

        let orch_resolve = built.orchestrator.clone();
        self.interaction.set_conflict_resolver(Arc::new(
            move |conflict: &SaveConflict, decision: Decision| {
                orch_resolve.resolve_conflict(conflict, decision);
            },
        ));

        let orch_place = built.orchestrator.clone();
        self.interaction.set_placement_resolver(Arc::new(
            move |request: &SavePlacementRequest, choice: Option<Vec<usize>>| {
                orch_place.resolve_save_placement(request, choice);
            },
        ));

        let orch_retry = built.orchestrator.clone();
        let retained_retry = built.retained.clone();
        let retry_interaction = self.interaction.clone();
        let coord_clone = self.clone();
        let retry_runner: RetryRunner = Arc::new(move |failure: PostSyncFailure| {
            let orch = orch_retry.clone();
            let retained = retained_retry.clone();
            let inter = retry_interaction.clone();
            let coord = coord_clone.clone();
            Box::pin(async move {
                let retry_res = orch.retry_post_sync(&failure).await;
                if retained.has_error()
                    && let Some(err) = retained.quarantine_and_take_error().await
                {
                    let detail = err.to_string();
                    eprintln!("EmuSync: retry failed during transfer: {detail}");
                    coord.record_transfer_error(err);
                    inter
                        .event_tx
                        .send(AppEvent::StatusChanged(AppStatus::Error {
                            message: format!(
                                "Sync failed during retry: {detail}. Recovery is required."
                            ),
                        }))
                        .ok();
                    return;
                }
                if let Err(e) = retry_res {
                    let message = retained.take_notice().unwrap_or(e);
                    inter
                        .event_tx
                        .send(AppEvent::StatusChanged(AppStatus::Error { message }))
                        .ok();
                }
            })
        });

        *retry_runner_slot.lock().unwrap() = Some(retry_runner);

        Ok(built)
    }

    pub fn build_check_orchestrator(
        &self,
        config: &AppConfig,
    ) -> Result<BuiltOrchestrator, String> {
        let cancel = CancellationToken::new();
        let transfer_progress: Arc<dyn Fn(TransferProgress) + Send + Sync> = Arc::new(|_| {});
        let on_status = |_| {};

        composition::build_orchestrator(
            self.drive_clients()
                .map_err(|e| format!("HTTP client configuration failed: {e}"))?,
            &self.config_manager,
            &self.platform.steam,
            &self.platform.browser,
            &self.endpoints,
            config,
            cancel,
            transfer_progress,
            on_status,
        )
    }

    pub fn connect_drive(&self, client_id: String, client_secret: String) {
        if !self.try_acquire_admission_for_connect() {
            self.send_toast("Cannot connect Drive: another operation is in progress.");
            return;
        }

        let id = client_id.trim().to_string();
        let secret = client_secret.trim().to_string();
        if id.is_empty() || secret.is_empty() {
            self.send_toast("Client ID and Client Secret cannot be empty.");
            self.release_admission();
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
                self.send_toast(format!("Failed to save credentials: {e}"));
                self.release_admission();
                return;
            }
        };

        self.send_event(AppEvent::ConfigReloaded(config.clone()));
        self.send_toast("Opening browser for Google Drive authorization...");

        let clients = match self.drive_clients() {
            Ok(c) => c,
            Err(e) => {
                self.send_toast(format!("HTTP client error: {e}"));
                self.release_admission();
                return;
            }
        };

        let oauth = InteractiveOAuth::with_endpoints(
            clients.metadata,
            self.config_manager.clone(),
            self.platform.browser.clone(),
            self.endpoints.token_endpoint.clone(),
            self.endpoints.authorization_endpoint.clone(),
            self.endpoints.oauth_address,
            self.endpoints.redirect_uri.clone(),
        );

        let coord = self.clone();
        let handle = self.tokio_handle.spawn(async move {
            match oauth.authorize_scoped(&config, true).await {
                Ok(_) => {
                    if let Ok(updated) = coord.config_manager.load() {
                        coord.send_event(AppEvent::ConfigReloaded(updated));
                    }
                    coord.send_toast("Google Drive connected successfully.");
                    // Reconcile any existing persisted uploads upon connecting
                    if let Ok(statuses) = coord.reconcile_persisted().await {
                        let all_recovered = !statuses.is_empty()
                            && statuses
                                .iter()
                                .all(|(_, s)| matches!(s, PersistedRecoveryStatus::Recovered));
                        if all_recovered {
                            coord.clear_pending_recovery();
                        }
                    }
                }
                Err(err) => {
                    coord.send_toast(format!("Google Drive authorization failed: {err}"));
                }
            }
            coord.release_admission();
        });
        self.track_task(handle);
    }

    pub fn unlink_drive(&self) {
        if !self.try_acquire_admission() {
            self.send_toast("Cannot disconnect Drive: an operation is currently active.");
            return;
        }

        match self.config_manager.update_credentials(|curr| {
            curr.map(|c| Credentials {
                refresh_token: None,
                ..c
            })
        }) {
            Ok(updated) => {
                self.send_event(AppEvent::ConfigReloaded(updated));
                self.send_toast("Google Drive disconnected.");
            }
            Err(err) => {
                self.send_toast(format!("Failed to disconnect Drive: {err}"));
            }
        }
        self.release_admission();
    }

    pub async fn reconcile_active(&self) -> Result<RecoveryStatus, RecoveryError> {
        let config = self.config_manager.load().map_err(RecoveryError::Local)?;
        if !drive_connected(&config) {
            return Err(RecoveryError::NoFailure);
        }

        let clients = self
            .drive_clients()
            .map_err(|_| RecoveryError::UnsupportedFailure)?;
        let oauth = InteractiveOAuth::with_endpoints(
            clients.metadata.clone(),
            self.config_manager.clone(),
            self.platform.browser.clone(),
            self.endpoints.token_endpoint.clone(),
            self.endpoints.authorization_endpoint.clone(),
            self.endpoints.oauth_address,
            self.endpoints.redirect_uri.clone(),
        );
        let (token, _) = oauth
            .authorize_scoped(&config, false)
            .await
            .map_err(|_| RecoveryError::UnsupportedFailure)?;

        let resumable =
            ResumableUpload::new(clients.metadata.clone(), self.endpoints.upload_v3.clone());
        let listing = emusync_data_drive::DriveListing::new(
            clients.metadata.clone(),
            self.endpoints.files_v3.clone(),
        );

        let history = Arc::new(
            emusync_data_local::SyncHistoryStore::new(self.config_manager.path())
                .map_err(RecoveryError::Local)?,
        );
        let recovery_store =
            Arc::new(RecoveryStore::new(self.config_manager.path()).map_err(RecoveryError::Local)?);
        let tracked = Arc::new(
            emusync_data_local::TrackedFileSync::new((), history).with_recovery(recovery_store),
        );

        let mut supervisor = self.supervisor.lock().await;

        // Try conditional reconciliation first, then resumable
        if let Ok(status) =
            reconcile_conditional_upload(&mut *supervisor, &listing, &tracked, &token).await
        {
            if status == RecoveryStatus::Recovered {
                *self.pending_recovery.lock().unwrap() = None;
            }
            return Ok(status);
        }

        if let Ok(status) =
            reconcile_created_upload(&mut *supervisor, &listing, &tracked, &token).await
        {
            if status == RecoveryStatus::Recovered {
                *self.pending_recovery.lock().unwrap() = None;
            }
            return Ok(status);
        }

        let status = reconcile_tracked_upload_with_listing(
            &mut *supervisor,
            &resumable,
            &listing,
            &tracked,
            &token,
        )
        .await?;

        if status == RecoveryStatus::Recovered {
            *self.pending_recovery.lock().unwrap() = None;
        }

        Ok(status)
    }

    pub async fn reconcile_persisted(
        &self,
    ) -> Result<Vec<(String, PersistedRecoveryStatus)>, RecoveryError> {
        let config = self.config_manager.load().map_err(RecoveryError::Local)?;
        if !drive_connected(&config) {
            return Err(RecoveryError::NoFailure);
        }

        let clients = self
            .drive_clients()
            .map_err(|_| RecoveryError::UnsupportedFailure)?;
        let oauth = InteractiveOAuth::with_endpoints(
            clients.metadata.clone(),
            self.config_manager.clone(),
            self.platform.browser.clone(),
            self.endpoints.token_endpoint.clone(),
            self.endpoints.authorization_endpoint.clone(),
            self.endpoints.oauth_address,
            self.endpoints.redirect_uri.clone(),
        );
        let (token, _) = oauth
            .authorize_scoped(&config, false)
            .await
            .map_err(|_| RecoveryError::UnsupportedFailure)?;

        let listing = emusync_data_drive::DriveListing::new(
            clients.metadata.clone(),
            self.endpoints.files_v3.clone(),
        );

        let history = Arc::new(
            emusync_data_local::SyncHistoryStore::new(self.config_manager.path())
                .map_err(RecoveryError::Local)?,
        );
        let store =
            Arc::new(RecoveryStore::new(self.config_manager.path()).map_err(RecoveryError::Local)?);
        let tracked = Arc::new(
            emusync_data_local::TrackedFileSync::new((), history).with_recovery(store.clone()),
        );

        let snapshot = self
            .config_manager
            .snapshot(&config)
            .map_err(RecoveryError::Local)?;
        let credentials =
            RecoveryCredentials::new(Arc::new(self.config_manager.clone()), snapshot, config);

        reconcile_persisted_uploads(store, &listing, &tracked, &token, credentials).await
    }
}
