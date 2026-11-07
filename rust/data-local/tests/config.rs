use emusync_data_local::ConfigManager;
use emusync_domain::{
    config::{ConfigurationStore, CredentialStore, Credentials},
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
    let literal = r#"{"googleDrive":{"clientId":"client","clientSecret":"secret","refreshToken":"refresh"},"entries":[{"type":"emulator","name":"System","executablePath":"/bin/emu","arguments":["--game","{ROM}"],"romsDirectory":"/roms","extensions":["iso","bin"],"savePaths":["/synthetic/system-file"],"saveTargets":[{"path":"/synthetic/system-file","type":"file","cloudPath":"slot/save.dat"},{"path":"/synthetic/system-dir","type":"directory","cloudPath":"profile"},{"path":"/synthetic/unknown","type":"unknown","cloudPath":"legacy"}],"savePathsByRom":{"Game.iso":["/synthetic/game-file"]},"saveTargetsByRom":{"Game.iso":[{"path":"/synthetic/game-file","type":"file","cloudPath":"game.sav"}]},"coverPathByRom":{"Game.iso":"/covers/game.png"},"titleByRom":{"first":"One","second":"Two"},"group":"Console","cloudFolder":"Existing saves","driveFileId":"remote-system","steamAppId":-123,"steamProcessName":"emu-process","fullscreenArgs":"--fullscreen","coversDirectory":"/covers"},{"type":"native","name":"Native","executablePath":"/bin/game","arguments":["--safe"],"savePaths":["/synthetic/native-file"],"saveTargets":[{"path":"/synthetic/native-file","type":"file","cloudPath":"native.sav"}],"waitForProcess":"game-process","group":"PC","cloudFolder":"Native saves","driveFileId":"remote-native","steamAppId":456,"coverPath":"/covers/native.png"}]}"#;
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
}
