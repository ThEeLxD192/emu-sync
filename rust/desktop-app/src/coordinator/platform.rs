use crate::{events::AppEvent, scanner_helper::scan_entry_games};
use emusync_application::library::{EntryEditResult, SteamRegistration};
use emusync_data_local::ConfigManager;
use emusync_domain::{
    execution::{BrowserLauncher, PathPicker, PickerType},
    library::{GameEntry, SteamLibrary},
};
use emusync_platform_desktop::{DesktopBrowserLauncher, DesktopSteamLibrary, FilePicker};
use emusync_presentation::model::{PathTarget, PickMode};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::Sender,
    },
};
use tokio::runtime::Handle;

#[derive(Clone)]
pub struct PlatformServices {
    pub config_manager: ConfigManager,
    pub steam: DesktopSteamLibrary,
    pub browser: Arc<DesktopBrowserLauncher>,
    pub picker: FilePicker,
    pub event_tx: Sender<AppEvent>,
    pub redraw: Arc<dyn Fn() + Send + Sync>,
    pub tokio_handle: Handle,
    pub scan_revision: Arc<AtomicU64>,
}

impl PlatformServices {
    pub fn toggle_steam(&self, entry: GameEntry) {
        let name = entry.common().name.clone();
        let is_registered = self
            .steam
            .is_registered(&entry)
            .unwrap_or(entry.common().steam_app_id.is_some());

        let reg = SteamRegistration::new(self.config_manager.clone(), self.steam.clone());
        let result = if is_registered {
            reg.unregister(&entry)
        } else {
            reg.register(&entry)
        };

        match result {
            Ok(EntryEditResult { config, .. }) => {
                let _ = self.event_tx.send(AppEvent::ConfigReloaded(config));
                let _ = self.event_tx.send(AppEvent::SteamRegistrationUpdated {
                    entry_name: name.clone(),
                    registered: !is_registered,
                });
                let msg = if !is_registered {
                    format!("Added '{name}' to Steam.")
                } else {
                    format!("Removed '{name}' from Steam.")
                };
                let _ = self.event_tx.send(AppEvent::Toast(msg));
                (self.redraw)();
            }
            Err(err) => {
                let _ = self
                    .event_tx
                    .send(AppEvent::Toast(format!("Steam operation failed: {err}")));
                (self.redraw)();
            }
        }
    }

    pub fn pick_path(&self, target: PathTarget, mode: PickMode) -> tokio::task::JoinHandle<()> {
        let picker = self.picker;
        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();

        let kind = match mode {
            PickMode::File => PickerType::File,
            PickMode::Directory => PickerType::Directory,
        };

        self.tokio_handle.spawn_blocking(move || {
            if let Ok(Some(path)) = picker.pick("Select path", "", kind) {
                let _ = tx.send(AppEvent::PathPicked { target, path });
                (redraw)();
            }
        })
    }

    pub fn open_config_dir(&self) {
        let config_path = self.config_manager.path();
        let dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        let _ = self.browser.open(&dir.to_string_lossy());
    }

    pub fn scan_entry_games_async(&self, entry: GameEntry) -> (u64, tokio::task::JoinHandle<()>) {
        let revision = self.scan_revision.fetch_add(1, Ordering::SeqCst) + 1;
        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();
        let entry_name = entry.common().name.clone();

        let handle = self.tokio_handle.spawn_blocking(move || {
            let games = scan_entry_games(&entry);
            let _ = tx.send(AppEvent::EntryGamesScanned {
                entry_name,
                games,
                revision,
            });
            (redraw)();
        });

        (revision, handle)
    }
}
