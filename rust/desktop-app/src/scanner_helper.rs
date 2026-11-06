//! ROM scanning and GameItem resolution helper for desktop application.

use emusync_domain::library::GameEntry;
use emusync_platform_desktop::scan_roms;
use emusync_presentation::model::GameItem;
use std::path::Path;

/// Scans games for a single library entry.
pub fn scan_entry_games(entry: &GameEntry) -> Vec<GameItem> {
    match entry {
        GameEntry::Native(native) => {
            vec![GameItem::new(
                native.common.name.clone(),
                entry.clone(),
                None,
            )]
        }
        GameEntry::Emulator(emulator) => {
            let roms_dir = emulator.roms_directory.trim();
            if roms_dir.is_empty() || !Path::new(roms_dir).is_dir() {
                return Vec::new();
            }
            let files = scan_roms(Path::new(roms_dir), &emulator.extensions);
            files
                .into_iter()
                .map(|rom| GameItem::new(rom.name, entry.clone(), Some(rom.file)))
                .collect()
        }
    }
}
