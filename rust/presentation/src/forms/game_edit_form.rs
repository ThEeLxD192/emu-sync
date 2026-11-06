//! Form state for editing game overrides and save setup requests.

use super::save_targets::SaveTargetFormState;
use crate::model::GameItem;
use emusync_domain::library::{SaveTarget, SaveTargetType, scope_new_targets};
use std::path::Path;

/// Result tuple containing (title_override, save_targets, cover_override).
pub type GameEditResult = (Option<String>, Vec<SaveTarget>, Option<String>);

/// Draft overrides and restore choices for one game.
#[derive(Clone, Debug, PartialEq)]
pub struct GameEditFormState {
    pub custom_title: String,
    pub cover_path: String,
    pub restore_destination_folder: String,
    pub restore_whole_folder: bool,
    pub saves: SaveTargetFormState,
    pub error_message: Option<String>,
    has_initial_targets: bool,
}

impl GameEditFormState {
    pub fn new(game: &GameItem, initial_targets: &[SaveTarget]) -> Self {
        let custom_title = game.effective_title();
        let cover_path = game.effective_cover_path().unwrap_or_default();
        let saves = SaveTargetFormState::from_targets(initial_targets);
        let has_initial_targets = !initial_targets.is_empty();

        Self {
            custom_title,
            cover_path,
            restore_destination_folder: String::new(),
            restore_whole_folder: false,
            saves,
            error_message: None,
            has_initial_targets,
        }
    }

    pub fn restore_paths(&self, cloud_file_names: &[String]) -> Vec<String> {
        let destination = self.restore_destination_folder.trim();
        if cloud_file_names.is_empty() || destination.is_empty() {
            return Vec::new();
        }
        if self.restore_whole_folder {
            vec![destination.to_string()]
        } else {
            let mut paths = Vec::new();
            for name in cloud_file_names {
                let resolved = Path::new(destination)
                    .join(name)
                    .to_string_lossy()
                    .to_string();
                if !paths.contains(&resolved) {
                    paths.push(resolved);
                }
            }
            paths
        }
    }

    pub fn can_save(&self, cloud_file_names: &[String]) -> bool {
        let dest = self.restore_destination_folder.trim();
        let valid_restore = cloud_file_names.is_empty() || !dest.is_empty();
        valid_restore && self.saves.cloud_paths_are_valid()
    }

    pub fn build_result(&mut self, cloud_file_names: &[String]) -> Result<GameEditResult, String> {
        if !self.can_save(cloud_file_names) {
            let msg = if !self.saves.cloud_paths_are_valid() {
                "Cloud paths must be relative and cannot contain '..'."
            } else {
                "Please choose a restore destination folder."
            };
            self.error_message = Some(msg.to_string());
            return Err(msg.to_string());
        }

        let raw_targets = if cloud_file_names.is_empty() {
            self.saves.build_targets()
        } else {
            let dest = self.restore_destination_folder.trim().to_string();
            if self.restore_whole_folder {
                vec![SaveTarget {
                    path: dest,
                    kind: SaveTargetType::Directory,
                    cloud_path: None,
                }]
            } else {
                let mut targets = Vec::new();
                for name in cloud_file_names {
                    let path = Path::new(&dest).join(name).to_string_lossy().to_string();
                    targets.push(SaveTarget {
                        path,
                        kind: SaveTargetType::File,
                        cloud_path: Some(name.clone()),
                    });
                }
                targets
            }
        };

        let targets = if cloud_file_names.is_empty() && !self.has_initial_targets {
            scope_new_targets(&raw_targets)
        } else {
            raw_targets
        };

        let title_override = {
            let trimmed = self.custom_title.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        let cover_override = {
            let trimmed = self.cover_path.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        self.error_message = None;
        Ok((title_override, targets, cover_override))
    }
}
