use crate::{
    events::{AppEvent, EventSender},
    scanner_helper::scan_entry_games,
};
use emusync_application::library::game_specific_targets;
use emusync_application::library::{EntryEditResult, SteamRegistration};
use emusync_data_local::{ConfigManager, LocalSaveTargets};
use emusync_domain::{
    execution::{BrowserLauncher, PathPicker, PickerType},
    library::{GameEntry, SaveTarget, SaveTargetAccess, SaveTargetType, SteamLibrary},
};
use emusync_platform_desktop::{DesktopBrowserLauncher, DesktopSteamLibrary};
use emusync_presentation::i18n::fill;
use emusync_presentation::model::{GameItem, PathTarget, PickMode};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::runtime::Handle;

#[derive(Clone)]
pub struct PlatformServices {
    pub config_manager: ConfigManager,
    pub steam: DesktopSteamLibrary,
    pub browser: Arc<DesktopBrowserLauncher>,
    pub picker: Arc<dyn PathPicker<Error = std::io::Error> + Send + Sync>,
    pub event_tx: EventSender,
    pub redraw: Arc<dyn Fn() + Send + Sync>,
    pub tokio_handle: Handle,
    pub scan_revision: Arc<AtomicU64>,
    pub steam_revision: Arc<AtomicU64>,
    /// The interface language, for the messages these services send.
    pub language: crate::CurrentLanguage,
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
                let t = self.language.texts();
                let msg = if !is_registered {
                    fill(t.msg_steam_added, &[("name", &name)])
                } else {
                    fill(t.msg_steam_removed, &[("name", &name)])
                };
                let _ = self.event_tx.send(AppEvent::Toast(msg));
                (self.redraw)();
            }
            Err(err) => {
                let _ = self.event_tx.send(AppEvent::Toast(fill(
                    self.language.texts().msg_steam_failed,
                    &[("error", &err)],
                )));
                (self.redraw)();
            }
        }
    }

    pub fn pick_path(
        &self,
        request_id: u64,
        target: PathTarget,
        mode: PickMode,
        initial_path: String,
    ) -> tokio::task::JoinHandle<()> {
        let picker = self.picker.clone();
        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();

        let kind = match mode {
            PickMode::File => PickerType::File,
            PickMode::Directory => PickerType::Directory,
        };

        let t = self.language.texts();
        self.tokio_handle.spawn_blocking(move || {
            match picker.pick(t.select_path, &initial_path, kind) {
                Ok(Some(path)) => {
                    let _ = tx.send(AppEvent::PathPicked {
                        request_id,
                        target,
                        mode,
                        path,
                    });
                    (redraw)();
                }
                Ok(None) => {}
                Err(err) => {
                    let _ = tx.send(AppEvent::Toast(fill(
                        t.msg_file_chooser_failed,
                        &[("error", &err)],
                    )));
                    (redraw)();
                }
            }
        })
    }

    /// Reads the game's own save targets off the UI thread, then opens its dialog.
    pub fn load_game_edit(&self, item: GameItem) -> tokio::task::JoinHandle<()> {
        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();
        let t = self.language.texts();
        self.tokio_handle.spawn_blocking(move || {
            let rom_name = item.rom_file_name();
            let event =
                match game_specific_targets(&LocalSaveTargets, &item.entry, rom_name.as_deref()) {
                    Ok(targets) => AppEvent::GameEditReady {
                        item: Box::new(item),
                        targets,
                    },
                    Err(err) => {
                        AppEvent::Toast(fill(t.msg_save_paths_unreadable, &[("error", &err)]))
                    }
                };
            let _ = tx.send(event);
            (redraw)();
        })
    }

    /// Resolves restore paths off the UI thread with `for_restore`, which rejects names
    /// escaping the destination.
    pub fn preview_restore_paths(
        &self,
        request_id: u64,
        destination: String,
        names: Vec<String>,
    ) -> tokio::task::JoinHandle<()> {
        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();
        self.tokio_handle.spawn_blocking(move || {
            let paths = restore_targets(&destination, false, &names)
                .map(|targets| dedup_paths(&targets))
                .unwrap_or_default();
            let _ = tx.send(AppEvent::RestorePathsPreviewed {
                request_id,
                destination,
                paths,
            });
            (redraw)();
        })
    }

    /// Reports the real kind of a typed save path, if it exists.
    pub fn detect_save_target_type(
        &self,
        index: usize,
        path: String,
    ) -> tokio::task::JoinHandle<()> {
        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();
        self.tokio_handle.spawn_blocking(move || {
            if let Ok(Some(kind)) = LocalSaveTargets.type_of(path.trim()) {
                let _ = tx.send(AppEvent::SaveTargetTypeDetected { index, path, kind });
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

    /// Re-reads Steam availability and registrations off the UI thread.
    pub fn refresh_steam_state_async(
        &self,
        entries: Vec<GameEntry>,
    ) -> tokio::task::JoinHandle<()> {
        let revision = self.steam_revision.fetch_add(1, Ordering::SeqCst) + 1;
        let steam = self.steam.clone();
        let tx = self.event_tx.clone();
        let redraw = self.redraw.clone();

        self.tokio_handle.spawn_blocking(move || {
            let available = steam.is_available().unwrap_or(false);
            let registered = entries
                .iter()
                .filter(|entry| {
                    steam
                        .is_registered(entry)
                        .unwrap_or(entry.common().steam_app_id.is_some())
                })
                .map(|entry| entry.common().name.clone())
                .collect();
            let _ = tx.send(AppEvent::SteamStateRefreshed {
                available,
                registered,
                revision,
            });
            (redraw)();
        })
    }
}

/// Restore targets for a cloud restore, as Kotlin's `GameEditFormState.result`: the whole
/// folder, or each cloud file resolved inside it by `for_restore`.
pub fn restore_targets(
    destination: &str,
    whole_folder: bool,
    names: &[String],
) -> std::io::Result<Vec<SaveTarget>> {
    let destination = destination.trim();
    if destination.is_empty() {
        return Ok(Vec::new());
    }
    if whole_folder {
        return Ok(vec![SaveTarget {
            path: destination.to_string(),
            kind: SaveTargetType::Directory,
            cloud_path: None,
            excluded_folders: Vec::new(),
        }]);
    }
    LocalSaveTargets.for_restore(destination, names)
}

fn dedup_paths(targets: &[SaveTarget]) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for target in targets {
        if !paths.contains(&target.path) {
            paths.push(target.path.clone());
        }
    }
    paths
}
