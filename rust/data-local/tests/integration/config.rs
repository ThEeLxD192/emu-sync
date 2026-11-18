use emusync_data_local::ConfigManager;
use emusync_domain::{
    config::{ConfigurationStore, CredentialStore, Credentials, Language},
    library::{EntryCommon, GameEntry, NativeGame, SaveTargetType},
};
use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};

#[test]
fn config_loads_legacy_defaults_round_trips_and_preserves_concurrent_updates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    fs::write(
        &path,
        r#"{"googleDrive":{"clientId":"id","clientSecret":"secret"},"entries":[{"type":"emulator","name":"Emu","executablePath":"emu","romsDirectory":"roms","extensions":["gb"],"titleByRom":{"first":"One","second":"Two"}},{"type":"native","name":"Game","executablePath":"game","arguments":[]}],"future":true}"#,
    )
    .unwrap();
    let store = ConfigManager::new(&path).unwrap();
    let config = store.load().unwrap();
    let GameEntry::Emulator(emu) = &config.entries[0] else {
        panic!()
    };
    assert_eq!(emu.common.arguments, ["{ROM}"]);
    assert_eq!(emu.title_by_rom[0].0, "first");
    store.save(&config).unwrap();
    assert_eq!(store.load().unwrap(), config);
    let snapshot = store.snapshot(&config).unwrap();
    store
        .update_credentials(|_| {
            Some(Credentials {
                client_id: "new".into(),
                client_secret: "secret".into(),
                refresh_token: None,
            })
        })
        .unwrap();
    assert!(!store.compare_and_set(&snapshot, None, &config).unwrap());
    let latest = store.snapshot(&config).unwrap();
    store
        .update(|mut c| {
            c.entries.remove(1);
            c
        })
        .unwrap();
    assert!(
        store
            .compare_and_set(
                &latest,
                Some(Credentials {
                    client_id: "new".into(),
                    client_secret: "secret".into(),
                    refresh_token: Some("token".into()),
                }),
                &config
            )
            .unwrap()
    );
    assert_eq!(store.load().unwrap().entries.len(), 1);
}

#[test]
fn explicit_empty_emulator_arguments_survive_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    fs::write(
        &path,
        r#"{"entries":[{"type":"emulator","name":"E","executablePath":"e","romsDirectory":"r","extensions":[],"arguments":[]}]}"#,
    )
    .unwrap();
    let store = ConfigManager::new(&path).unwrap();
    let config = store.load().unwrap();
    store.save(&config).unwrap();
    assert_eq!(store.load().unwrap(), config);
}

#[test]
fn concurrent_config_updates_merge_and_legacy_targets_keep_paths() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let save = dir.path().join("legacy.sav");
    fs::write(&save, b"save").unwrap();
    let manager = ConfigManager::new(&path).unwrap();
    manager
        .update(|mut c| {
            c.entries.push(GameEntry::Native(NativeGame {
                common: EntryCommon {
                    name: "legacy".into(),
                    save_paths: vec![save.to_str().unwrap().into()],
                    ..Default::default()
                },
                ..Default::default()
            }));
            c
        })
        .unwrap();
    let barrier = Arc::new(Barrier::new(9));
    thread::scope(|scope| {
        for n in 0..8 {
            let path = &path;
            let barrier = barrier.clone();
            scope.spawn(move || {
                let store = ConfigManager::new(path).unwrap();
                barrier.wait();
                store
                    .update(|mut c| {
                        c.entries.push(GameEntry::Native(NativeGame {
                            common: EntryCommon {
                                name: n.to_string(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }));
                        c
                    })
                    .unwrap();
            });
        }
        barrier.wait();
    });
    let config = manager.load().unwrap();
    assert_eq!(config.entries.len(), 9);
    let common = config.entries[0].common();
    assert_eq!(common.save_paths, [save.to_str().unwrap()]);
    assert_eq!(common.save_targets[0].kind, SaveTargetType::File);
    assert_eq!(common.save_targets[0].path, save.to_str().unwrap());
}

#[test]
fn literal_config_preserves_all_nondefault_fields_and_ordered_titles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let literal = r#"{"googleDrive":{"clientId":"client","clientSecret":"secret","refreshToken":"refresh"},"entries":[{"type":"emulator","name":"System","executablePath":"/bin/emu","arguments":["--game","{ROM}"],"romsDirectory":"/roms","extensions":["iso","bin"],"savePaths":["/synthetic/system-file"],"saveTargets":[{"path":"/synthetic/system-file","type":"file","cloudPath":"slot/save.dat"},{"path":"/synthetic/system-dir","type":"directory","cloudPath":"profile","excludedFolders":["replays","cache/tmp"]},{"path":"/synthetic/unknown","type":"unknown","cloudPath":"legacy"}],"savePathsByRom":{"Game.iso":["/synthetic/game-file"]},"saveTargetsByRom":{"Game.iso":[{"path":"/synthetic/game-file","type":"file","cloudPath":"game.sav"}]},"coverPathByRom":{"Game.iso":"/covers/game.png"},"titleByRom":{"first":"One","second":"Two"},"group":"Console","cloudFolder":"Existing saves","driveFileId":"remote-system","steamAppId":-123,"steamProcessName":"emu-process","fullscreenArgs":"--fullscreen","coversDirectory":"/covers"},{"type":"native","name":"Native","executablePath":"/bin/game","arguments":["--safe"],"savePaths":["/synthetic/native-file"],"saveTargets":[{"path":"/synthetic/native-file","type":"file","cloudPath":"native.sav"}],"waitForProcess":"game-process","group":"PC","cloudFolder":"Native saves","driveFileId":"remote-native","steamAppId":456,"coverPath":"/covers/native.png"}]}"#;
    fs::write(&path, literal).unwrap();
    let store = ConfigManager::new(&path).unwrap();
    let config = store.load().unwrap();
    store.save(&config).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(literal).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap()
    );
    let GameEntry::Emulator(emu) = &config.entries[0] else {
        panic!()
    };
    assert_eq!(emu.title_by_rom[0].0, "first");
    assert_eq!(emu.title_by_rom[1].0, "second");
    assert_eq!(
        emu.common.save_targets[1].excluded_folders,
        ["replays", "cache/tmp"]
    );
    assert!(emu.common.save_targets[0].excluded_folders.is_empty());
}

#[test]
fn read_only_operations_do_not_rewrite_config() {
    use emusync_domain::config::CredentialTransaction;

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let target_save = dir.path().join("target.sav");

    let initial_json = serde_json::json!({
        "googleDrive": {
            "clientId": "client",
            "clientSecret": "secret"
        },
        "entries": [
            {
                "type": "native",
                "name": "Game",
                "executablePath": "game",
                "savePaths": [target_save.to_str().unwrap()],
                "saveTargets": [
                    {
                        "path": target_save.to_str().unwrap(),
                        "type": "unknown"
                    }
                ]
            }
        ]
    });
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&initial_json).unwrap(),
    )
    .unwrap();

    let before_bytes = fs::read(&config_path).unwrap();

    // Now create the save file on disk
    fs::write(&target_save, b"save contents").unwrap();

    let store = ConfigManager::new(&config_path).unwrap();
    let fallback = emusync_domain::config::AppConfig::default();

    // snapshot should not trigger migration or disk write
    let snapshot = store.snapshot(&fallback).unwrap();
    assert_eq!(fs::read(&config_path).unwrap(), before_bytes);

    // while_current should also not trigger migration or disk write
    let called = store.while_current(&snapshot, &fallback, || 123).unwrap();
    assert_eq!(called, Some(123));
    assert_eq!(fs::read(&config_path).unwrap(), before_bytes);
}

#[test]
fn empty_zero_byte_config_file_is_treated_as_error_not_silently_overwritten_on_read_raw_while_missing_file_auto_creates()
 {
    use emusync_domain::config::AppConfig;

    let dir = tempfile::tempdir().unwrap();
    let missing_path = dir.path().join("missing/config.json");
    let missing_store = ConfigManager::new(&missing_path).unwrap();

    // Missing file auto-creates default config on load()
    let loaded_default = missing_store.load().unwrap();
    assert_eq!(loaded_default, AppConfig::default());
    assert!(missing_path.exists());
    let created_bytes = fs::read(&missing_path).unwrap();
    assert!(!created_bytes.is_empty());
    assert!(serde_json::from_slice::<serde_json::Value>(&created_bytes).is_ok());

    // Existing zero-byte file must fail with "empty configuration" and must NOT be overwritten
    let empty_dir = dir.path().join("empty");
    fs::create_dir_all(&empty_dir).unwrap();
    let empty_path = empty_dir.join("config.json");
    fs::write(&empty_path, b"").unwrap();
    assert_eq!(fs::metadata(&empty_path).unwrap().len(), 0);

    let empty_store = ConfigManager::new(&empty_path).unwrap();
    let load_err = empty_store.load().unwrap_err();
    assert_eq!(load_err.kind(), std::io::ErrorKind::NotFound);
    assert!(load_err.to_string().contains("empty configuration"));

    let snapshot_err = empty_store.snapshot(&AppConfig::default()).unwrap_err();
    assert!(snapshot_err.to_string().contains("empty configuration"));

    let update_err = empty_store.update(|c| c).unwrap_err();
    assert!(update_err.to_string().contains("empty configuration"));

    // Verify empty file still has exactly 0 bytes (not overwritten with `{}`)
    assert_eq!(fs::metadata(&empty_path).unwrap().len(), 0);
}

#[test]
fn library_store_and_emulator_rom_migration_populate_only_empty_rom_targets() {
    use emusync_domain::library::{
        EntryCommon, GameEntry, LibraryStore, NativeGame, SaveTargetType,
    };

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");

    let file_path = dir.path().join("real_save.sav");
    fs::write(&file_path, b"bytes").unwrap();
    let file_path_str = file_path.to_str().unwrap();

    let other_path = dir.path().join("other_save.sav");
    fs::write(&other_path, b"other").unwrap();
    let other_path_str = other_path.to_str().unwrap();

    let custom_path = "/synthetic/custom.sav";

    let initial_json = serde_json::json!({
        "entries": [
            {
                "type": "emulator",
                "name": "Emu",
                "executablePath": "emu",
                "romsDirectory": "roms",
                "extensions": ["iso"],
                "savePathsByRom": {
                    "rom_new.iso": [file_path_str],
                    "rom_existing.iso": [other_path_str],
                    "rom_blank.iso": ["  ", ""]
                },
                "saveTargetsByRom": {
                    "rom_existing.iso": [
                        {
                            "path": custom_path,
                            "type": "file",
                            "cloudPath": "kept.sav"
                        }
                    ]
                }
            }
        ]
    });

    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&initial_json).unwrap(),
    )
    .unwrap();

    let store = ConfigManager::new(&config_path).unwrap();
    let entries = store.load_entries().unwrap();
    assert_eq!(entries.len(), 1);

    let GameEntry::Emulator(emu) = &entries[0] else {
        panic!("Expected emulator entry");
    };

    // 1. rom_new.iso was migrated to save_targets_by_rom with SaveTargetType::File
    let new_target = emu
        .save_targets_by_rom
        .iter()
        .find(|(k, _)| k == "rom_new.iso")
        .expect("rom_new.iso should be migrated");
    assert_eq!(new_target.1.len(), 1);
    assert_eq!(new_target.1[0].path, file_path_str);
    assert_eq!(new_target.1[0].kind, SaveTargetType::File);

    // 2. rom_existing.iso preserves custom_path and cloudPath: Some("kept.sav") intact
    let existing_target = emu
        .save_targets_by_rom
        .iter()
        .find(|(k, _)| k == "rom_existing.iso")
        .expect("rom_existing.iso should be present");
    assert_eq!(existing_target.1.len(), 1);
    assert_eq!(existing_target.1[0].path, custom_path);
    assert_eq!(existing_target.1[0].cloud_path.as_deref(), Some("kept.sav"));

    // 3. rom_blank.iso was not added to save_targets_by_rom
    assert!(
        !emu.save_targets_by_rom
            .iter()
            .any(|(k, _)| k == "rom_blank.iso"),
        "rom_blank.iso must not be added to save_targets_by_rom"
    );

    // 4. update_entries persists new entries to disk
    let updated = store
        .update_entries(|mut list| {
            list.push(GameEntry::Native(NativeGame {
                common: EntryCommon {
                    name: "NativeAdded".into(),
                    ..Default::default()
                },
                ..Default::default()
            }));
            list
        })
        .unwrap();
    assert_eq!(updated.len(), 2);

    let reloaded = store.load_entries().unwrap();
    assert_eq!(reloaded.len(), 2);
    assert_eq!(reloaded[1].common().name, "NativeAdded");
}

#[test]
fn a_configuration_written_before_languages_reads_as_english_and_is_not_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let written =
        r#"{"entries":[{"type":"native","name":"Game","executablePath":"game","arguments":[]}]}"#;
    fs::write(&path, written).unwrap();
    let store = ConfigManager::new(&path).unwrap();

    assert_eq!(store.load().unwrap().language, Language::English);
    assert_eq!(fs::read_to_string(&path).unwrap(), written);
}

#[test]
fn the_language_is_stored_by_code_and_english_adds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    fs::write(
        &path,
        r#"{"googleDrive":{"clientId":"id","clientSecret":"secret"},"entries":[{"type":"native","name":"Game","executablePath":"game","arguments":[]}]}"#,
    )
    .unwrap();
    let store = ConfigManager::new(&path).unwrap();
    let before = store.load().unwrap();

    store
        .update(|mut config| {
            config.language = Language::Spanish;
            config
        })
        .unwrap();
    let stored: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(stored["language"], "es");
    let spanish = store.load().unwrap();
    assert_eq!(spanish.language, Language::Spanish);
    assert_eq!(
        (spanish.credentials, spanish.entries),
        (before.credentials, before.entries),
        "only the language changed"
    );

    store
        .update(|mut config| {
            config.language = Language::English;
            config
        })
        .unwrap();
    let stored: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(stored.get("language").is_none(), "English is the default");
}

#[test]
fn an_unknown_language_reads_as_english() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    fs::write(&path, r#"{"language":"fr"}"#).unwrap();

    let config = ConfigManager::new(&path).unwrap().load().unwrap();

    assert_eq!(config.language, Language::English);
}
