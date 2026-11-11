//! Main EmuSync application presentation coordinator and window layout.

mod dialog_dispatch;
mod status;

use crate::{
    actions::UiAction,
    components::{
        render_game_grid, render_header, render_sidebar, render_toast,
        sidebar_order::SidebarInteraction,
    },
    cover_cache::CoverCache,
    dialogs::DriveSetupDialogState,
    forms::{EntryFormState, GameEditFormState},
    model::{AppStatus, GameItem, PathTarget, PickMode, SaveSetupRequest},
    state::AppUiState,
    theme::BACKGROUND,
};
use dialog_dispatch::{GameEditOutcome, GameEditParts};
use egui::{CentralPanel, Frame, Margin, Panel, Ui};
use emusync_domain::library::{GameEntry, SaveTarget, SaveTargetType};
use std::collections::HashMap;

/// Where a game dialog came from, which decides how it ends.
#[derive(Clone, Debug, PartialEq)]
pub enum GameEditOrigin {
    /// Opened by the user to override a game's title, cover or saves.
    Override,
    /// Opened by a launch that needs a save setup; it must end in
    /// `CompleteSaveSetup` or `CancelSaveSetup`.
    SaveSetup(Box<SaveSetupRequest>),
}

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
        origin: GameEditOrigin,
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
    active_dialog: ActiveDialog,
    pending_actions: Vec<UiAction>,
    pub sidebar: SidebarInteraction,
    /// The conflict already answered, shown as syncing until the status changes.
    answered_conflict: Option<AppStatus>,
    /// Fullscreen flags auto-detected from an emulator executable, injected by the
    /// composition (Kotlin's `LibraryFormActions.detectFullscreenArgs`).
    pub fullscreen_detector: fn(&str) -> String,
    /// Last request id handed out; ids are monotonic and start at 1.
    last_request_id: u64,
    /// Picker requests of the open dialog still unanswered, by request id.
    pending_picks: HashMap<u64, PathTarget>,
    /// Restore preview requests of the open dialog still unanswered, with their destination.
    pending_previews: HashMap<u64, String>,
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
            last_request_id: 0,
            pending_picks: HashMap::new(),
            pending_previews: HashMap::new(),
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
            last_request_id: 0,
            pending_picks: HashMap::new(),
            pending_previews: HashMap::new(),
        }
    }

    /// Fills the path chosen in a picker; a save row takes the kind that was picked,
    /// as Kotlin's File/Folder buttons do. Only the answer to a request of the open
    /// dialog is applied; unknown, repeated or stale ids are dropped.
    pub fn apply_picked_path(
        &mut self,
        request_id: u64,
        target: PathTarget,
        mode: PickMode,
        path: String,
    ) {
        if self.pending_picks.remove(&request_id).as_ref() != Some(&target) {
            return;
        }
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

    /// Stores resolved restore paths if they answer a request of the open dialog for
    /// that destination; unknown, repeated or stale ids are dropped.
    pub fn apply_restore_preview(
        &mut self,
        request_id: u64,
        destination: &str,
        paths: Vec<String>,
    ) {
        if self.pending_previews.remove(&request_id).as_deref() != Some(destination) {
            return;
        }
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

    /// Replaces the open dialog. A save setup it dismisses is cancelled, so its
    /// pending request never stays unanswered.
    fn set_dialog(&mut self, dialog: ActiveDialog) {
        if let ActiveDialog::GameEdit {
            origin: GameEditOrigin::SaveSetup(request),
            ..
        } = &self.active_dialog
        {
            self.pending_actions.push(UiAction::CancelSaveSetup {
                request_id: request.id,
            });
        }
        self.forget_pending_requests();
        self.active_dialog = dialog;
    }

    /// Closes the save setup for `request_id` without answering it; the coordinator
    /// already did. Any other dialog is left untouched.
    pub fn close_save_setup(&mut self, request_id: u64) {
        if matches!(
            &self.active_dialog,
            ActiveDialog::GameEdit {
                origin: GameEditOrigin::SaveSetup(request),
                ..
            } if request.id == request_id
        ) {
            self.forget_pending_requests();
            self.active_dialog = ActiveDialog::None;
        }
    }

    /// Drops every unanswered picker and preview request: whatever answers later
    /// belongs to a dialog that is no longer open.
    fn forget_pending_requests(&mut self) {
        self.pending_picks.clear();
        self.pending_previews.clear();
    }

    /// Gives each picker and preview action a fresh id and records it as pending.
    /// The dialog handlers publish them with the placeholder id 0; this pass runs
    /// before the actions are queued, so no published action keeps it.
    fn register_requests(&mut self, actions: &mut [UiAction]) {
        for action in actions {
            match action {
                UiAction::PickPath {
                    request_id, target, ..
                } => {
                    self.last_request_id += 1;
                    *request_id = self.last_request_id;
                    self.pending_picks.insert(*request_id, target.clone());
                }
                UiAction::PreviewRestorePaths {
                    request_id,
                    destination,
                    ..
                } => {
                    self.last_request_id += 1;
                    *request_id = self.last_request_id;
                    self.pending_previews
                        .insert(*request_id, destination.clone());
                }
                _ => {}
            }
        }
    }

    /// The open modal dialog, read-only.
    pub fn active_dialog(&self) -> &ActiveDialog {
        &self.active_dialog
    }

    /// The open entry form, editable but not replaceable, so a dialog can only be
    /// swapped through the methods that answer a pending save setup.
    pub fn entry_form_mut(&mut self) -> Option<&mut EntryFormState> {
        match &mut self.active_dialog {
            ActiveDialog::Entry(form) => Some(form),
            _ => None,
        }
    }

    /// Whether any modal dialog is open.
    pub fn has_open_dialog(&self) -> bool {
        !matches!(self.active_dialog, ActiveDialog::None)
    }

    pub fn open_add_entry(&mut self) {
        self.set_dialog(ActiveDialog::Entry(Box::new(EntryFormState::new(
            None,
            &[],
        ))));
    }

    pub fn open_edit_entry(&mut self, entry: &GameEntry) {
        let targets = &entry.common().save_targets;
        self.set_dialog(ActiveDialog::Entry(Box::new(EntryFormState::new(
            Some(entry.clone()),
            targets,
        ))));
    }

    /// Opens the game dialog; `game_targets` are the game's own targets, read by the
    /// composition with `game_specific_targets`.
    pub fn open_edit_game(&mut self, game: &GameItem, game_targets: &[SaveTarget]) {
        self.cover_cache.forget_failures();
        self.set_dialog(ActiveDialog::GameEdit {
            form: Box::new(GameEditFormState::new(game, game_targets)),
            notice: None,
            cloud_files: Vec::new(),
            game: Box::new(game.clone()),
            origin: GameEditOrigin::Override,
        });
    }

    /// Opens the save setup for `request`; it ends in `CompleteSaveSetup` or
    /// `CancelSaveSetup`.
    pub fn open_save_setup(&mut self, request: &SaveSetupRequest) {
        self.cover_cache.forget_failures();
        self.set_dialog(ActiveDialog::GameEdit {
            form: Box::new(GameEditFormState::new(&request.item, &request.targets)),
            notice: request.notice.clone(),
            cloud_files: request.cloud_file_names.clone(),
            game: Box::new(request.item.clone()),
            origin: GameEditOrigin::SaveSetup(Box::new(request.clone())),
        });
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
        self.set_dialog(ActiveDialog::DriveSetup(DriveSetupDialogState::new(
            client_id,
            client_secret,
            is_linked,
        )));
    }

    /// Asks for confirmation; nothing is deleted until the user confirms.
    pub fn request_delete_entry(&mut self, entry: GameEntry) {
        self.set_dialog(ActiveDialog::ConfirmDelete(Box::new(entry)));
    }

    pub fn open_update_dialog(&mut self) {
        self.set_dialog(ActiveDialog::Update);
    }

    pub fn close_dialog(&mut self) {
        self.set_dialog(ActiveDialog::None);
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
        let mut setup_completed = false;

        match &mut self.active_dialog {
            ActiveDialog::None => {}
            ActiveDialog::Entry(form) => {
                close_dialog = dialog_dispatch::handle_entry(
                    &ctx,
                    form,
                    &self.state,
                    self.fullscreen_detector,
                    &mut actions,
                );
            }
            ActiveDialog::GameEdit {
                form,
                notice,
                cloud_files,
                game,
                origin,
            } => {
                let parts = GameEditParts {
                    form,
                    notice: notice.as_deref(),
                    cloud_files,
                    game,
                    origin,
                };
                match dialog_dispatch::handle_game_edit(
                    &ctx,
                    parts,
                    &mut self.cover_cache,
                    &mut actions,
                ) {
                    GameEditOutcome::Open => {}
                    GameEditOutcome::Closed => close_dialog = true,
                    GameEditOutcome::Saved {
                        setup_completed: done,
                    } => {
                        close_dialog = true;
                        setup_completed = done;
                    }
                }
            }
            ActiveDialog::DriveSetup(drive_state) => {
                close_dialog = dialog_dispatch::handle_drive(
                    &ctx,
                    drive_state,
                    &self.state.config_path,
                    &mut actions,
                );
            }
            ActiveDialog::Update => {
                close_dialog = dialog_dispatch::handle_update(&ctx, &self.state, &mut actions);
            }
            ActiveDialog::ConfirmDelete(entry) => {
                close_dialog =
                    dialog_dispatch::handle_confirm_delete(&ctx, entry, &self.state, &mut actions);
            }
        }

        self.register_requests(&mut actions);

        if close_dialog {
            if setup_completed {
                // Answered by `CompleteSaveSetup`; closing it must not cancel.
                self.forget_pending_requests();
                self.active_dialog = ActiveDialog::None;
            } else {
                self.close_dialog();
            }
        }

        self.show_status_overlay(&ctx, &mut actions);

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
