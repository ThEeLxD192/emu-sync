//! Form state for creating and editing library categories (emulators and native games).

use super::save_targets::SaveTargetFormState;
use crate::model::EntryType;
use emusync_domain::{
    execution::{format_arguments, parse_arguments},
    library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget},
    sync::remote_path_segments,
};

/// Draft fields and validation for adding or editing a library entry.
#[derive(Clone, Debug, PartialEq)]
pub struct EntryFormState {
    pub entry_type: EntryType,
    pub name: String,
    pub group: String,
    pub cloud_folder: Option<String>,
    pub executable_path: String,
    pub arguments: String,
    pub roms_directory: String,
    pub extensions: String,
    pub wait_for_process: String,
    pub fullscreen_args: String,
    pub covers_directory: String,
    pub cover_path: String,
    pub saves: SaveTargetFormState,
    pub error_message: Option<String>,
    pub initial_entry: Option<GameEntry>,
}

impl EntryFormState {
    pub fn new(initial_entry: Option<GameEntry>, initial_targets: &[SaveTarget]) -> Self {
        let (
            entry_type,
            name,
            group,
            cloud_folder,
            exec,
            args,
            roms,
            exts,
            wait,
            fulls,
            covers,
            cover_p,
        ) = match &initial_entry {
            Some(GameEntry::Emulator(emu)) => (
                EntryType::Emulator,
                emu.common.name.clone(),
                emu.common.group.clone().unwrap_or_default(),
                emu.common.cloud_folder.clone(),
                emu.common.executable_path.clone(),
                format_arguments(&emu.common.arguments),
                emu.roms_directory.clone(),
                emu.extensions.join(", "),
                String::new(),
                emu.fullscreen_args.clone().unwrap_or_default(),
                emu.covers_directory.clone().unwrap_or_default(),
                String::new(),
            ),
            Some(GameEntry::Native(nat)) => (
                EntryType::Native,
                nat.common.name.clone(),
                nat.common.group.clone().unwrap_or_default(),
                nat.common.cloud_folder.clone(),
                nat.common.executable_path.clone(),
                format_arguments(&nat.common.arguments),
                String::new(),
                String::new(),
                nat.wait_for_process.clone().unwrap_or_default(),
                String::new(),
                String::new(),
                nat.cover_path.clone().unwrap_or_default(),
            ),
            None => (
                EntryType::Emulator,
                String::new(),
                String::new(),
                None,
                String::new(),
                "{ROM}".to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
        };

        let saves = SaveTargetFormState::from_targets(initial_targets);

        Self {
            entry_type,
            name,
            group,
            cloud_folder,
            executable_path: exec,
            arguments: args,
            roms_directory: roms,
            extensions: exts,
            wait_for_process: wait,
            fullscreen_args: fulls,
            covers_directory: covers,
            cover_path: cover_p,
            saves,
            error_message: None,
            initial_entry,
        }
    }

    /// Drive folder the saves will use: the stored cloud folder, else the group, else the
    /// name (Kotlin's `cloudFolder.ifBlank { group.ifBlank { name } }`).
    pub fn cloud_folder_preview(&self) -> String {
        [
            self.cloud_folder.as_deref().unwrap_or(""),
            &self.group,
            &self.name,
        ]
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or("...")
        .to_string()
    }

    /// Toggles a group chip: selecting the current group (ignoring case) clears it.
    pub fn toggle_group(&mut self, group: &str) {
        self.group = if self.group_is(group) {
            String::new()
        } else {
            group.to_string()
        };
    }

    /// Whether `group` is the current group, ignoring case.
    pub fn group_is(&self, group: &str) -> bool {
        self.group.to_lowercase() == group.to_lowercase()
    }

    pub fn apply_type_defaults(&mut self) {
        match self.entry_type {
            EntryType::Emulator => {
                if self.arguments.trim().is_empty() {
                    self.arguments = "{ROM}".to_string();
                }
            }
            EntryType::Native => {
                if self.arguments.trim() == "{ROM}" {
                    self.arguments = String::new();
                }
            }
        }
    }

    pub fn build_entry(&mut self) -> Result<GameEntry, String> {
        let raw_targets = self.saves.build_targets();

        if raw_targets.iter().any(|t| {
            t.cloud_path
                .as_deref()
                .is_some_and(|p| remote_path_segments(p.trim()).is_none())
        }) {
            let msg = "Cloud paths must be relative and cannot contain '..'.";
            self.error_message = Some(msg.to_string());
            return Err(msg.to_string());
        }

        let targets = self.saves.scoped_targets();

        let trimmed_name = self.name.trim();
        let trimmed_exec = self.executable_path.trim();

        if trimmed_name.is_empty() || trimmed_exec.is_empty() {
            let msg = "Please fill in all required fields.";
            self.error_message = Some(msg.to_string());
            return Err(msg.to_string());
        }

        let parsed_args = match parse_arguments(self.arguments.trim()) {
            Some(args) => args,
            None => {
                let msg = "Arguments format is invalid (unclosed quotes or escapes).";
                self.error_message = Some(msg.to_string());
                return Err(msg.to_string());
            }
        };

        let opt_group = if self.group.trim().is_empty() {
            None
        } else {
            Some(self.group.trim().to_string())
        };

        let common = EntryCommon {
            name: trimmed_name.to_string(),
            executable_path: trimmed_exec.to_string(),
            arguments: parsed_args,
            save_paths: targets.iter().map(|t| t.path.clone()).collect(),
            save_targets: targets,
            group: opt_group,
            cloud_folder: self.cloud_folder.clone(),
            drive_file_id: self
                .initial_entry
                .as_ref()
                .and_then(|e| e.common().drive_file_id.clone()),
            steam_app_id: self
                .initial_entry
                .as_ref()
                .and_then(|e| e.common().steam_app_id),
        };

        let entry = match self.entry_type {
            EntryType::Emulator => {
                let trimmed_roms = self.roms_directory.trim();
                let ext_list: Vec<String> = self
                    .extensions
                    .split([',', ' '])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                if trimmed_roms.is_empty() || ext_list.is_empty() {
                    let msg = "Please specify the ROMs directory and at least one extension.";
                    self.error_message = Some(msg.to_string());
                    return Err(msg.to_string());
                }

                let opt_fulls = if self.fullscreen_args.trim().is_empty() {
                    None
                } else {
                    Some(self.fullscreen_args.trim().to_string())
                };

                let opt_covers = if self.covers_directory.trim().is_empty() {
                    None
                } else {
                    Some(self.covers_directory.trim().to_string())
                };

                let (
                    steam_process_name,
                    save_paths_by_rom,
                    save_targets_by_rom,
                    cover_path_by_rom,
                    title_by_rom,
                ) = match &self.initial_entry {
                    Some(GameEntry::Emulator(prev)) => (
                        prev.steam_process_name.clone(),
                        prev.save_paths_by_rom.clone(),
                        prev.save_targets_by_rom.clone(),
                        prev.cover_path_by_rom.clone(),
                        prev.title_by_rom.clone(),
                    ),
                    _ => (None, Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                };

                GameEntry::Emulator(Emulator {
                    common,
                    roms_directory: trimmed_roms.to_string(),
                    extensions: ext_list,
                    save_paths_by_rom,
                    save_targets_by_rom,
                    cover_path_by_rom,
                    title_by_rom,
                    steam_process_name,
                    covers_directory: opt_covers,
                    fullscreen_args: opt_fulls,
                })
            }
            EntryType::Native => {
                if common.save_targets.is_empty() {
                    let msg = "Native PC games require at least one save target path.";
                    self.error_message = Some(msg.to_string());
                    return Err(msg.to_string());
                }

                let opt_wait = if self.wait_for_process.trim().is_empty() {
                    None
                } else {
                    Some(self.wait_for_process.trim().to_string())
                };

                let opt_cover = if self.cover_path.trim().is_empty() {
                    None
                } else {
                    Some(self.cover_path.trim().to_string())
                };

                GameEntry::Native(NativeGame {
                    common,
                    wait_for_process: opt_wait,
                    cover_path: opt_cover,
                })
            }
        };

        self.error_message = None;
        Ok(entry)
    }
}

/// Distinct non-blank groups already used in the library, in library order.
///
/// Case-sensitive on purpose: Kotlin's dropdown uses `distinct()` (`EmuSyncApp.kt`), even
/// though the sidebar merges groups ignoring case.
pub fn existing_groups(entries: &[GameEntry]) -> Vec<String> {
    let mut groups: Vec<String> = Vec::new();
    for entry in entries {
        if let Some(group) = entry.common().group.as_deref().map(str::trim)
            && !group.is_empty()
            && !groups.iter().any(|g| g == group)
        {
            groups.push(group.to_string());
        }
    }
    groups
}
