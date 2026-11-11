//! Form state for editing game overrides and save setup requests (Kotlin's
//! `GameEditFormState`).

use super::save_targets::SaveTargetFormState;
use crate::model::GameItem;
use emusync_domain::library::{SaveTarget, scope_new_targets};

/// Where cloud saves should be restored; the composition turns it into targets with
/// `SaveTargetAccess::for_restore`, which rejects names escaping the destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreChoice {
    pub destination: String,
    pub whole_folder: bool,
}

/// What saving the game dialog produces.
#[derive(Clone, Debug, PartialEq)]
pub struct GameEditResult {
    pub title: Option<String>,
    /// Explicit targets; empty when `restore` is set.
    pub targets: Vec<SaveTarget>,
    pub cover_path: Option<String>,
    pub restore: Option<RestoreChoice>,
}

/// Draft overrides and restore choices for one game, without emulator-wide fallbacks.
#[derive(Clone, Debug, PartialEq)]
pub struct GameEditFormState {
    pub custom_title: String,
    pub cover_path: String,
    pub restore_destination_folder: String,
    pub restore_whole_folder: bool,
    pub saves: SaveTargetFormState,
    pub error_message: Option<String>,
    has_initial_targets: bool,
    /// Restore paths resolved for a destination, and the destination last requested.
    restore_preview: Option<(String, Vec<String>)>,
    requested_preview: Option<String>,
}

impl GameEditFormState {
    /// `game_targets` must be the game's own targets (`game_specific_targets`), never the
    /// emulator-wide ones, or saving would copy them into a per-game override.
    pub fn new(game: &GameItem, game_targets: &[SaveTarget]) -> Self {
        Self {
            custom_title: game.effective_title(),
            cover_path: game.custom_cover_path().unwrap_or_default(),
            restore_destination_folder: String::new(),
            restore_whole_folder: false,
            saves: SaveTargetFormState::from_targets(game_targets),
            error_message: None,
            has_initial_targets: !game_targets.is_empty(),
            restore_preview: None,
            requested_preview: None,
        }
    }

    fn destination(&self) -> Option<&str> {
        Some(self.restore_destination_folder.trim()).filter(|d| !d.is_empty())
    }

    /// Paths the restore would write: the folder itself, or the resolved cloud files.
    pub fn selected_restore_paths(&self, cloud_file_names: &[String]) -> Vec<String> {
        let Some(destination) = self.destination() else {
            return Vec::new();
        };
        if cloud_file_names.is_empty() {
            return Vec::new();
        }
        if self.restore_whole_folder {
            return vec![destination.to_string()];
        }
        match &self.restore_preview {
            Some((resolved_for, paths)) if resolved_for == destination => paths.clone(),
            _ => Vec::new(),
        }
    }

    /// The destination whose per-file restore paths still need resolving, once per value.
    pub fn take_restore_preview_request(&mut self, cloud_file_names: &[String]) -> Option<String> {
        if cloud_file_names.is_empty() || self.restore_whole_folder {
            return None;
        }
        let destination = self.destination()?.to_string();
        let resolved = self
            .restore_preview
            .as_ref()
            .is_some_and(|(d, _)| *d == destination);
        if resolved || self.requested_preview.as_deref() == Some(&destination) {
            return None;
        }
        self.requested_preview = Some(destination.clone());
        Some(destination)
    }

    /// Stores resolved paths if they belong to the destination currently in the form.
    pub fn apply_restore_preview(&mut self, destination: &str, paths: Vec<String>) {
        if self.destination() == Some(destination) {
            self.restore_preview = Some((destination.to_string(), paths));
        }
    }

    pub fn can_save(&self, cloud_file_names: &[String]) -> bool {
        (cloud_file_names.is_empty() || !self.selected_restore_paths(cloud_file_names).is_empty())
            && self.saves.cloud_paths_are_valid()
    }

    pub fn build_result(&mut self, cloud_file_names: &[String]) -> Result<GameEditResult, String> {
        if !self.can_save(cloud_file_names) {
            let msg = if self.saves.cloud_paths_are_valid() {
                "Select a destination folder before saving."
            } else {
                "Cloud paths must be relative and cannot contain '..'."
            };
            self.error_message = Some(msg.to_string());
            return Err(msg.to_string());
        }

        let (targets, restore) = if cloud_file_names.is_empty() {
            let raw = self.saves.build_targets();
            let targets = if self.has_initial_targets {
                raw
            } else {
                scope_new_targets(&raw)
            };
            (targets, None)
        } else {
            let choice = RestoreChoice {
                destination: self.destination().unwrap_or_default().to_string(),
                whole_folder: self.restore_whole_folder,
            };
            (Vec::new(), Some(choice))
        };

        self.error_message = None;
        Ok(GameEditResult {
            title: non_blank(&self.custom_title),
            targets,
            cover_path: non_blank(&self.cover_path),
            restore,
        })
    }
}

fn non_blank(value: &str) -> Option<String> {
    Some(value.trim())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}
