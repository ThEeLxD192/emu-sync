//! Library mutations (reordering, adding, deleting, and editing entries) with cloud synchronization.

use super::entry_edits::{
    GameOverrideChanges, build_game_entry, preserve_shared_cloud_root, resolve_rom_name,
};
use emusync_domain::config::{AppConfig, ConfigurationStore};
use emusync_domain::library::{GameEntry, SteamLibrary};
use emusync_domain::sync::{
    SaveSynchronizer, SaveSynchronizerFactory, SyncEvent, SyncGame, SyncMessage,
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryEditResult {
    pub config: AppConfig,
    pub entry: GameEntry,
}

pub struct EditLibrary<S, F, St> {
    store: S,
    synchronizers: F,
    steam: St,
    on_status: Arc<dyn Fn(SyncEvent) + Send + Sync + 'static>,
}

impl<S, F, St> EditLibrary<S, F, St>
where
    S: ConfigurationStore,
    F: SaveSynchronizerFactory,
    St: SteamLibrary,
{
    pub fn new(
        store: S,
        synchronizers: F,
        steam: St,
        on_status: impl Fn(SyncEvent) + Send + Sync + 'static,
    ) -> Self {
        Self {
            store,
            synchronizers,
            steam,
            on_status: Arc::new(on_status),
        }
    }

    pub fn reorder_entries(
        &self,
        config: &AppConfig,
        from_index: usize,
        to_index: usize,
    ) -> Result<AppConfig, S::Error> {
        let len = config.entries.len();
        if from_index >= len || to_index >= len || from_index == to_index {
            return Ok(config.clone());
        }
        let mut entries = config.entries.clone();
        let item = entries.remove(from_index);
        entries.insert(to_index, item);
        self.reorder_entries_list(&entries)
    }

    pub fn reorder_entries_list(&self, new_entries: &[GameEntry]) -> Result<AppConfig, S::Error> {
        self.store.update(|mut current| {
            let mut ordered = Vec::with_capacity(current.entries.len());
            for target in new_entries {
                let target_name = &target.common().name;
                if let Some(pos) = current
                    .entries
                    .iter()
                    .position(|e| &e.common().name == target_name)
                {
                    ordered.push(current.entries.remove(pos));
                }
            }
            ordered.append(&mut current.entries);
            current.entries = ordered;
            current
        })
    }

    pub fn add_entry(
        &self,
        config: &AppConfig,
        entry: GameEntry,
    ) -> Result<Option<AppConfig>, S::Error> {
        if entry.common().name.trim().is_empty()
            || config
                .entries
                .iter()
                .any(|e| e.common().name == entry.common().name)
        {
            (self.on_status)(SyncEvent::Error(SyncMessage::CategoryNameInvalid));
            return Ok(None);
        }
        let updated = self.store.update(|mut current| {
            if !current
                .entries
                .iter()
                .any(|e| e.common().name == entry.common().name)
            {
                current.entries.push(entry);
            }
            current
        })?;
        Ok(Some(updated))
    }

    pub fn delete_entry(&self, entry: &GameEntry) -> Result<AppConfig, S::Error> {
        let target_name = &entry.common().name;
        self.store.update(|mut current| {
            current.entries.retain(|e| &e.common().name != target_name);
            current
        })
    }

    pub async fn edit_entry(
        &self,
        config: &AppConfig,
        old_entry: &GameEntry,
        new_entry: &GameEntry,
    ) -> Result<Option<EntryEditResult>, S::Error> {
        if !self.validate_edit(config, old_entry, new_entry) {
            return Ok(None);
        }
        let mut edited = new_entry.clone();
        preserve_shared_cloud_root(config, old_entry, &mut edited);

        let migrated = if config.credentials.is_some() {
            let on_status = Arc::clone(&self.on_status);
            let sync = self
                .synchronizers
                .create(config.clone(), move |ev| on_status(ev));
            sync.rename_entry(old_entry, &edited).await.unwrap_or(false)
        } else {
            true
        };

        if !migrated {
            return Ok(None);
        }

        let old_name = &old_entry.common().name;
        let updated = self.store.update(|mut current| {
            if let Some(pos) = current
                .entries
                .iter()
                .position(|e| &e.common().name == old_name)
            {
                current.entries[pos] = edited.clone();
            }
            current
        })?;

        if edited.common().steam_app_id.is_some() {
            let _ = self.steam.register(&edited);
        }

        Ok(Some(EntryEditResult {
            config: updated,
            entry: edited,
        }))
    }

    pub async fn edit_game_override(
        &self,
        config: &AppConfig,
        game: &SyncGame,
        changes: &GameOverrideChanges,
    ) -> Result<Option<EntryEditResult>, S::Error> {
        let Some(latest) = config
            .entries
            .iter()
            .find(|e| e.common().name == game.entry.common().name)
        else {
            return Ok(None);
        };
        let current_game = SyncGame {
            entry: latest.clone(),
            ..game.clone()
        };
        let edited = build_game_entry(&current_game, changes);

        let migrated = self
            .migrate_game_title(config, &current_game, &edited, changes)
            .await;
        if migrated {
            self.edit_entry(config, &current_game.entry, &edited).await
        } else {
            Ok(None)
        }
    }

    fn validate_edit(
        &self,
        config: &AppConfig,
        old_entry: &GameEntry,
        new_entry: &GameEntry,
    ) -> bool {
        if config
            .entries
            .iter()
            .all(|e| e.common().name != old_entry.common().name)
        {
            return false;
        }
        if new_entry.common().name.trim().is_empty()
            || config.entries.iter().any(|e| {
                e.common().name != old_entry.common().name
                    && e.common().name == new_entry.common().name
            })
        {
            (self.on_status)(SyncEvent::Error(SyncMessage::CategoryNameInvalid));
            return false;
        }
        true
    }

    async fn migrate_game_title(
        &self,
        config: &AppConfig,
        current: &SyncGame,
        edited: &GameEntry,
        changes: &GameOverrideChanges,
    ) -> bool {
        let old_title = &current.effective_title;
        let rom_name = resolve_rom_name(current);
        let new_title = match edited {
            GameEntry::Emulator(emu) => emu.effective_title(rom_name, &current.name),
            GameEntry::Native(nat) => nat.common.name.clone(),
        };

        if new_title == *old_title
            || !matches!(edited, GameEntry::Emulator(_))
            || config.credentials.is_none()
        {
            return true;
        }

        let local_targets = if changes.targets.is_empty() {
            &current.save_targets
        } else {
            &changes.targets
        };
        let on_status = Arc::clone(&self.on_status);
        let sync = self
            .synchronizers
            .create(config.clone(), move |ev| on_status(ev));
        sync.rename_game(edited, Some(rom_name), old_title, &new_title, local_targets)
            .await
            .unwrap_or(false)
    }
}
