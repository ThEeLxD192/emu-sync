use emusync_application::{SteamRegistration, SteamRegistrationError};
use emusync_domain::config::{AppConfig, ConfigurationStore, Credentials};
use emusync_domain::library::{EntryCommon, GameEntry, NativeGame, SteamLibrary};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct MemoryStore {
    config: Arc<Mutex<AppConfig>>,
    update_calls: Arc<AtomicUsize>,
}

impl ConfigurationStore for MemoryStore {
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
        _updater: impl FnOnce(Option<Credentials>) -> Option<Credentials>,
    ) -> Result<AppConfig, Self::Error> {
        Ok(self.config.lock().unwrap().clone())
    }
}

type OnRegisterCallback = Box<dyn Fn() + Send + Sync>;

#[derive(Clone, Default)]
struct RecordingSteam {
    registered: Arc<Mutex<Vec<GameEntry>>>,
    unregistered: Arc<Mutex<Vec<GameEntry>>>,
    on_register: Arc<Mutex<Option<OnRegisterCallback>>>,
    should_fail: Arc<Mutex<bool>>,
}

impl SteamLibrary for RecordingSteam {
    type Error = String;

    fn is_available(&self) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn is_registered(&self, entry: &GameEntry) -> Result<bool, Self::Error> {
        Ok(entry.common().steam_app_id.is_some())
    }

    fn migrate_launchers(&self, _entries: &[GameEntry]) -> Result<Vec<(String, i32)>, Self::Error> {
        Ok(Vec::new())
    }

    fn register(&self, entry: &GameEntry) -> Result<i32, Self::Error> {
        if *self.should_fail.lock().unwrap() {
            return Err("Steam refused the shortcut".into());
        }
        self.registered.lock().unwrap().push(entry.clone());
        if let Some(cb) = self.on_register.lock().unwrap().as_ref() {
            cb();
        }
        Ok(42)
    }

    fn unregister(&self, entry: &GameEntry) -> Result<(), Self::Error> {
        if *self.should_fail.lock().unwrap() {
            return Err("Steam refused to unregister".into());
        }
        self.unregistered.lock().unwrap().push(entry.clone());
        Ok(())
    }
}

fn sample_native(name: &str, exe: &str, steam_app_id: Option<i32>) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: exe.to_string(),
            steam_app_id,
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    })
}

#[test]
fn registration_uses_latest_entry_and_retains_concurrent_edits() {
    let stale = sample_native("Game", "/games/old.exe", None);
    let latest = sample_native("Game", "/games/new.exe", None);
    let added = sample_native("AddedGame", "/games/added.exe", None);

    let store = MemoryStore {
        config: Arc::new(Mutex::new(AppConfig {
            language: Default::default(),
            credentials: None,
            entries: vec![latest.clone()],
        })),
        update_calls: Arc::new(AtomicUsize::new(0)),
    };

    let steam = RecordingSteam::default();

    // While Steam registers, simulate concurrent edits to the store
    let store_clone = store.clone();
    let added_clone = added.clone();
    *steam.on_register.lock().unwrap() = Some(Box::new(move || {
        store_clone
            .config
            .lock()
            .unwrap()
            .entries
            .push(added_clone.clone());
        store_clone.config.lock().unwrap().credentials = Some(Credentials {
            client_id: "client".into(),
            client_secret: "secret".into(),
            refresh_token: Some("new-refresh".into()),
        });
    }));

    let registration = SteamRegistration::new(store.clone(), steam.clone());
    let result = registration
        .register(&stale)
        .expect("registration should succeed");

    // 1. Steam received the latest entry, not the stale one
    let registered_entries = steam.registered.lock().unwrap().clone();
    assert_eq!(registered_entries.len(), 1);
    assert_eq!(
        registered_entries[0].common().executable_path,
        "/games/new.exe"
    );

    // 2. Result entry has the assigned app_id
    assert_eq!(result.entry.common().steam_app_id, Some(42));
    assert_eq!(result.entry.common().name, "Game");

    // 3. Concurrent edits (added game and credentials) were preserved
    assert_eq!(result.config.entries.len(), 2);
    assert_eq!(result.config.entries[0].common().steam_app_id, Some(42));
    assert_eq!(result.config.entries[1].common().name, "AddedGame");
    assert_eq!(
        result
            .config
            .credentials
            .as_ref()
            .and_then(|c| c.refresh_token.as_deref()),
        Some("new-refresh")
    );
}

#[test]
fn unregister_clears_only_target_app_id_leaving_others_intact() {
    let game1 = sample_native("Game1", "/games/g1.exe", Some(42));
    let game2 = sample_native("Game2", "/games/g2.exe", Some(99));

    let store = MemoryStore {
        config: Arc::new(Mutex::new(AppConfig {
            language: Default::default(),
            credentials: None,
            entries: vec![game1.clone(), game2.clone()],
        })),
        update_calls: Arc::new(AtomicUsize::new(0)),
    };

    let steam = RecordingSteam::default();
    let registration = SteamRegistration::new(store.clone(), steam.clone());

    let result = registration
        .unregister(&game1)
        .expect("unregister should succeed");

    let unregistered_entries = steam.unregistered.lock().unwrap().clone();
    assert_eq!(unregistered_entries.len(), 1);
    assert_eq!(unregistered_entries[0].common().name, "Game1");

    assert_eq!(result.entry.common().steam_app_id, None);
    assert_eq!(result.config.entries[0].common().steam_app_id, None);
    assert_eq!(result.config.entries[1].common().steam_app_id, Some(99));
}

#[test]
fn failed_steam_registration_does_not_mutate_store() {
    let game = sample_native("Game", "/games/g.exe", None);
    let store = MemoryStore {
        config: Arc::new(Mutex::new(AppConfig {
            language: Default::default(),
            credentials: None,
            entries: vec![game.clone()],
        })),
        update_calls: Arc::new(AtomicUsize::new(0)),
    };

    let steam = RecordingSteam::default();
    *steam.should_fail.lock().unwrap() = true;

    let registration = SteamRegistration::new(store.clone(), steam.clone());
    let err = registration.register(&game).unwrap_err();

    assert!(matches!(err, SteamRegistrationError::Steam(_)));
    assert_eq!(store.update_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        store.config.lock().unwrap().entries[0]
            .common()
            .steam_app_id,
        None
    );
}

#[test]
fn category_not_found_when_game_does_not_exist_in_store() {
    let game = sample_native("NonExistent", "/games/none.exe", None);
    let store = MemoryStore {
        config: Arc::new(Mutex::new(AppConfig {
            language: Default::default(),
            credentials: None,
            entries: Vec::new(),
        })),
        update_calls: Arc::new(AtomicUsize::new(0)),
    };

    let steam = RecordingSteam::default();
    let registration = SteamRegistration::new(store.clone(), steam.clone());

    let err = registration.register(&game).unwrap_err();
    assert_eq!(err, SteamRegistrationError::CategoryNotFound);
    assert_eq!(steam.registered.lock().unwrap().len(), 0);
    assert_eq!(store.update_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn concurrent_deletion_of_category_during_steam_register_returns_category_not_found() {
    let target_game = sample_native("TargetGame", "/games/target.exe", None);
    let survivor_game = sample_native("SurvivorGame", "/games/survivor.exe", Some(999));

    let store = MemoryStore {
        config: Arc::new(Mutex::new(AppConfig {
            language: Default::default(),
            credentials: None,
            entries: vec![target_game.clone(), survivor_game.clone()],
        })),
        update_calls: Arc::new(AtomicUsize::new(0)),
    };

    let steam = RecordingSteam::default();
    let store_clone = store.clone();

    // In the on_register callback (simulating race between load() and update()),
    // remove target_game from the store
    *steam.on_register.lock().unwrap() = Some(Box::new(move || {
        store_clone
            .config
            .lock()
            .unwrap()
            .entries
            .retain(|e| e.common().name != "TargetGame");
    }));

    let registration = SteamRegistration::new(store.clone(), steam.clone());
    let err = registration.register(&target_game).expect_err(
        "Registration must fail with CategoryNotFound when entry was deleted concurrently",
    );

    assert_eq!(err, SteamRegistrationError::CategoryNotFound);

    // Verify other entries in store remain uncorrupted
    let stored = store.config.lock().unwrap().clone();
    assert_eq!(stored.entries.len(), 1);
    assert_eq!(stored.entries[0].common().name, "SurvivorGame");
    assert_eq!(stored.entries[0].common().steam_app_id, Some(999));
}
