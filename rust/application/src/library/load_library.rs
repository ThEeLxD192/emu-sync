//! Loads library and synchronizes Steam shortcuts into persisted entries.

use emusync_domain::config::{AppConfig, ConfigurationStore};
use emusync_domain::library::{GameEntry, SteamLibrary};
use std::collections::HashMap;

pub struct LoadLibrary<S, St> {
    store: S,
    steam: St,
}

impl<S: ConfigurationStore, St: SteamLibrary> LoadLibrary<S, St> {
    pub fn new(store: S, steam: St) -> Self {
        Self { store, steam }
    }

    pub fn load(&self) -> Result<AppConfig, S::Error> {
        let config = self.store.load()?;
        let shortcuts = self
            .steam
            .migrate_launchers(&config.entries)
            .unwrap_or_default();
        let shortcut_map: HashMap<_, _> = shortcuts.into_iter().collect();
        let mut modified = false;
        let mut entries = config.entries.clone();
        for entry in &mut entries {
            let key = format!("[EmuSync] {}", entry.common().name);
            let app_id = shortcut_map.get(&key).copied();
            let common = match entry {
                GameEntry::Emulator(e) => &mut e.common,
                GameEntry::Native(n) => &mut n.common,
            };
            if common.steam_app_id != app_id {
                common.steam_app_id = app_id;
                modified = true;
            }
        }
        if modified {
            self.store.update(|c| AppConfig { entries, ..c })
        } else {
            Ok(config)
        }
    }
}
