//! Preservation of shared cloud folders and game-specific override building.

use emusync_domain::config::AppConfig;
use emusync_domain::library::{Emulator, GameEntry, SaveTarget};
use emusync_domain::sync::SyncGame;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameOverrideChanges {
    pub targets: Vec<SaveTarget>,
    pub cover_path: Option<String>,
    pub title: Option<String>,
}

pub fn preserve_shared_cloud_root(
    config: &AppConfig,
    old_entry: &GameEntry,
    new_entry: &mut GameEntry,
) {
    let source_uses_root = match old_entry {
        GameEntry::Emulator(_) => true,
        GameEntry::Native(n) => n.common.group.is_none() || n.common.cloud_folder.is_some(),
    };
    let new_cloud_folder_blank = new_entry
        .common()
        .cloud_folder
        .as_deref()
        .is_none_or(|s| s.trim().is_empty());
    if !source_uses_root || !new_cloud_folder_blank {
        return;
    }
    let old_effective = old_entry.common().effective_cloud_folder();
    let new_effective = new_entry.common().effective_cloud_folder();
    let shared_root_changed = old_effective != new_effective
        && config
            .entries
            .iter()
            .filter(|e| e.common().name != old_entry.common().name)
            .any(|e| e.common().effective_cloud_folder() == old_effective);
    if shared_root_changed {
        match new_entry {
            GameEntry::Emulator(e) => e.common.cloud_folder = Some(old_effective.to_string()),
            GameEntry::Native(n) => n.common.cloud_folder = Some(old_effective.to_string()),
        }
    }
}

pub fn build_game_entry(game: &SyncGame, changes: &GameOverrideChanges) -> Option<GameEntry> {
    match &game.entry {
        GameEntry::Emulator(emu) => {
            let rom_name = game
                .rom_path
                .as_deref()
                .and_then(|p| {
                    p.rsplit(['/', '\\'])
                        .next()
                        .filter(|name| !name.trim().is_empty())
                })
                .unwrap_or(game.name.as_str());
            Some(GameEntry::Emulator(update_emulator_overrides(
                emu, rom_name, changes,
            )))
        }
        GameEntry::Native(native) => {
            let mut updated = native.clone();
            if let Some(title) = changes
                .title
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                updated.common.name = title.to_string();
            }
            if let Some(p) = &changes.cover_path {
                let clean = p.trim();
                updated.cover_path = if clean.is_empty() {
                    None
                } else {
                    Some(clean.to_string())
                };
            }
            updated.common.save_paths = changes.targets.iter().map(|t| t.path.clone()).collect();
            updated.common.save_targets = changes.targets.clone();
            Some(GameEntry::Native(updated))
        }
    }
}

fn update_emulator_overrides(
    emu: &Emulator,
    rom_name: &str,
    changes: &GameOverrideChanges,
) -> Emulator {
    let mut updated = emu.clone();

    let has_valid_targets = changes.targets.iter().any(|t| !t.path.trim().is_empty());

    if !has_valid_targets {
        updated.save_paths_by_rom.retain(|(k, _)| k != rom_name);
        updated.save_targets_by_rom.retain(|(k, _)| k != rom_name);
    } else {
        let mut valid_paths = Vec::with_capacity(changes.targets.len());
        let mut valid_targets = Vec::with_capacity(changes.targets.len());
        for t in &changes.targets {
            let trimmed = t.path.trim();
            if !trimmed.is_empty() {
                valid_paths.push(trimmed.to_string());
                valid_targets.push(t.clone());
            }
        }
        if let Some(entry) = updated
            .save_paths_by_rom
            .iter_mut()
            .find(|(k, _)| k == rom_name)
        {
            entry.1 = valid_paths;
        } else {
            updated
                .save_paths_by_rom
                .push((rom_name.to_string(), valid_paths));
        }
        if let Some(entry) = updated
            .save_targets_by_rom
            .iter_mut()
            .find(|(k, _)| k == rom_name)
        {
            entry.1 = valid_targets;
        } else {
            updated
                .save_targets_by_rom
                .push((rom_name.to_string(), valid_targets));
        }
    }

    apply_optional_override(
        &mut updated.cover_path_by_rom,
        rom_name,
        changes.cover_path.as_deref(),
    );
    apply_optional_override(
        &mut updated.title_by_rom,
        rom_name,
        changes.title.as_deref(),
    );

    updated
}

fn apply_optional_override(map: &mut Vec<(String, String)>, key: &str, value: Option<&str>) {
    if let Some(val) = value {
        let clean = val.trim();
        if clean.is_empty() {
            map.retain(|(k, _)| k != key);
        } else if let Some(entry) = map.iter_mut().find(|(k, _)| k == key) {
            entry.1 = clean.to_string();
        } else {
            map.push((key.to_string(), clean.to_string()));
        }
    }
}
