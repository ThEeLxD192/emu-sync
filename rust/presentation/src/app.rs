//! Main EmuSync application presentation coordinator and window layout.

use crate::{
    actions::UiAction,
    components::{
        render_game_grid, render_header, render_sidebar, render_toast,
        sidebar_order::SidebarInteraction,
    },
    cover_cache::CoverCache,
    dialogs::{
        DriveSetupDialogState, render_drive_setup_dialog, render_entry_dialog,
        render_game_edit_dialog, render_status_overlay, render_update_dialog,
    },
    forms::{EntryFormState, GameEditFormState},
    model::{AppStatus, GameItem, SaveSetupRequest},
    state::AppUiState,
    theme::{BACKGROUND, configure_egui_theme},
};
use egui::{CentralPanel, Frame, Margin, Panel, Ui};
use emusync_domain::library::GameEntry;

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
}

/// The top-level presentation coordinator for EmuSync.
pub struct EmuSyncApp {
    pub state: AppUiState,
    pub cover_cache: CoverCache,
    pub active_dialog: ActiveDialog,
    pub pending_actions: Vec<UiAction>,
    pub sidebar: SidebarInteraction,
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
        }
    }

    pub fn with_state(state: AppUiState) -> Self {
        Self {
            state,
            cover_cache: CoverCache::new(),
            active_dialog: ActiveDialog::None,
            pending_actions: Vec::new(),
            sidebar: SidebarInteraction::default(),
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

    pub fn open_edit_game(&mut self, game: &GameItem) {
        let targets = &game.entry.common().save_targets;
        self.active_dialog = ActiveDialog::GameEdit {
            form: Box::new(GameEditFormState::new(game, targets)),
            notice: None,
            cloud_files: Vec::new(),
            game: Box::new(game.clone()),
        };
    }

    pub fn open_save_setup(&mut self, request: &SaveSetupRequest) {
        let targets = &request.item.entry.common().save_targets;
        self.active_dialog = ActiveDialog::GameEdit {
            form: Box::new(GameEditFormState::new(&request.item, targets)),
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
            .frame(Frame::new().fill(BACKGROUND).inner_margin(Margin::same(16)))
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
                self.open_edit_game(&game);
            }
            if let Some(entry) = edit_category {
                self.open_edit_entry(&entry);
            }
            if let Some(entry) = delete_category {
                actions.push(UiAction::DeleteEntry(entry));
            }
        }

        // Modals
        let mut close_dialog = false;

        match &mut self.active_dialog {
            ActiveDialog::None => {}
            ActiveDialog::Entry(form) => {
                let mut saved_entry = None;
                let mut pick = None;

                render_entry_dialog(
                    &ctx,
                    form,
                    &mut |entry| saved_entry = Some(entry),
                    &mut |target, mode| pick = Some((target, mode)),
                    &mut || close_dialog = true,
                );

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

                render_game_edit_dialog(
                    &ctx,
                    form,
                    notice.as_deref(),
                    cloud_files,
                    &mut |title, targets, cover| saved_result = Some((title, targets, cover)),
                    &mut |target, mode| pick = Some((target, mode)),
                    &mut || close_dialog = true,
                );

                if let Some((title, targets, cover)) = saved_result {
                    if let Some(req) = &self.state.save_setup_request {
                        actions.push(UiAction::CompleteSaveSetup {
                            request: Box::new(req.clone()),
                            targets,
                            title,
                            cover_path: cover,
                        });
                        self.state.save_setup_request = None;
                    } else {
                        actions.push(UiAction::EditGameOverride {
                            item: *game.clone(),
                            title,
                            targets,
                            cover_path: cover,
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
                    &mut |id, sec| connect_args = Some((id, sec)),
                    &mut || unlink_clicked = true,
                    &mut || open_cfg = true,
                    &mut || close_dialog = true,
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
        }

        if close_dialog {
            self.close_dialog();
        }

        // Status overlay
        let mut resolve_conflict = None;
        let mut retry_post_sync = false;
        let mut dismiss_status = false;

        render_status_overlay(
            &ctx,
            &self.state.status,
            &mut |decision| resolve_conflict = Some(decision),
            &mut || retry_post_sync = true,
            &mut || dismiss_status = true,
        );

        if let Some(decision) = resolve_conflict {
            actions.push(UiAction::ResolveConflict(decision));
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
