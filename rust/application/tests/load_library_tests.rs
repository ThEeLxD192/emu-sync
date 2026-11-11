use emusync_application::LoadLibrary;
use emusync_domain::config::{AppConfig, ConfigurationStore};
use emusync_domain::library::{Emulator, EntryCommon, GameEntry, NativeGame, SteamLibrary};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct SpyStore {
    config: Arc<Mutex<AppConfig>>,
    update_calls: Arc<AtomicUsize>,
}

impl ConfigurationStore for SpyStore {
    type Error = String;

    fn load(&self) -> Result<AppConfig, Self::Error> {
        Ok(self.config.lock().unwrap().clone())
    }

    fn update(
        &self,
        transform: impl FnOnce(AppConfig) -> AppConfig,
    ) -> Result<AppConfig, Self::Error> {
        self.update_calls.fetch_add(1, Ordering::SeqCst);
        let mut lock = self.config.lock().unwrap();
        *lock = transform(lock.clone());
        Ok(lock.clone())
    }

    fn update_credentials(
        &self,
        _updater: impl FnOnce(
            Option<emusync_domain::config::Credentials>,
        ) -> Option<emusync_domain::config::Credentials>,
    ) -> Result<AppConfig, Self::Error> {
        Ok(self.config.lock().unwrap().clone())
    }
}

type ShortcutsResult = Result<Vec<(String, i32)>, String>;

#[derive(Clone)]
struct MockSteam {
    shortcuts: Arc<Mutex<ShortcutsResult>>,
}

impl SteamLibrary for MockSteam {
    type Error = String;

    fn is_available(&self) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn is_registered(&self, _entry: &GameEntry) -> Result<bool, Self::Error> {
        Ok(false)
    }

    fn migrate_launchers(&self, _entries: &[GameEntry]) -> Result<Vec<(String, i32)>, Self::Error> {
        self.shortcuts.lock().unwrap().clone()
    }

    fn register(&self, _entry: &GameEntry) -> Result<i32, Self::Error> {
        Ok(0)
    }

    fn unregister(&self, _entry: &GameEntry) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn sample_native(name: &str, steam_app_id: Option<i32>) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: format!("/games/{name}"),
            steam_app_id,
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    })
}

fn sample_emulator(name: &str, steam_app_id: Option<i32>) -> GameEntry {
    let mut emu = Emulator::new(name, format!("/bin/{name}"), "/roms", vec!["iso".into()]);
    emu.common.steam_app_id = steam_app_id;
    GameEntry::Emulator(emu)
}

#[test]
fn initial_load_synchronizes_steam_app_ids_for_native_and_emulator() {
    let g1 = sample_native("Hollow Knight", None);
    let g2 = sample_emulator("PCSX2", None);

    let store = SpyStore {
        config: Arc::new(Mutex::new(AppConfig {
            credentials: None,
            entries: vec![g1, g2],
        })),
        update_calls: Arc::new(AtomicUsize::new(0)),
    };

    let steam = MockSteam {
        shortcuts: Arc::new(Mutex::new(Ok(vec![
            ("[EmuSync] Hollow Knight".into(), 1001),
            ("[EmuSync] PCSX2".into(), 1002),
        ]))),
    };

    let loader = LoadLibrary::new(store.clone(), steam);
    let loaded = loader.load().expect("load should succeed");

    assert_eq!(loaded.entries[0].common().steam_app_id, Some(1001));
    assert_eq!(loaded.entries[1].common().steam_app_id, Some(1002));
    assert_eq!(store.update_calls.load(Ordering::SeqCst), 1);

    // Persisted store was updated
    let persisted = store.load().unwrap();
    assert_eq!(persisted.entries[0].common().steam_app_id, Some(1001));
    assert_eq!(persisted.entries[1].common().steam_app_id, Some(1002));
}

#[test]
fn idempotent_load_does_not_call_store_update() {
    let g1 = sample_native("Hollow Knight", Some(1001));
    let g2 = sample_emulator("PCSX2", Some(1002));

    let store = SpyStore {
        config: Arc::new(Mutex::new(AppConfig {
            credentials: None,
            entries: vec![g1, g2],
        })),
        update_calls: Arc::new(AtomicUsize::new(0)),
    };

    let steam = MockSteam {
        shortcuts: Arc::new(Mutex::new(Ok(vec![
            ("[EmuSync] Hollow Knight".into(), 1001),
            ("[EmuSync] PCSX2".into(), 1002),
        ]))),
    };

    let loader = LoadLibrary::new(store.clone(), steam);
    let loaded = loader.load().expect("load should succeed");

    assert_eq!(loaded.entries[0].common().steam_app_id, Some(1001));
    assert_eq!(loaded.entries[1].common().steam_app_id, Some(1002));
    // Fast path: no store update performed
    assert_eq!(store.update_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn removed_steam_shortcuts_clears_app_ids_in_store() {
    let g1 = sample_native("Hollow Knight", Some(1001));

    let store = SpyStore {
        config: Arc::new(Mutex::new(AppConfig {
            credentials: None,
            entries: vec![g1],
        })),
        update_calls: Arc::new(AtomicUsize::new(0)),
    };

    let steam = MockSteam {
        shortcuts: Arc::new(Mutex::new(Ok(Vec::new()))),
    };

    let loader = LoadLibrary::new(store.clone(), steam);
    let loaded = loader.load().expect("load should succeed");

    assert_eq!(loaded.entries[0].common().steam_app_id, None);
    assert_eq!(store.update_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn empty_shortcuts_with_no_assigned_ids_short_circuits_instantly() {
    let g1 = sample_native("Game Without Shortcut", None);

    let store = SpyStore {
        config: Arc::new(Mutex::new(AppConfig {
            credentials: None,
            entries: vec![g1],
        })),
        update_calls: Arc::new(AtomicUsize::new(0)),
    };

    let steam = MockSteam {
        shortcuts: Arc::new(Mutex::new(Ok(Vec::new()))),
    };

    let loader = LoadLibrary::new(store.clone(), steam);
    let loaded = loader.load().expect("load should succeed");

    assert_eq!(loaded.entries[0].common().steam_app_id, None);
    assert_eq!(store.update_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn steam_error_is_handled_gracefully() {
    let g1 = sample_native("Game", Some(999));

    let store = SpyStore {
        config: Arc::new(Mutex::new(AppConfig {
            credentials: None,
            entries: vec![g1],
        })),
        update_calls: Arc::new(AtomicUsize::new(0)),
    };

    let steam = MockSteam {
        shortcuts: Arc::new(Mutex::new(Err("Steam VDF parsing failed".into()))),
    };

    let loader = LoadLibrary::new(store.clone(), steam);
    let loaded = loader.load().expect("load should not fail on steam error");

    // A Steam failure must not erase persisted IDs: the config comes back untouched.
    assert_eq!(loaded.entries[0].common().steam_app_id, Some(999));
    assert_eq!(store.update_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn steam_migration_error_keeps_persisted_steam_app_ids() {
    let store = SpyStore {
        config: Arc::new(Mutex::new(AppConfig {
            credentials: None,
            entries: vec![
                sample_native("Hollow Knight", Some(1001)),
                sample_emulator("PCSX2", Some(1002)),
            ],
        })),
        update_calls: Arc::new(AtomicUsize::new(0)),
    };
    let steam = MockSteam {
        shortcuts: Arc::new(Mutex::new(Err("shortcuts.vdf is corrupt".into()))),
    };

    let loaded = LoadLibrary::new(store.clone(), steam).load().unwrap();

    assert_eq!(loaded.entries[0].common().steam_app_id, Some(1001));
    assert_eq!(loaded.entries[1].common().steam_app_id, Some(1002));
    assert_eq!(store.update_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        store.load().unwrap().entries[0].common().steam_app_id,
        Some(1001)
    );
}
