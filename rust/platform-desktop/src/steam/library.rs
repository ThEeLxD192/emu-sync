use crate::steam::manager::SteamShortcutManager;
use emusync_domain::{
    execution::SteamLaunchPreparation,
    library::{Emulator, GameEntry, SteamLibrary},
};
use std::{collections::HashMap, io, path::Path};

#[derive(Clone, Debug, Default)]
pub struct DesktopSteamLibrary {
    manager: SteamShortcutManager,
}

impl DesktopSteamLibrary {
    pub fn new(manager: SteamShortcutManager) -> Self {
        Self { manager }
    }

    pub fn manager(&self) -> &SteamShortcutManager {
        &self.manager
    }
}

impl SteamLibrary for DesktopSteamLibrary {
    type Error = io::Error;

    fn is_available(&self) -> Result<bool, Self::Error> {
        Ok(self.manager.find_shortcuts_file().is_some())
    }

    fn is_registered(&self, entry: &GameEntry) -> Result<bool, Self::Error> {
        Ok(self.manager.is_registered(entry))
    }

    fn migrate_launchers(&self, entries: &[GameEntry]) -> Result<Vec<(String, i32)>, Self::Error> {
        let updated = self.manager.migrate_legacy_launchers(entries)?;
        let mut grouped: HashMap<String, i32> = HashMap::new();
        for s in updated {
            grouped.entry(s.app_name).or_insert(s.app_id);
        }
        Ok(grouped.into_iter().collect())
    }

    fn register(&self, entry: &GameEntry) -> Result<i32, Self::Error> {
        let shortcut = self.manager.register_entry(entry)?;
        Ok(shortcut.app_id)
    }

    fn unregister(&self, entry: &GameEntry) -> Result<(), Self::Error> {
        self.manager.unregister_entry(entry)
    }
}

impl SteamLaunchPreparation for DesktopSteamLibrary {
    type Error = io::Error;

    fn prepare(&self, entry: &Emulator, rom_path: &str) -> Result<(), Self::Error> {
        self.manager.prepare_launch(entry, Path::new(rom_path))
    }
}
