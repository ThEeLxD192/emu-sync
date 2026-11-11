//! Status overlay frame: conflict answers, post-sync retry and dismissal.

use super::{ActiveDialog, EmuSyncApp, GameEditOrigin};
use crate::{
    actions::UiAction,
    dialogs::{StatusOverlayEvents, render_status_overlay},
    model::AppStatus,
};

impl EmuSyncApp {
    pub(super) fn show_status_overlay(&mut self, ctx: &egui::Context, actions: &mut Vec<UiAction>) {
        let mut resolve_conflict = None;
        let mut choose_save_folders = None;
        let mut retry_post_sync = false;
        let mut dismiss_status = false;

        // A choice is remembered only for the conflict it answered, like Kotlin's
        // `remember(gameName, localDate, cloudDate)`; any other status forgets it.
        if self.answered_conflict.as_ref() != Some(&self.state.status) {
            self.answered_conflict = None;
        }
        self.save_folder_selection.follow(&self.state.status);
        // A save setup stays interactable above the non-interactive launch states.
        let setup_open = matches!(
            self.active_dialog,
            ActiveDialog::GameEdit {
                origin: GameEditOrigin::SaveSetup(_),
                ..
            }
        );
        let passive_status = matches!(
            self.state.status,
            AppStatus::Playing | AppStatus::Loading | AppStatus::Syncing { .. }
        );
        if !(setup_open && passive_status) {
            render_status_overlay(
                ctx,
                &self.state.status,
                self.answered_conflict.is_some(),
                &mut self.save_folder_selection,
                StatusOverlayEvents {
                    on_resolve_conflict: &mut |decision| resolve_conflict = Some(decision),
                    on_choose_save_folders: &mut |choice| choose_save_folders = Some(choice),
                    on_retry_post_sync: &mut || retry_post_sync = true,
                    on_dismiss: &mut || dismiss_status = true,
                },
            );
        }

        if let Some(decision) = resolve_conflict {
            actions.push(UiAction::ResolveConflict(decision));
            self.answered_conflict = Some(self.state.status.clone());
        }
        if let Some(choice) = choose_save_folders {
            actions.push(UiAction::ResolveSaveFolders(choice));
        }
        if retry_post_sync {
            actions.push(UiAction::RetryPostSync);
        }
        if dismiss_status {
            actions.push(UiAction::ClearStatus);
            self.state.status = AppStatus::Idle;
        }
    }
}
