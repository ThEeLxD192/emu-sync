//! ROM scanning and GameItem resolution helper for desktop application.

use emusync_domain::library::{CoverFiles, GameEntry};
use emusync_platform_desktop::{DesktopCoverFiles, scan_roms};
use emusync_presentation::model::GameItem;
use std::path::Path;

/// Scans games for a single library entry.
pub fn scan_entry_games(entry: &GameEntry) -> Vec<GameItem> {
    let cover_finder = DesktopCoverFiles;
    match entry {
        GameEntry::Native(native) => {
            let cover = cover_finder
                .find(&native.common.name, entry, None)
                .ok()
                .flatten();
            vec![
                GameItem::new(native.common.name.clone(), entry.clone(), None)
                    .with_cover_path(cover),
            ]
        }
        GameEntry::Emulator(emulator) => {
            let roms_dir = emulator.roms_directory.trim();
            if roms_dir.is_empty() || !Path::new(roms_dir).is_dir() {
                return Vec::new();
            }
            let files = scan_roms(Path::new(roms_dir), &emulator.extensions);
            files
                .into_iter()
                .map(|rom| {
                    let rom_path_str = rom.file.to_string_lossy();
                    let cover = cover_finder
                        .find(&rom.name, entry, Some(&rom_path_str))
                        .ok()
                        .flatten();
                    GameItem::new(rom.name, entry.clone(), Some(rom.file)).with_cover_path(cover)
                })
                .collect()
        }
    }
}
