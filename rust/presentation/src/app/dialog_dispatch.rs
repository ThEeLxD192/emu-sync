//! Renders each modal dialog and turns its events into `UiAction`s.

use super::GameEditOrigin;
use crate::{
    actions::UiAction,
    cover_cache::CoverCache,
    dialogs::{
        ConfirmDeleteOutcome, DriveSetupDialogState, DriveSetupEvents, EntryDialogEvents,
        GameEditDialogEvents, UpdateDialogEvents, render_confirm_delete_dialog,
        render_drive_setup_dialog, render_entry_dialog, render_game_edit_dialog,
        render_update_dialog,
    },
    forms::{EntryFormState, GameEditFormState, entry_form::existing_groups},
    model::GameItem,
    state::AppUiState,
};
use emusync_domain::library::GameEntry;

/// How a game dialog frame ended.
pub(super) enum GameEditOutcome {
    Open,
    /// Dismissed without saving.
    Closed,
    /// Saved; a save setup was answered by `CompleteSaveSetup`.
    Saved {
        setup_completed: bool,
    },
}

/// Returns whether the dialog should close.
pub(super) fn handle_entry(
    ctx: &egui::Context,
    form: &mut EntryFormState,
    state: &AppUiState,
    fullscreen_detector: fn(&str) -> String,
    actions: &mut Vec<UiAction>,
) -> bool {
    let mut close = false;
    let mut saved_entry = None;
    let mut pick = None;
    let mut edited_paths = Vec::new();
    let groups = existing_groups(
        state
            .config
            .as_ref()
            .map(|c| c.entries.as_slice())
            .unwrap_or_default(),
    );

    render_entry_dialog(
        ctx,
        form,
        &groups,
        fullscreen_detector,
        EntryDialogEvents {
            on_save: &mut |entry| saved_entry = Some(entry),
            on_pick_path: &mut |target, mode| pick = Some((target, mode)),
            on_save_path_edited: &mut |index, path| edited_paths.push((index, path)),
            on_close: &mut || close = true,
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
        close = true;
    }

    if let Some((target, mode)) = pick {
        actions.push(UiAction::PickPath { target, mode });
    }
    close
}

/// Borrowed parts of the open game dialog.
pub(super) struct GameEditParts<'a> {
    pub(super) form: &'a mut GameEditFormState,
    pub(super) notice: Option<&'a str>,
    pub(super) cloud_files: &'a [String],
    pub(super) game: &'a GameItem,
    pub(super) origin: &'a GameEditOrigin,
}

pub(super) fn handle_game_edit(
    ctx: &egui::Context,
    parts: GameEditParts<'_>,
    cover_cache: &mut CoverCache,
    actions: &mut Vec<UiAction>,
) -> GameEditOutcome {
    let GameEditParts {
        form,
        notice,
        cloud_files,
        game,
        origin,
    } = parts;
    let mut close = false;
    let mut saved_result = None;
    let mut pick = None;
    let mut edited_paths = Vec::new();

    render_game_edit_dialog(
        ctx,
        form,
        game,
        notice,
        cloud_files,
        cover_cache,
        GameEditDialogEvents {
            on_save: &mut |result| saved_result = Some(result),
            on_pick_path: &mut |target, mode| pick = Some((target, mode)),
            on_save_path_edited: &mut |index, path| edited_paths.push((index, path)),
            on_close: &mut || close = true,
        },
    );

    if let Some(destination) = form.take_restore_preview_request(cloud_files) {
        actions.push(UiAction::PreviewRestorePaths {
            destination,
            names: cloud_files.to_vec(),
        });
    }
    for (index, path) in edited_paths {
        actions.push(UiAction::DetectSaveTargetType { index, path });
    }

    let mut outcome = if close {
        GameEditOutcome::Closed
    } else {
        GameEditOutcome::Open
    };
    if let Some(result) = saved_result {
        let setup_completed = match origin {
            GameEditOrigin::SaveSetup(request) => {
                actions.push(UiAction::CompleteSaveSetup {
                    request: request.clone(),
                    targets: result.targets,
                    title: result.title,
                    cover_path: result.cover_path,
                    restore: result.restore,
                });
                true
            }
            GameEditOrigin::Override => {
                actions.push(UiAction::EditGameOverride {
                    item: game.clone(),
                    title: result.title,
                    targets: result.targets,
                    cover_path: result.cover_path,
                });
                false
            }
        };
        outcome = GameEditOutcome::Saved { setup_completed };
    }

    if let Some((target, mode)) = pick {
        actions.push(UiAction::PickPath { target, mode });
    }
    outcome
}

/// Returns whether the dialog should close.
pub(super) fn handle_drive(
    ctx: &egui::Context,
    drive_state: &mut DriveSetupDialogState,
    config_path: &str,
    actions: &mut Vec<UiAction>,
) -> bool {
    let mut close = false;
    let mut connect_args = None;
    let mut unlink_clicked = false;
    let mut open_cfg = false;

    render_drive_setup_dialog(
        ctx,
        drive_state,
        config_path,
        DriveSetupEvents {
            on_save_connect: &mut |id, sec| connect_args = Some((id, sec)),
            on_unlink: &mut || unlink_clicked = true,
            on_open_config: &mut || open_cfg = true,
            on_close: &mut || close = true,
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
    close
}

/// Returns whether the dialog should close.
pub(super) fn handle_update(
    ctx: &egui::Context,
    state: &AppUiState,
    actions: &mut Vec<UiAction>,
) -> bool {
    let mut close = false;
    let mut download_info = None;
    let mut restart_path = None;

    render_update_dialog(
        ctx,
        &state.update_state,
        &state.current_version,
        UpdateDialogEvents {
            on_download: &mut |info| download_info = Some(info),
            on_restart: &mut |path| restart_path = Some(path),
            on_close: &mut || close = true,
        },
    );

    if let Some(info) = download_info {
        actions.push(UiAction::DownloadUpdate(info));
    }
    if let Some(path) = restart_path {
        actions.push(UiAction::RestartApp(path));
    }
    close
}

/// Returns whether the dialog should close.
pub(super) fn handle_confirm_delete(
    ctx: &egui::Context,
    entry: &GameEntry,
    state: &AppUiState,
    actions: &mut Vec<UiAction>,
) -> bool {
    let steam_registered = state.is_steam_registered(entry);
    match render_confirm_delete_dialog(ctx, &entry.common().name, steam_registered) {
        ConfirmDeleteOutcome::Confirmed => {
            actions.push(UiAction::DeleteEntry(entry.clone()));
            true
        }
        ConfirmDeleteOutcome::Cancelled => true,
        ConfirmDeleteOutcome::Pending => false,
    }
}
