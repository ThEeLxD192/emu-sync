//! Main EmuSync application presentation coordinator and window layout.

use crate::{
    actions::UiAction,
    components::{
        render_game_grid, render_header, render_sidebar, render_toast,
        sidebar_order::SidebarInteraction,
    },
    cover_cache::CoverCache,
    dialogs::{
        ConfirmDeleteOutcome, DriveSetupDialogState, DriveSetupEvents, EntryDialogEvents,
        GameEditDialogEvents, StatusOverlayEvents, render_confirm_delete_dialog,
        render_drive_setup_dialog, render_entry_dialog, render_game_edit_dialog,
        render_status_overlay, render_update_dialog,
    },
    forms::{EntryFormState, GameEditFormState, entry_form::existing_groups},
    model::{AppStatus, GameItem, PathTarget, PickMode, SaveSetupRequest},
    state::AppUiState,
    theme::{BACKGROUND, configure_egui_theme},
};
use egui::{CentralPanel, Frame, Margin, Panel, Ui};
use emusync_domain::library::{GameEntry, SaveTarget, SaveTargetType};

/// Currently active modal dialog.
#[derive(Clone, Debug, PartialEq)]
pub enum ActiveDialog {
    None,
    Entry(Box<EntryFormState>),
    GameEdit {
        form: Box<GameEditFormState>,
        notice: Option<String>,
        cloud_files: Vec<String>,
        game: Box<GameItem>,
    },
    DriveSetup(DriveSetupDialogState),
    Update,
    /// Rust-only confirmation before a category is deleted.
    ConfirmDelete(Box<GameEntry>),
}

/// The top-level presentation coordinator for EmuSync.
pub struct EmuSyncApp {
    pub state: AppUiState,
    pub cover_cache: CoverCache,
    pub active_dialog: ActiveDialog,
    pub pending_actions: Vec<UiAction>,
    pub sidebar: SidebarInteraction,
    /// The conflict already answered, shown as syncing until the status changes.
    answered_conflict: Option<AppStatus>,
    /// Fullscreen flags auto-detected from an emulator executable, injected by the
    /// composition (Kotlin's `LibraryFormActions.detectFullscreenArgs`).
    pub fullscreen_detector: fn(&str) -> String,
}

fn no_fullscreen_flags(_: &str) -> String {
    String::new()
}

impl Default for EmuSyncApp {
    fn default() -> Self {
        Self::new()
    }
}

impl EmuSyncApp {
    pub fn new() -> Self {
        Self {
            state: AppUiState::default(),
            cover_cache: CoverCache::new(),
            active_dialog: ActiveDialog::None,
            pending_actions: Vec::new(),
            sidebar: SidebarInteraction::default(),
            answered_conflict: None,
            fullscreen_detector: no_fullscreen_flags,
        }
    }

    pub fn with_state(state: AppUiState) -> Self {
        Self {
            state,
            cover_cache: CoverCache::new(),
            active_dialog: ActiveDialog::None,
            pending_actions: Vec::new(),
            sidebar: SidebarInteraction::default(),
            answered_conflict: None,
            fullscreen_detector: no_fullscreen_flags,
        }
    }

    /// Fills the path chosen in a picker; a save row takes the kind that was picked,
    /// as Kotlin's File/Folder buttons do.
    pub fn apply_picked_path(&mut self, target: PathTarget, mode: PickMode, path: String) {
        let picked_kind = match mode {
            PickMode::File => SaveTargetType::File,
            PickMode::Directory => SaveTargetType::Directory,
        };
        match (&mut self.active_dialog, target) {
            (ActiveDialog::Entry(form), PathTarget::Executable) => form.executable_path = path,
            (ActiveDialog::Entry(form), PathTarget::RomsDirectory) => form.roms_directory = path,
            (ActiveDialog::Entry(form), PathTarget::CoverPath) => form.cover_path = path,
            (ActiveDialog::Entry(form), PathTarget::SavePath(index)) => {
                form.saves.update_path(index, path);
                form.saves.update_type(index, picked_kind);
            }
            (ActiveDialog::GameEdit { form, .. }, PathTarget::CoverPath) => form.cover_path = path,
            (ActiveDialog::GameEdit { form, .. }, PathTarget::RestoreDestination) => {
                form.restore_destination_folder = path
            }
            (ActiveDialog::GameEdit { form, .. }, PathTarget::SavePath(index)) => {
                form.saves.update_path(index, path);
                form.saves.update_type(index, picked_kind);
            }
            _ => {}
        }
    }

    /// Stores resolved restore paths if the dialog still shows that destination.
    pub fn apply_restore_preview(&mut self, destination: &str, paths: Vec<String>) {
        if let ActiveDialog::GameEdit { form, .. } = &mut self.active_dialog {
            form.apply_restore_preview(destination, paths);
        }
    }

    /// Applies a detected save kind only if the row still holds the path that was checked.
    pub fn apply_detected_save_type(&mut self, index: usize, path: &str, kind: SaveTargetType) {
        let saves = match &mut self.active_dialog {
            ActiveDialog::Entry(form) => &mut form.saves,
            ActiveDialog::GameEdit { form, .. } => &mut form.saves,
            _ => return,
        };
        if saves.rows.get(index).is_some_and(|row| row.path == path) {
            saves.update_type(index, kind);
        }
    }

    pub fn open_add_entry(&mut self) {
        self.active_dialog = ActiveDialog::Entry(Box::new(EntryFormState::new(None, &[])));
    }

    pub fn open_edit_entry(&mut self, entry: &GameEntry) {
        let targets = &entry.common().save_targets;
        self.active_dialog =
            ActiveDialog::Entry(Box::new(EntryFormState::new(Some(entry.clone()), targets)));
    }

    /// Opens the game dialog; `game_targets` are the game's own targets, read by the
    /// composition with `game_specific_targets`.
    pub fn open_edit_game(&mut self, game: &GameItem, game_targets: &[SaveTarget]) {
        self.active_dialog = ActiveDialog::GameEdit {
            form: Box::new(GameEditFormState::new(game, game_targets)),
            notice: None,
            cloud_files: Vec::new(),
            game: Box::new(game.clone()),
        };
    }

    pub fn open_save_setup(&mut self, request: &SaveSetupRequest) {
        self.active_dialog = ActiveDialog::GameEdit {
            form: Box::new(GameEditFormState::new(&request.item, &request.targets)),
            notice: request.notice.clone(),
            cloud_files: request.cloud_file_names.clone(),
            game: Box::new(request.item.clone()),
        };
    }

    pub fn open_drive_setup(&mut self) {
        let (client_id, client_secret) = self
            .state
            .config
            .as_ref()
            .and_then(|c| c.credentials.as_ref())
            .map(|creds| (creds.client_id.clone(), creds.client_secret.clone()))
            .unwrap_or_default();

        let is_linked = self.state.is_drive_connected();
        self.active_dialog = ActiveDialog::DriveSetup(DriveSetupDialogState::new(
            client_id,
            client_secret,
            is_linked,
        ));
    }

    /// Asks for confirmation; nothing is deleted until the user confirms.
    pub fn request_delete_entry(&mut self, entry: GameEntry) {
        self.active_dialog = ActiveDialog::ConfirmDelete(Box::new(entry));
    }

    pub fn open_update_dialog(&mut self) {
        self.active_dialog = ActiveDialog::Update;
    }

    pub fn close_dialog(&mut self) {
        self.active_dialog = ActiveDialog::None;
    }

    pub fn set_toast(&mut self, message: impl Into<String>) {
        self.state.toast_message = Some(message.into());
    }

    pub fn clear_toast(&mut self) {
        self.state.toast_message = None;
    }

    /// Drains all pending actions queued by user interactions.
    pub fn take_actions(&mut self) -> Vec<UiAction> {
        std::mem::take(&mut self.pending_actions)
    }

    /// Processes one frame of UI layout, updates and event collection within a given Ui.
    pub fn update(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        configure_egui_theme(&ctx);

        // Auto-open Save Setup modal when requested by the application layer
        if let Some(req) = &self.state.save_setup_request
            && !matches!(self.active_dialog, ActiveDialog::GameEdit { .. })
        {
            self.open_save_setup(&req.clone());
        }

        // Auto-open Update dialog when requested
        if self.state.show_update_dialog && !matches!(self.active_dialog, ActiveDialog::Update) {
            self.state.show_update_dialog = false;
            self.open_update_dialog();
        }

        let is_modal_active = !matches!(self.state.status, AppStatus::Idle)
            || !matches!(self.active_dialog, ActiveDialog::None);

        let mut actions = Vec::new();
        let mut open_drive = false;
        let mut open_update = false;
        let mut open_add = false;
        let mut edit_game = None;
        let mut edit_category = None;
        let mut delete_category = None;

        // Top Panel: Header
        Panel::top("top_header")
            .frame(
                Frame::new()
                    .fill(BACKGROUND)
                    .inner_margin(Margin::symmetric(20, 14)),
            )
            .show(ui, |ui| {
                ui.add_enabled_ui(!is_modal_active, |ui| {
                    render_header(
                        ui,
                        &self.state,
                        &mut |action| actions.push(action),
                        &mut || open_drive = true,
                        &mut || open_update = true,
                    );
                });
            });

        // Left Panel: Sidebar
        // Fixed width as in Kotlin; a resize handle is easy to drag by accident on touch screens.
        Panel::left("left_sidebar")
            .resizable(false)
            .exact_size(240.0)
            .frame(
                Frame::new()
                    .fill(BACKGROUND)
                    .inner_margin(Margin::symmetric(0, 4)),
            )
            .show(ui, |ui| {
                ui.add_enabled_ui(!is_modal_active, |ui| {
                    render_sidebar(
                        ui,
                        &self.state,
                        &mut self.sidebar,
                        &mut |action| actions.push(action),
                        &mut || open_add = true,
                    );
                });
            });

        for action in &actions {
            if let UiAction::SelectEntry(entry) = action {
                self.state.selected_entry = entry.clone();
            }
        }

        // Center Panel: Game Grid
        CentralPanel::default()
            .frame(Frame::new().fill(BACKGROUND).inner_margin(Margin::same(20)))
            .show(ui, |ui| {
                ui.add_enabled_ui(!is_modal_active, |ui| {
                    render_game_grid(
                        ui,
                        &self.state,
                        &mut self.cover_cache,
                        &mut |action| actions.push(action),
                        &mut |item| edit_game = Some(item),
                        &mut |entry| edit_category = Some(entry),
                        &mut |entry| delete_category = Some(entry),
                    );
                });
            });

        // Handle navigation triggers from the main view (only when no modal is active)
        if is_modal_active {
            actions.clear();
        } else {
            if open_drive {
                self.open_drive_setup();
            }
            if open_update {
                self.open_update_dialog();
            }
            if open_add {
                self.open_add_entry();
            }
            if let Some(game) = edit_game {
                actions.push(UiAction::EditGame(game));
            }
            if let Some(entry) = edit_category {
                self.open_edit_entry(&entry);
            }
            if let Some(entry) = delete_category {
                self.request_delete_entry(entry);
            }
        }

        // Modals
        let mut close_dialog = false;

        match &mut self.active_dialog {
            ActiveDialog::None => {}
            ActiveDialog::Entry(form) => {
                let mut saved_entry = None;
                let mut pick = None;
                let mut edited_paths = Vec::new();
                let groups = existing_groups(
                    self.state
                        .config
                        .as_ref()
                        .map(|c| c.entries.as_slice())
                        .unwrap_or_default(),
                );

                render_entry_dialog(
                    &ctx,
                    form,
                    &groups,
                    self.fullscreen_detector,
                    EntryDialogEvents {
                        on_save: &mut |entry| saved_entry = Some(entry),
                        on_pick_path: &mut |target, mode| pick = Some((target, mode)),
                        on_save_path_edited: &mut |index, path| edited_paths.push((index, path)),
                        on_close: &mut || close_dialog = true,
                    },
                );

                for (index, path) in edited_paths {
                    actions.push(UiAction::DetectSaveTargetType { index, path });
                }

                if let Some(entry) = saved_entry {
                    if let Some(old) = &form.initial_entry {
                        actions.push(UiAction::EditEntry {
                            old: Box::new(old.clone()),
                            new: Box::new(entry),
                        });
                    } else {
                        actions.push(UiAction::AddEntry(entry));
                    }
                    close_dialog = true;
                }

                if let Some((target, mode)) = pick {
                    actions.push(UiAction::PickPath { target, mode });
                }
            }
            ActiveDialog::GameEdit {
                form,
                notice,
                cloud_files,
                game,
            } => {
                let mut saved_result = None;
                let mut pick = None;
                let mut edited_paths = Vec::new();

                render_game_edit_dialog(
                    &ctx,
                    form,
                    game,
                    notice.as_deref(),
                    cloud_files,
                    &mut self.cover_cache,
                    GameEditDialogEvents {
                        on_save: &mut |result| saved_result = Some(result),
                        on_pick_path: &mut |target, mode| pick = Some((target, mode)),
                        on_save_path_edited: &mut |index, path| edited_paths.push((index, path)),
                        on_close: &mut || close_dialog = true,
                    },
                );

                if let Some(destination) = form.take_restore_preview_request(cloud_files) {
                    actions.push(UiAction::PreviewRestorePaths {
                        destination,
                        names: cloud_files.clone(),
                    });
                }
                for (index, path) in edited_paths {
                    actions.push(UiAction::DetectSaveTargetType { index, path });
                }

                if let Some(result) = saved_result {
                    if let Some(req) = &self.state.save_setup_request {
                        actions.push(UiAction::CompleteSaveSetup {
                            request: Box::new(req.clone()),
                            targets: result.targets,
                            title: result.title,
                            cover_path: result.cover_path,
                            restore: result.restore,
                        });
                        self.state.save_setup_request = None;
                    } else {
                        actions.push(UiAction::EditGameOverride {
                            item: *game.clone(),
                            title: result.title,
                            targets: result.targets,
                            cover_path: result.cover_path,
                        });
                    }
                    close_dialog = true;
                }

                if let Some((target, mode)) = pick {
                    actions.push(UiAction::PickPath { target, mode });
                }
            }
            ActiveDialog::DriveSetup(drive_state) => {
                let mut connect_args = None;
                let mut unlink_clicked = false;
                let mut open_cfg = false;

                render_drive_setup_dialog(
                    &ctx,
                    drive_state,
                    &self.state.config_path,
                    DriveSetupEvents {
                        on_save_connect: &mut |id, sec| connect_args = Some((id, sec)),
                        on_unlink: &mut || unlink_clicked = true,
                        on_open_config: &mut || open_cfg = true,
                        on_close: &mut || close_dialog = true,
                    },
                );

                if let Some((client_id, client_secret)) = connect_args {
                    actions.push(UiAction::ConnectDrive {
                        client_id,
                        client_secret,
                    });
                }
                if unlink_clicked {
                    actions.push(UiAction::UnlinkDrive);
                }
                if open_cfg {
                    actions.push(UiAction::OpenConfigDirectory);
                }
            }
            ActiveDialog::Update => {
                let mut download_info = None;
                let mut restart_path = None;

                render_update_dialog(
                    &ctx,
                    &self.state.update_state,
                    &self.state.current_version,
                    &mut |info| download_info = Some(info),
                    &mut |path| restart_path = Some(path),
                    &mut || close_dialog = true,
                );

                if let Some(info) = download_info {
                    actions.push(UiAction::DownloadUpdate(info));
                }
                if let Some(path) = restart_path {
                    actions.push(UiAction::RestartApp(path));
                }
            }
            ActiveDialog::ConfirmDelete(entry) => {
                let steam_registered = self.state.is_steam_registered(entry);
                match render_confirm_delete_dialog(&ctx, &entry.common().name, steam_registered) {
                    ConfirmDeleteOutcome::Confirmed => {
                        actions.push(UiAction::DeleteEntry((**entry).clone()));
                        close_dialog = true;
                    }
                    ConfirmDeleteOutcome::Cancelled => close_dialog = true,
                    ConfirmDeleteOutcome::Pending => {}
                }
            }
        }

        if close_dialog {
            self.close_dialog();
        }

        // Status overlay
        let mut resolve_conflict = None;
        let mut retry_post_sync = false;
        let mut dismiss_status = false;

        // A choice is remembered only for the conflict it answered, like Kotlin's
        // `remember(gameName, localDate, cloudDate)`; any other status forgets it.
        if self.answered_conflict.as_ref() != Some(&self.state.status) {
            self.answered_conflict = None;
        }
        render_status_overlay(
            &ctx,
            &self.state.status,
            self.answered_conflict.is_some(),
            StatusOverlayEvents {
                on_resolve_conflict: &mut |decision| resolve_conflict = Some(decision),
                on_retry_post_sync: &mut || retry_post_sync = true,
                on_dismiss: &mut || dismiss_status = true,
            },
        );

        if let Some(decision) = resolve_conflict {
            actions.push(UiAction::ResolveConflict(decision));
            self.answered_conflict = Some(self.state.status.clone());
        }
        if retry_post_sync {
            actions.push(UiAction::RetryPostSync);
        }
        if dismiss_status {
            actions.push(UiAction::ClearStatus);
            self.state.status = AppStatus::Idle;
        }

        // Toast overlay
        let mut dismiss_toast = false;
        if let Some(msg) = &self.state.toast_message {
            render_toast(ui, msg, &mut || dismiss_toast = true);
        }
        if dismiss_toast {
            self.clear_toast();
        }

        self.pending_actions.extend(actions);
    }
}
