//! Preservation of shared cloud folders and game-specific override building.

use emusync_domain::config::AppConfig;
use emusync_domain::library::{Emulator, GameEntry, NativeGame, SaveTarget};
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
            let title = changes
                .title
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(&native.common.name);
            let cover_path = match &changes.cover_path {
                Some(p) => {
                    let clean = p.trim();
                    if clean.is_empty() {
                        None
                    } else {
                        Some(clean.to_string())
                    }
                }
                None => native.cover_path.clone(),
            };
            Some(GameEntry::Native(NativeGame {
                common: emusync_domain::library::EntryCommon {
                    name: title.to_string(),
                    save_paths: changes.targets.iter().map(|t| t.path.clone()).collect(),
                    save_targets: changes.targets.clone(),
                    ..native.common.clone()
                },
                cover_path,
                ..native.clone()
            }))
        }
    }
}

fn update_emulator_overrides(
    emu: &Emulator,
    rom_name: &str,
    changes: &GameOverrideChanges,
) -> Emulator {
    let mut save_paths_by_rom = emu.save_paths_by_rom.clone();
    let mut save_targets_by_rom = emu.save_targets_by_rom.clone();
    let mut cover_path_by_rom = emu.cover_path_by_rom.clone();
    let mut title_by_rom = emu.title_by_rom.clone();

    let valid_paths: Vec<_> = changes
        .targets
        .iter()
        .map(|t| t.path.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();

    if valid_paths.is_empty() {
        save_paths_by_rom.retain(|(k, _)| k != rom_name);
        save_targets_by_rom.retain(|(k, _)| k != rom_name);
    } else {
        if let Some(entry) = save_paths_by_rom.iter_mut().find(|(k, _)| k == rom_name) {
            entry.1 = valid_paths;
        } else {
            save_paths_by_rom.push((rom_name.to_string(), valid_paths));
        }
        let valid_targets: Vec<_> = changes
            .targets
            .iter()
            .filter(|t| !t.path.trim().is_empty())
            .cloned()
            .collect();
        if let Some(entry) = save_targets_by_rom.iter_mut().find(|(k, _)| k == rom_name) {
            entry.1 = valid_targets;
        } else {
            save_targets_by_rom.push((rom_name.to_string(), valid_targets));
        }
    }

    apply_optional_override(
        &mut cover_path_by_rom,
        rom_name,
        changes.cover_path.as_deref(),
    );
    apply_optional_override(&mut title_by_rom, rom_name, changes.title.as_deref());

    Emulator {
        save_paths_by_rom,
        save_targets_by_rom,
        cover_path_by_rom,
        title_by_rom,
        ..emu.clone()
    }
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
