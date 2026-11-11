//! Registers or unregisters Steam shortcuts and updates the entry's steam_app_id.

use super::edit_library::EntryEditResult;
use emusync_domain::config::ConfigurationStore;
use emusync_domain::library::{GameEntry, SteamLibrary};

pub struct SteamRegistration<S, St> {
    store: S,
    steam: St,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SteamRegistrationError<E1, E2> {
    Store(E1),
    Steam(E2),
    CategoryNotFound,
}

impl<E1: std::fmt::Display, E2: std::fmt::Display> std::fmt::Display
    for SteamRegistrationError<E1, E2>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(e) => write!(f, "Store error: {e}"),
            Self::Steam(e) => write!(f, "Steam error: {e}"),
            Self::CategoryNotFound => write!(f, "The category no longer exists."),
        }
    }
}

impl<E1: std::error::Error + 'static, E2: std::error::Error + 'static> std::error::Error
    for SteamRegistrationError<E1, E2>
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(e) => Some(e),
            Self::Steam(e) => Some(e),
            Self::CategoryNotFound => None,
        }
    }
}

impl<S: ConfigurationStore, St: SteamLibrary> SteamRegistration<S, St> {
    pub fn new(store: S, steam: St) -> Self {
        Self { store, steam }
    }

    pub fn register(
        &self,
        entry: &GameEntry,
    ) -> Result<EntryEditResult, SteamRegistrationError<S::Error, St::Error>> {
        self.change(entry, true)
    }

    pub fn unregister(
        &self,
        entry: &GameEntry,
    ) -> Result<EntryEditResult, SteamRegistrationError<S::Error, St::Error>> {
        self.change(entry, false)
    }

    fn change(
        &self,
        entry: &GameEntry,
        register: bool,
    ) -> Result<EntryEditResult, SteamRegistrationError<S::Error, St::Error>> {
        let current_config = self.store.load().map_err(SteamRegistrationError::Store)?;
        let latest = current_config
            .entries
            .iter()
            .find(|e| e.common().name == entry.common().name)
            .ok_or(SteamRegistrationError::CategoryNotFound)?;

        let app_id = if register {
            Some(
                self.steam
                    .register(latest)
                    .map_err(SteamRegistrationError::Steam)?,
            )
        } else {
            self.steam
                .unregister(latest)
                .map_err(SteamRegistrationError::Steam)?;
            None
        };

        let target_name = &entry.common().name;
        let mut edited_entry = None;
        let updated = self
            .store
            .update(|mut current| {
                if let Some(e) = current
                    .entries
                    .iter_mut()
                    .find(|e| &e.common().name == target_name)
                {
                    match e {
                        GameEntry::Emulator(emu) => emu.common.steam_app_id = app_id,
                        GameEntry::Native(nat) => nat.common.steam_app_id = app_id,
                    }
                    edited_entry = Some(e.clone());
                }
                current
            })
            .map_err(SteamRegistrationError::Store)?;

        let edited_entry = edited_entry.ok_or(SteamRegistrationError::CategoryNotFound)?;

        Ok(EntryEditResult {
            config: updated,
            entry: edited_entry,
        })
    }
}
