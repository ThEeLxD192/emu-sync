use emusync_application::{
    EditLibrary, GameOverrideChanges, SteamRegistration, build_game_entry,
    preserve_shared_cloud_root,
};
use emusync_domain::config::{AppConfig, ConfigurationStore};
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
        Ok(true)
    }

    async fn rename_entry(&self, old: &GameEntry, new: &GameEntry) -> Result<bool, Self::Error> {
        self.renamed_entries
            .lock()
            .unwrap()
            .push((old.clone(), new.clone()));
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
}

impl SaveSynchronizerFactory for SpyFactory {
    type Synchronizer = SpySynchronizer;

    fn create(
        &self,
        _config: AppConfig,
        _on_status: impl Fn(SyncEvent) + Send + Sync + 'static,
    ) -> Self::Synchronizer {
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

    let updated = build_game_entry(&game, &changes).expect("build entry should succeed");
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

    let cleared =
        build_game_entry(&game, &clear_changes).expect("clearing overrides should succeed");
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

    let nat_updated =
        build_game_entry(&nat_game, &nat_changes).expect("native override should succeed");
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

    let updated = build_game_entry(&game, &changes).expect("build entry should succeed");
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
    let updated2 = build_game_entry(&game_no_rom, &changes2).expect("should succeed");
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
