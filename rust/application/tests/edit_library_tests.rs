use emusync_application::{
    EditLibrary, GameOverrideChanges, SteamRegistration, build_game_entry,
    library::preserve_shared_cloud_root,
};
use emusync_domain::config::{AppConfig, ConfigurationStore, Credentials};
use emusync_domain::library::{
    Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType, SteamLibrary,
};
use emusync_domain::sync::{
    Decision, EntrySyncStatus, PostSyncFailure, SaveConflict, SaveSetup, SaveSynchronizer,
    SaveSynchronizerFactory, SyncEvent, SyncGame,
};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct InMemoryStore {
    config: Arc<Mutex<AppConfig>>,
}

impl ConfigurationStore for InMemoryStore {
    type Error = String;

    fn load(&self) -> Result<AppConfig, Self::Error> {
        Ok(self.config.lock().unwrap().clone())
    }

    fn update(
        &self,
        transform: impl FnOnce(AppConfig) -> AppConfig,
    ) -> Result<AppConfig, Self::Error> {
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

#[derive(Clone, Default)]
struct MockSteam {
    registered_names: Arc<Mutex<HashSet<String>>>,
}

impl SteamLibrary for MockSteam {
    type Error = String;

    fn is_available(&self) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn is_registered(&self, entry: &GameEntry) -> Result<bool, Self::Error> {
        Ok(self
            .registered_names
            .lock()
            .unwrap()
            .contains(&entry.common().name))
    }

    fn migrate_launchers(&self, _entries: &[GameEntry]) -> Result<Vec<(String, i32)>, Self::Error> {
        Ok(Vec::new())
    }

    fn register(&self, entry: &GameEntry) -> Result<i32, Self::Error> {
        self.registered_names
            .lock()
            .unwrap()
            .insert(entry.common().name.clone());
        Ok(12345)
    }

    fn unregister(&self, entry: &GameEntry) -> Result<(), Self::Error> {
        self.registered_names
            .lock()
            .unwrap()
            .remove(&entry.common().name);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct SpySynchronizer {
    renamed_entries: Arc<Mutex<Vec<(GameEntry, GameEntry)>>>,
    renamed_games: Arc<Mutex<Vec<(String, String)>>>,
    rename_entry_result: Arc<Mutex<Option<Result<bool, String>>>>,
    rename_game_result: Arc<Mutex<Option<Result<bool, String>>>>,
}

impl SaveSynchronizer for SpySynchronizer {
    type Error = String;

    async fn check_entry(&self, _entry: &GameEntry) -> Result<EntrySyncStatus, Self::Error> {
        Ok(EntrySyncStatus::InSync)
    }

    async fn sync_entry(&self, _entry: &GameEntry) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn rename_game(
        &self,
        _entry: &GameEntry,
        _rom_name: Option<&str>,
        old_title: &str,
        new_title: &str,
        _local_save_targets: &[SaveTarget],
    ) -> Result<bool, Self::Error> {
        self.renamed_games
            .lock()
            .unwrap()
            .push((old_title.to_string(), new_title.to_string()));
        if let Some(res) = self.rename_game_result.lock().unwrap().clone() {
            return res;
        }
        Ok(true)
    }

    async fn rename_entry(&self, old: &GameEntry, new: &GameEntry) -> Result<bool, Self::Error> {
        self.renamed_entries
            .lock()
            .unwrap()
            .push((old.clone(), new.clone()));
        if let Some(res) = self.rename_entry_result.lock().unwrap().clone() {
            return res;
        }
        Ok(true)
    }

    async fn play_with_sync(
        &self,
        _game: SyncGame,
        _setup: &(impl SaveSetup + Sync),
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn resolve_conflict(&self, _request: &SaveConflict, _decision: Decision) {}

    async fn retry_post_sync(&self, _failure: &PostSyncFailure) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct SpyFactory {
    synchronizer: SpySynchronizer,
    status_emitter: Arc<Mutex<Option<SyncEvent>>>,
}

impl SaveSynchronizerFactory for SpyFactory {
    type Synchronizer = SpySynchronizer;

    fn create(
        &self,
        _config: AppConfig,
        on_status: impl Fn(SyncEvent) + Send + Sync + 'static,
    ) -> Self::Synchronizer {
        if let Some(ev) = self.status_emitter.lock().unwrap().take() {
            on_status(ev);
        }
        self.synchronizer.clone()
    }
}

fn make_native(name: &str) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: format!("/games/{name}"),
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    })
}

#[tokio::test]
async fn reordering_and_mutations_preserve_library_invariants() {
    let e1 = make_native("Alpha");
    let e2 = make_native("Beta");
    let e3 = make_native("Gamma");

    let initial = AppConfig {
        credentials: None,
        entries: vec![e1.clone(), e2.clone(), e3.clone()],
    };
    let store = InMemoryStore {
        config: Arc::new(Mutex::new(initial)),
    };
    let steam = MockSteam::default();
    let factory = SpyFactory::default();

    let editor = EditLibrary::new(store.clone(), factory.clone(), steam.clone(), |_| {});

    // 1. Reorder: request [Gamma, Alpha] -> should be [Gamma, Alpha, Beta] preserving unlisted Beta
    let reordered = editor
        .reorder_entries_list(&[e3.clone(), e1.clone()])
        .unwrap();
    let names: Vec<_> = reordered
        .entries
        .iter()
        .map(|e| e.common().name.as_str())
        .collect();
    assert_eq!(names, vec!["Gamma", "Alpha", "Beta"]);

    // 2. Add entry
    let e4 = make_native("Delta");
    let added = editor
        .add_entry(&reordered, e4)
        .unwrap()
        .expect("should add delta");
    assert_eq!(added.entries.len(), 4);
    assert_eq!(added.entries[3].common().name, "Delta");

    // 3. Delete entry
    let deleted = editor.delete_entry(&e2).unwrap();
    let remaining_names: Vec<_> = deleted
        .entries
        .iter()
        .map(|e| e.common().name.as_str())
        .collect();
    assert_eq!(remaining_names, vec!["Gamma", "Alpha", "Delta"]);
}

#[tokio::test]
async fn reorder_by_index_and_validation_guards() {
    let e1 = make_native("Alpha");
    let e2 = make_native("Beta");
    let e3 = make_native("Gamma");

    let initial = AppConfig {
        credentials: None,
        entries: vec![e1.clone(), e2.clone(), e3.clone()],
    };
    let store = InMemoryStore {
        config: Arc::new(Mutex::new(initial.clone())),
    };
    let steam = MockSteam::default();
    let factory = SpyFactory::default();

    let editor = EditLibrary::new(store.clone(), factory.clone(), steam.clone(), |_| {});

    // 1. Reorder by index: move index 0 (Alpha) to index 2 -> [Beta, Gamma, Alpha]
    let reordered = editor.reorder_entries(&initial, 0, 2).unwrap();
    let names: Vec<_> = reordered
        .entries
        .iter()
        .map(|e| e.common().name.as_str())
        .collect();
    assert_eq!(names, vec!["Beta", "Gamma", "Alpha"]);

    // 2. Out-of-bounds or same-index reorder is a no-op
    let same = editor.reorder_entries(&initial, 1, 1).unwrap();
    assert_eq!(same, initial);
    let oob = editor.reorder_entries(&initial, 0, 99).unwrap();
    assert_eq!(oob, initial);

    // 3. Add duplicate name rejected
    let dup = make_native("Alpha");
    let add_dup = editor.add_entry(&initial, dup).unwrap();
    assert_eq!(add_dup, None);

    // 4. Add empty name rejected
    let empty = make_native("   ");
    let add_empty = editor.add_entry(&initial, empty).unwrap();
    assert_eq!(add_empty, None);

    // 5. Edit entry colliding with existing name rejected
    let mut colliding = e1.clone();
    colliding.common_mut_test_helper().name = "Beta".into();
    let edit_colliding = editor.edit_entry(&initial, &e1, &colliding).await.unwrap();
    assert_eq!(edit_colliding, None);
}

#[tokio::test]
async fn edit_entry_triggers_cloud_rename_and_updates_configuration() {
    let mut e1 = make_native("OldName");
    e1.common_mut_test_helper().cloud_folder = Some("CloudOld".into());

    let initial = AppConfig {
        credentials: Some(emusync_domain::config::Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: vec![e1.clone()],
    };
    let store = InMemoryStore {
        config: Arc::new(Mutex::new(initial.clone())),
    };
    let steam = MockSteam::default();
    let factory = SpyFactory::default();

    let editor = EditLibrary::new(store.clone(), factory.clone(), steam.clone(), |_| {});

    let mut edited = e1.clone();
    edited.common_mut_test_helper().name = "NewName".into();
    edited.common_mut_test_helper().cloud_folder = Some("CloudNew".into());

    let res = editor
        .edit_entry(&initial, &e1, &edited)
        .await
        .unwrap()
        .expect("should edit");
    assert_eq!(res.config.entries[0].common().name, "NewName");

    // Verified cloud rename was triggered
    let renames = factory.synchronizer.renamed_entries.lock().unwrap().clone();
    assert_eq!(renames.len(), 1);
    assert_eq!(renames[0].0.common().name, "OldName");
    assert_eq!(renames[0].1.common().name, "NewName");
}

#[test]
fn shared_cloud_root_is_preserved_when_shared_by_another_entry() {
    let mut emu1 = Emulator::new("Dolphin1", "/usr/bin/dolphin", "/roms", vec!["iso".into()]);
    emu1.common.cloud_folder = Some("SharedFolder".into());

    let mut emu2 = Emulator::new("Dolphin2", "/usr/bin/dolphin", "/roms", vec!["iso".into()]);
    emu2.common.cloud_folder = Some("SharedFolder".into());

    let config = AppConfig {
        credentials: None,
        entries: vec![GameEntry::Emulator(emu1.clone()), GameEntry::Emulator(emu2)],
    };

    // Editing emu1 with an empty cloud folder should retain "SharedFolder" because emu2 still shares it
    let old_entry = GameEntry::Emulator(emu1.clone());
    let mut new_entry = GameEntry::Emulator(Emulator::new(
        "DolphinRenamed",
        "/usr/bin/dolphin",
        "/roms",
        vec!["iso".into()],
    ));
    new_entry.common_mut_test_helper().cloud_folder = None;

    preserve_shared_cloud_root(&config, &old_entry, &mut new_entry);
    assert_eq!(
        new_entry.common().cloud_folder.as_deref(),
        Some("SharedFolder")
    );
}

#[test]
fn build_game_entry_applies_overrides() {
    let emu = Emulator::new(
        "RetroArch",
        "/usr/bin/retroarch",
        "/roms",
        vec!["sfc".into()],
    );
    let game = SyncGame {
        entry: GameEntry::Emulator(emu),
        name: "Super Mario World".into(),
        effective_title: "Super Mario World".into(),
        rom_path: Some("/roms/smw.sfc".into()),
        save_targets: Vec::new(),
    };

    let changes = GameOverrideChanges {
        targets: vec![SaveTarget {
            path: "/saves/smw.srm".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
        }],
        cover_path: Some("/covers/smw.png".into()),
        title: Some("SMW Customized".into()),
    };

    let updated = build_game_entry(&game, &changes);
    if let GameEntry::Emulator(e) = updated {
        assert_eq!(e.effective_title("smw.sfc", "fallback"), "SMW Customized");
        assert_eq!(e.cover_path_by_rom.len(), 1);
        assert_eq!(e.save_paths_by_rom.len(), 1);
    } else {
        panic!("expected emulator entry");
    }
}

#[test]
fn build_game_entry_clears_overrides_and_handles_native_games() {
    // 1. Emulator override removal
    let mut emu = Emulator::new("Snes9x", "/usr/bin/snes9x", "/roms", vec!["sfc".into()]);
    emu.title_by_rom = vec![("smw.sfc".into(), "Custom Title".into())];
    emu.cover_path_by_rom = vec![("smw.sfc".into(), "/covers/smw.png".into())];
    emu.save_paths_by_rom = vec![("smw.sfc".into(), vec!["/saves/smw.srm".into()])];
    emu.save_targets_by_rom = vec![(
        "smw.sfc".into(),
        vec![SaveTarget {
            path: "/saves/smw.srm".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
        }],
    )];

    let game = SyncGame {
        entry: GameEntry::Emulator(emu),
        name: "Super Mario World".into(),
        effective_title: "Custom Title".into(),
        rom_path: Some("/roms/smw.sfc".into()),
        save_targets: Vec::new(),
    };

    let clear_changes = GameOverrideChanges {
        targets: Vec::new(),
        cover_path: Some("".into()),
        title: Some("   ".into()),
    };

    let cleared = build_game_entry(&game, &clear_changes);
    if let GameEntry::Emulator(e) = cleared {
        assert_eq!(e.effective_title("smw.sfc", "fallback"), "fallback");
        assert!(e.cover_path_by_rom.is_empty());
        assert!(e.save_paths_by_rom.is_empty());
        assert!(e.save_targets_by_rom.is_empty());
    } else {
        panic!("expected emulator entry");
    }

    // 2. Native PC Game overrides
    let nat = make_native("NativeGame");
    let nat_game = SyncGame {
        entry: nat,
        name: "NativeGame".into(),
        effective_title: "NativeGame".into(),
        rom_path: None,
        save_targets: Vec::new(),
    };

    let nat_changes = GameOverrideChanges {
        targets: vec![SaveTarget {
            path: "/saves/native.dat".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
        }],
        cover_path: Some("/covers/native.jpg".into()),
        title: Some("Renamed Native Game".into()),
    };

    let nat_updated = build_game_entry(&nat_game, &nat_changes);
    if let GameEntry::Native(n) = nat_updated {
        assert_eq!(n.common.name, "Renamed Native Game");
        assert_eq!(n.cover_path.as_deref(), Some("/covers/native.jpg"));
        assert_eq!(n.common.save_targets.len(), 1);
        assert_eq!(n.common.save_paths, vec!["/saves/native.dat"]);
    } else {
        panic!("expected native entry");
    }
}

#[test]
fn build_game_entry_updates_existing_rom_overrides_and_resolves_rom_name() {
    // 1. rom_path with Windows backslash and existing overrides being updated
    let mut emu = Emulator::new("PCSX2", "/usr/bin/pcsx2", "/roms", vec!["iso".into()]);
    emu.title_by_rom = vec![("game.iso".into(), "Old ISO Title".into())];
    emu.save_paths_by_rom = vec![("game.iso".into(), vec!["/saves/old.mcd".into()])];
    emu.save_targets_by_rom = vec![(
        "game.iso".into(),
        vec![SaveTarget {
            path: "/saves/old.mcd".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
        }],
    )];

    let game = SyncGame {
        entry: GameEntry::Emulator(emu),
        name: "Game ISO".into(),
        effective_title: "Old ISO Title".into(),
        rom_path: Some(r"D:\Emulation\PS2\game.iso".into()),
        save_targets: Vec::new(),
    };

    let changes = GameOverrideChanges {
        targets: vec![SaveTarget {
            path: "/saves/new.mcd".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
        }],
        cover_path: Some("/covers/ps2.png".into()),
        title: Some("New ISO Title".into()),
    };

    let updated = build_game_entry(&game, &changes);
    if let GameEntry::Emulator(e) = updated {
        assert_eq!(e.title_by_rom.len(), 1);
        assert_eq!(e.title_by_rom[0].1, "New ISO Title");
        assert_eq!(e.save_paths_by_rom.len(), 1);
        assert_eq!(e.save_paths_by_rom[0].1, vec!["/saves/new.mcd"]);
        assert_eq!(e.save_targets_by_rom.len(), 1);
        assert_eq!(e.save_targets_by_rom[0].1[0].path, "/saves/new.mcd");
        assert_eq!(e.cover_path_by_rom.len(), 1);
        assert_eq!(e.cover_path_by_rom[0].1, "/covers/ps2.png");
    } else {
        panic!("expected emulator");
    }

    // 2. rom_path is None -> falls back to game.name
    let game_no_rom = SyncGame {
        entry: GameEntry::Emulator(Emulator::new("Emu", "/bin/emu", "/roms", vec![])),
        name: "FallbackRomKey".into(),
        effective_title: "FallbackRomKey".into(),
        rom_path: None,
        save_targets: Vec::new(),
    };
    let changes2 = GameOverrideChanges {
        targets: Vec::new(),
        cover_path: None,
        title: Some("Overridden By Name".into()),
    };
    let updated2 = build_game_entry(&game_no_rom, &changes2);
    if let GameEntry::Emulator(e) = updated2 {
        assert_eq!(
            e.title_by_rom,
            vec![("FallbackRomKey".into(), "Overridden By Name".into())]
        );
    } else {
        panic!("expected emulator");
    }
}

#[test]
fn preserve_shared_cloud_root_respects_explicit_override_or_unshared_root() {
    let mut emu1 = Emulator::new("E1", "/bin/e1", "/roms", vec![]);
    emu1.common.cloud_folder = Some("CloudRoot".into());

    let mut emu2 = Emulator::new("E2", "/bin/e2", "/roms", vec![]);
    emu2.common.cloud_folder = Some("CloudRoot".into());

    let config = AppConfig {
        credentials: None,
        entries: vec![GameEntry::Emulator(emu1.clone()), GameEntry::Emulator(emu2)],
    };

    // Case 1: user explicitly specified a non-blank cloud folder -> should NOT overwrite with old CloudRoot
    let old_entry = GameEntry::Emulator(emu1.clone());
    let mut new_entry = GameEntry::Emulator(Emulator::new("E1Renamed", "/bin/e1", "/roms", vec![]));
    new_entry.common_mut_test_helper().cloud_folder = Some("CustomNewFolder".into());

    preserve_shared_cloud_root(&config, &old_entry, &mut new_entry);
    assert_eq!(
        new_entry.common().cloud_folder.as_deref(),
        Some("CustomNewFolder")
    );

    // Case 2: old entry's root was not shared with any other entry in config -> should NOT preserve
    let mut solo = Emulator::new("Solo", "/bin/solo", "/roms", vec![]);
    solo.common.cloud_folder = Some("UniqueCloudFolder".into());
    let solo_config = AppConfig {
        credentials: None,
        entries: vec![GameEntry::Emulator(solo.clone())],
    };
    let mut new_solo =
        GameEntry::Emulator(Emulator::new("SoloRenamed", "/bin/solo", "/roms", vec![]));
    new_solo.common_mut_test_helper().cloud_folder = None;

    preserve_shared_cloud_root(&solo_config, &GameEntry::Emulator(solo), &mut new_solo);
    assert_eq!(new_solo.common().cloud_folder, None);
}

#[tokio::test]
async fn steam_registration_registers_and_unregisters_shortcuts() {
    let entry = make_native("SteamGame");
    let initial = AppConfig {
        credentials: None,
        entries: vec![entry.clone()],
    };
    let store = InMemoryStore {
        config: Arc::new(Mutex::new(initial)),
    };
    let steam = MockSteam::default();

    let registration = SteamRegistration::new(store.clone(), steam.clone());

    // Register
    let reg_result = registration.register(&entry).unwrap();
    assert_eq!(
        reg_result.config.entries[0].common().steam_app_id,
        Some(12345)
    );
    assert!(steam.is_registered(&entry).unwrap());

    // Unregister
    let unreg_entry = reg_result.config.entries[0].clone();
    let unreg_result = registration.unregister(&unreg_entry).unwrap();
    assert_eq!(unreg_result.config.entries[0].common().steam_app_id, None);
    assert!(!steam.is_registered(&entry).unwrap());
}

#[tokio::test]
async fn edit_entry_forwards_synchronizer_events_to_on_status() {
    let mut e1 = make_native("OldName");
    e1.common_mut_test_helper().cloud_folder = Some("CloudOld".into());

    let initial = AppConfig {
        credentials: Some(emusync_domain::config::Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: vec![e1.clone()],
    };
    let store = InMemoryStore {
        config: Arc::new(Mutex::new(initial.clone())),
    };
    let steam = MockSteam::default();
    let factory = SpyFactory::default();
    *factory.status_emitter.lock().unwrap() = Some(SyncEvent::Error("Cloud sync failure".into()));

    let received_events = Arc::new(Mutex::new(Vec::new()));
    let rx_clone = Arc::clone(&received_events);
    let editor = EditLibrary::new(store.clone(), factory.clone(), steam.clone(), move |ev| {
        rx_clone.lock().unwrap().push(ev);
    });

    let mut edited = e1.clone();
    edited.common_mut_test_helper().name = "NewName".into();

    let _ = editor.edit_entry(&initial, &e1, &edited).await;

    let events = received_events.lock().unwrap().clone();
    assert_eq!(events, vec![SyncEvent::Error("Cloud sync failure".into())]);
}

#[test]
fn build_game_entry_sanitizes_targets_and_filters_empty_in_emulator_and_native() {
    // 1. Emulator trimming and filtering
    let emu = Emulator::new("Snes9x", "/usr/bin/snes9x", "/roms", vec!["sfc".into()]);
    let emu_game = SyncGame {
        entry: GameEntry::Emulator(emu),
        name: "Super Mario World".into(),
        effective_title: "Super Mario World".into(),
        rom_path: Some("/roms/smw.sfc".into()),
        save_targets: Vec::new(),
    };
    let changes = GameOverrideChanges {
        targets: vec![
            SaveTarget {
                path: "   ".into(),
                kind: SaveTargetType::File,
                cloud_path: None,
            },
            SaveTarget {
                path: "  /saves/smw.srm  ".into(),
                kind: SaveTargetType::File,
                cloud_path: Some("  smw_cloud.srm  ".into()),
            },
        ],
        cover_path: None,
        title: None,
    };
    let updated_emu = build_game_entry(&emu_game, &changes);
    if let GameEntry::Emulator(e) = updated_emu {
        assert_eq!(e.save_paths_by_rom[0].1, vec!["/saves/smw.srm"]);
        assert_eq!(e.save_targets_by_rom[0].1.len(), 1);
        assert_eq!(e.save_targets_by_rom[0].1[0].path, "/saves/smw.srm");
        assert_eq!(
            e.save_targets_by_rom[0].1[0].cloud_path.as_deref(),
            Some("smw_cloud.srm")
        );
    } else {
        panic!("expected emulator");
    }

    // 2. Native trimming and filtering
    let nat = make_native("NativeGame");
    let nat_game = SyncGame {
        entry: nat,
        name: "NativeGame".into(),
        effective_title: "NativeGame".into(),
        rom_path: None,
        save_targets: Vec::new(),
    };
    let updated_nat = build_game_entry(&nat_game, &changes);
    if let GameEntry::Native(n) = updated_nat {
        assert_eq!(n.common.save_paths, vec!["/saves/smw.srm"]);
        assert_eq!(n.common.save_targets.len(), 1);
        assert_eq!(n.common.save_targets[0].path, "/saves/smw.srm");
        assert_eq!(
            n.common.save_targets[0].cloud_path.as_deref(),
            Some("smw_cloud.srm")
        );
    } else {
        panic!("expected native");
    }
}

trait CommonMutHelper {
    fn common_mut_test_helper(&mut self) -> &mut EntryCommon;
}

impl CommonMutHelper for GameEntry {
    fn common_mut_test_helper(&mut self) -> &mut EntryCommon {
        match self {
            GameEntry::Native(n) => &mut n.common,
            GameEntry::Emulator(e) => &mut e.common,
        }
    }
}

// --- Adversarial Edit Library Tests ---

#[tokio::test]
async fn edit_entry_and_edit_game_override_abort_without_store_or_steam_mutation_when_cloud_rename_fails()
 {
    let creds = Some(Credentials {
        client_id: "c".into(),
        client_secret: "s".into(),
        refresh_token: Some("rt".into()),
    });

    let mut initial_entry = make_native("Game");
    if let GameEntry::Native(ref mut n) = initial_entry {
        n.common.steam_app_id = Some(12345);
    }

    let initial_config = AppConfig {
        credentials: creds,
        entries: vec![initial_entry.clone()],
    };

    let store = InMemoryStore {
        config: Arc::new(Mutex::new(initial_config.clone())),
    };
    let steam = MockSteam::default();
    let factory = SpyFactory::default();
    let edit = EditLibrary::new(store.clone(), factory.clone(), steam.clone(), |_| {});

    let mut new_entry = make_native("RenamedGame");
    if let GameEntry::Native(ref mut n) = new_entry {
        n.common.steam_app_id = Some(12345);
    }

    // 1. rename_entry returns Ok(false) -> aborts, no store mutation, no Steam register
    *factory.synchronizer.rename_entry_result.lock().unwrap() = Some(Ok(false));
    let res1 = edit
        .edit_entry(&initial_config, &initial_entry, &new_entry)
        .await
        .unwrap();
    assert!(
        res1.is_none(),
        "edit_entry must return Ok(None) when rename_entry is false"
    );
    assert_eq!(store.load().unwrap().entries[0].common().name, "Game");
    assert!(
        !steam
            .registered_names
            .lock()
            .unwrap()
            .contains("RenamedGame")
    );

    // 2. rename_entry returns Err("network error") -> aborts, no store mutation, no Steam register
    *factory.synchronizer.rename_entry_result.lock().unwrap() = Some(Err("network error".into()));
    let res2 = edit
        .edit_entry(&initial_config, &initial_entry, &new_entry)
        .await
        .unwrap();
    assert!(
        res2.is_none(),
        "edit_entry must return Ok(None) when rename_entry fails with error"
    );
    assert_eq!(store.load().unwrap().entries[0].common().name, "Game");
    assert!(
        !steam
            .registered_names
            .lock()
            .unwrap()
            .contains("RenamedGame")
    );

    // 3. edit_game_override with rename_game returning Ok(false) or Err
    let emu_entry = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "EmuGame".into(),
            steam_app_id: Some(12345),
            ..EntryCommon::default()
        },
        save_targets_by_rom: vec![("rom.sfc".into(), vec![])],
        title_by_rom: vec![("rom.sfc".into(), "Old ROM Title".into())],
        ..Default::default()
    });
    *store.config.lock().unwrap() = AppConfig {
        credentials: initial_config.credentials.clone(),
        entries: vec![emu_entry.clone()],
    };
    let config_with_emu = store.load().unwrap();

    let game = SyncGame {
        entry: emu_entry.clone(),
        name: "rom.sfc".into(),
        effective_title: "Old ROM Title".into(),
        rom_path: Some("/roms/rom.sfc".into()),
        save_targets: Vec::new(),
    };
    let changes = GameOverrideChanges {
        title: Some("New ROM Title".into()),
        targets: vec![],
        cover_path: None,
    };

    *factory.synchronizer.rename_game_result.lock().unwrap() = Some(Ok(false));
    let res3 = edit
        .edit_game_override(&config_with_emu, &game, &changes)
        .await
        .unwrap();
    assert!(
        res3.is_none(),
        "edit_game_override must return Ok(None) when rename_game is false"
    );
    assert_eq!(store.load().unwrap().entries[0], emu_entry);

    *factory.synchronizer.rename_game_result.lock().unwrap() = Some(Err("network error".into()));
    let res4 = edit
        .edit_game_override(&config_with_emu, &game, &changes)
        .await
        .unwrap();
    assert!(
        res4.is_none(),
        "edit_game_override must return Ok(None) when rename_game fails with error"
    );
    assert_eq!(store.load().unwrap().entries[0], emu_entry);
}

#[tokio::test]
async fn edit_game_override_merges_onto_latest_config_entry_instead_of_stale_sync_game_entry() {
    // Config has an emulator entry that already contains an override for "rom_a.sfc"
    let latest_emu = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "RetroArch".into(),
            ..EntryCommon::default()
        },
        save_targets_by_rom: vec![(
            "rom_a.sfc".into(),
            vec![SaveTarget {
                path: "/saves/rom_a.srm".into(),
                kind: SaveTargetType::File,
                cloud_path: None,
            }],
        )],
        title_by_rom: vec![("rom_a.sfc".into(), "Title A".into())],
        ..Default::default()
    });

    let config = AppConfig {
        credentials: None,
        entries: vec![latest_emu.clone()],
    };

    let store = InMemoryStore {
        config: Arc::new(Mutex::new(config.clone())),
    };
    let steam = MockSteam::default();
    let factory = SpyFactory::default();
    let edit = EditLibrary::new(store.clone(), factory, steam, |_| {});

    // Stale SyncGame for "rom_b.sfc" whose game.entry is a stale snapshot WITHOUT "rom_a.sfc"
    let stale_emu = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "RetroArch".into(),
            ..EntryCommon::default()
        },
        save_targets_by_rom: vec![],
        title_by_rom: vec![],
        ..Default::default()
    });
    let stale_game = SyncGame {
        entry: stale_emu,
        name: "rom_b.sfc".into(),
        effective_title: "Title B".into(),
        rom_path: Some("/roms/rom_b.sfc".into()),
        save_targets: Vec::new(),
    };

    let changes = GameOverrideChanges {
        title: Some("New Title B".into()),
        targets: vec![SaveTarget {
            path: "/saves/rom_b.srm".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
        }],
        cover_path: None,
    };

    let res = edit
        .edit_game_override(&config, &stale_game, &changes)
        .await
        .unwrap()
        .expect("edit_game_override should succeed");

    // Must merge onto latest, so both rom_a.sfc and rom_b.sfc overrides are preserved!
    if let GameEntry::Emulator(ref updated) = res.entry {
        let has_a = updated
            .save_targets_by_rom
            .iter()
            .any(|(rom, targets)| rom == "rom_a.sfc" && targets[0].path == "/saves/rom_a.srm");
        let has_b = updated
            .save_targets_by_rom
            .iter()
            .any(|(rom, targets)| rom == "rom_b.sfc" && targets[0].path == "/saves/rom_b.srm");
        assert!(
            has_a,
            "Override for rom_a.sfc must be retained from latest config"
        );
        assert!(has_b, "Override for rom_b.sfc must be added from changes");

        assert!(
            updated
                .title_by_rom
                .iter()
                .any(|(rom, title)| rom == "rom_a.sfc" && title == "Title A")
        );
        assert!(
            updated
                .title_by_rom
                .iter()
                .any(|(rom, title)| rom == "rom_b.sfc" && title == "New Title B")
        );
    } else {
        panic!("Expected emulator entry");
    }
}

#[test]
fn preserve_shared_cloud_root_does_not_force_cloud_folder_on_grouped_native_game_without_explicit_cloud_folder()
 {
    let old_entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Game1".into(),
            group: Some("SharedGroup".into()),
            cloud_folder: None,
            ..Default::default()
        },
        ..Default::default()
    });

    let mut new_entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "RenamedGame1".into(),
            group: Some("SharedGroup".into()),
            cloud_folder: None,
            ..Default::default()
        },
        ..Default::default()
    });

    let sibling_entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Game2".into(),
            group: Some("SharedGroup".into()),
            cloud_folder: None,
            ..Default::default()
        },
        ..Default::default()
    });

    let config = AppConfig {
        credentials: None,
        entries: vec![old_entry.clone(), sibling_entry],
    };

    // Because source_uses_root == false (group.is_some() && cloud_folder.is_none()),
    // preserve_shared_cloud_root must leave cloud_folder as None!
    preserve_shared_cloud_root(&config, &old_entry, &mut new_entry);
    assert_eq!(
        new_entry.common().cloud_folder,
        None,
        "preserve_shared_cloud_root must not force cloud_folder on grouped native game without explicit cloud_folder"
    );
}
