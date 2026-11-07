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

        if shortcuts.is_empty()
            && config
                .entries
                .iter()
                .all(|e| e.common().steam_app_id.is_none())
        {
            return Ok(config);
        }

        let shortcut_map: HashMap<&str, i32> = shortcuts
            .iter()
            .map(|(k, id)| (k.strip_prefix("[EmuSync] ").unwrap_or(k.as_str()), *id))
            .collect();

        let modified = config.entries.iter().any(|entry| {
            let app_id = shortcut_map.get(entry.common().name.as_str()).copied();
            entry.common().steam_app_id != app_id
        });

        if !modified {
            return Ok(config);
        }

        let mut entries = config.entries;
        for entry in &mut entries {
            let app_id = shortcut_map.get(entry.common().name.as_str()).copied();
            let common = match entry {
                GameEntry::Emulator(e) => &mut e.common,
                GameEntry::Native(n) => &mut n.common,
            };
            common.steam_app_id = app_id;
        }

        self.store.update(|c| AppConfig { entries, ..c })
    }
}
