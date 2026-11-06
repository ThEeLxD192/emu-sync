use emusync_domain::library::{Emulator, GameEntry, NativeGame};
use emusync_platform_desktop::{
    BinaryVdf, SteamShortcut, SteamShortcutManager, WindowsLaunchConfiguration,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn registers_emulator_entry_and_creates_shortcut_on_linux() {
    let dir = tempdir().unwrap();
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    let launcher_dir = dir.path().join("launchers");

    let manager = SteamShortcutManager::new()
        .with_custom_shortcuts_file(vdf_file.clone())
        .with_custom_launcher_dir(launcher_dir.clone())
        .with_simulate_windows(false);

    let mut emu = Emulator::new(
        "Game Boy Advance",
        "/usr/bin/mgba-qt",
        "/roms/gba",
        vec!["gba".into()],
    );
    emu.common.arguments = vec!["{ROM}".into()];
    let entry = GameEntry::Emulator(emu);

    let shortcut = manager.register_entry(&entry).unwrap();
    assert_eq!(shortcut.app_name, "[EmuSync] Game Boy Advance");
    assert!(shortcut.exe.ends_with(&format!(
        "launch_{}.sh\"",
        manager.launcher_key("Game Boy Advance")
    )));
    assert_eq!(shortcut.launch_options, "");

    let loaded = manager.load_shortcuts().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].app_id, shortcut.app_id);

    // Verify Linux launcher script
    let script = launcher_dir.join(format!(
        "launch_{}.sh",
        manager.launcher_key("Game Boy Advance")
    ));
    assert!(script.exists());
    let content = fs::read_to_string(&script).unwrap();
    assert!(content.starts_with("#!/bin/bash"));
    assert!(content.contains("FULLSCREEN_ARGS=(\"-f\")"));
    assert!(
        content.contains("exec \"/usr/bin/mgba-qt\" \"${FULLSCREEN_ARGS[@]}\" \"${ROM_PATH}\"")
    );
}

#[test]
fn registers_emulator_entry_and_creates_batch_script_on_windows() {
    let dir = tempdir().unwrap();
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    let launcher_dir = dir.path().join("launchers");

    let manager = SteamShortcutManager::new()
        .with_custom_shortcuts_file(vdf_file)
        .with_custom_launcher_dir(launcher_dir.clone())
        .with_simulate_windows(true);

    let emu = Emulator::new(
        "Game Boy Advance",
        "C:\\Emulators\\mGBA\\mgba.exe",
        "C:\\ROMs\\gba",
        vec!["gba".into()],
    );
    let entry = GameEntry::Emulator(emu.clone());

    let shortcut = manager.register_entry(&entry).unwrap();
    assert_eq!(shortcut.app_name, "[EmuSync] Game Boy Advance");
    assert!(shortcut.exe.to_lowercase().contains("powershell.exe"));
    assert!(shortcut.launch_options.contains("-EncodedCommand "));
    assert_eq!(shortcut.start_dir, "C:\\Emulators\\mGBA");

    let key = manager.launcher_key("Game Boy Advance");
    let bat_file = launcher_dir.join(format!("launch_{key}.bat"));
    assert!(bat_file.exists());
    let content = fs::read_to_string(&bat_file).unwrap();
    assert!(content.starts_with("@echo off"));
    assert!(content.contains("setlocal DisableDelayedExpansion"));
    assert!(content.contains("-EncodedCommand "));
    assert!(content.contains("exit /b %ERRORLEVEL%"));
    assert!(content.contains("\r\n"));

    let json_file = launcher_dir.join(format!("launch_{key}.json"));
    assert!(json_file.exists());
    let json_content = fs::read_to_string(&json_file).unwrap();
    let config: WindowsLaunchConfiguration = serde_json::from_str(&json_content).unwrap();
    assert_eq!(config.executable, "C:\\Emulators\\mGBA\\mgba.exe");
}

#[test]
fn normalized_names_unicode_and_case_variations_have_different_stable_paths() {
    let dir = tempdir().unwrap();
    let manager = SteamShortcutManager::new().with_custom_launcher_dir(dir.path().to_path_buf());

    let names = ["Foo-Bar", "Foo Bar", "foo bar", "Foo_Bar", "中文", "日本語"];
    let mut keys = Vec::new();
    let mut signals = Vec::new();

    for name in &names {
        let key = manager.launcher_key(name);
        keys.push(key);

        let emu = Emulator::new(*name, "/bin/true", "/roms", vec![]);
        signals.push(manager.get_signal_file(&emu));
        let _ = manager.create_linux_wrapper_script(&emu).unwrap();
    }

    let unique_keys: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(unique_keys.len(), names.len());

    let unique_signals: std::collections::HashSet<_> = signals.iter().collect();
    assert_eq!(unique_signals.len(), names.len());
}

#[test]
fn legacy_launcher_migration_retains_ids_and_generates_forwarder() {
    let dir = tempdir().unwrap();
    let vdf_file = dir.path().join("shortcuts.vdf");
    let launcher_dir = dir.path().join("launchers");
    fs::create_dir_all(&launcher_dir).unwrap();

    let legacy_script = launcher_dir.join("launch_foo_bar.sh");
    fs::write(&legacy_script, b"#!/bin/bash\nexit 99\n").unwrap();

    let rom_path = dir.path().join("previous.rom");
    fs::write(&rom_path, b"ROM").unwrap();
    fs::write(
        launcher_dir.join(".current_rom_foo_bar"),
        rom_path.to_str().unwrap().as_bytes(),
    )
    .unwrap();

    let old_shortcut = SteamShortcut {
        app_id: 12345,
        app_name: "[EmuSync] Foo-Bar".into(),
        exe: format!("\"{}\"", legacy_script.display()),
        start_dir: dir.path().to_str().unwrap().into(),
        allow_overlay: false,
        ..SteamShortcut::default()
    };
    BinaryVdf::write(&vdf_file, &[old_shortcut]).unwrap();

    let manager = SteamShortcutManager::new()
        .with_custom_shortcuts_file(vdf_file.clone())
        .with_custom_launcher_dir(launcher_dir.clone())
        .with_simulate_windows(false);

    let emu = Emulator::new("Foo-Bar", "/bin/echo", "/roms", vec![]);
    let entry = GameEntry::Emulator(emu.clone());

    let migrated = manager.migrate_legacy_launchers(&[entry]).unwrap();
    assert_eq!(migrated.len(), 1);
    assert_eq!(migrated[0].app_id, 12345);
    assert!(!migrated[0].allow_overlay);

    // Old script should now forward to new launcher script
    let legacy_content = fs::read_to_string(&legacy_script).unwrap();
    assert!(legacy_content.contains("exec "));
    assert!(legacy_content.contains(&manager.launcher_key("Foo-Bar")));

    // Backup .legacy.bak created
    let backup_file = launcher_dir.join("launch_foo_bar.sh.legacy.bak");
    assert!(backup_file.exists());
    assert_eq!(
        fs::read_to_string(&backup_file).unwrap(),
        "#!/bin/bash\nexit 99\n"
    );

    // Signal file copied over
    let new_signal = manager.get_signal_file(&emu);
    assert!(new_signal.exists());
    assert_eq!(
        fs::read_to_string(new_signal).unwrap(),
        rom_path.to_str().unwrap()
    );
}

#[test]
fn ambiguous_old_alias_refuses_to_launch() {
    let dir = tempdir().unwrap();
    let vdf_file = dir.path().join("shortcuts.vdf");
    let launcher_dir = dir.path().join("launchers");
    fs::create_dir_all(&launcher_dir).unwrap();

    let legacy_script = launcher_dir.join("launch_foo_bar.sh");
    fs::write(&legacy_script, b"old shared script").unwrap();
    fs::write(
        launcher_dir.join(".current_rom_foo_bar"),
        b"ambiguous old ROM",
    )
    .unwrap();

    let shortcuts = vec![
        SteamShortcut {
            app_id: 1,
            app_name: "[EmuSync] Foo-Bar".into(),
            exe: format!("\"{}\"", legacy_script.display()),
            ..SteamShortcut::default()
        },
        SteamShortcut {
            app_id: 2,
            app_name: "[EmuSync] Foo Bar".into(),
            exe: format!("\"{}\"", legacy_script.display()),
            ..SteamShortcut::default()
        },
    ];
    BinaryVdf::write(&vdf_file, &shortcuts).unwrap();

    let manager = SteamShortcutManager::new()
        .with_custom_shortcuts_file(vdf_file)
        .with_custom_launcher_dir(launcher_dir.clone())
        .with_simulate_windows(false);

    let entries = [
        GameEntry::Emulator(Emulator::new("Foo-Bar", "/bin/echo", "/roms", vec![])),
        GameEntry::Emulator(Emulator::new("Foo Bar", "/bin/echo", "/roms", vec![])),
    ];

    let migrated = manager.migrate_legacy_launchers(&entries).unwrap();
    assert_eq!(migrated.len(), 2);

    let content = fs::read_to_string(&legacy_script).unwrap();
    assert!(content.contains("Restart Steam to load the separate emulator launchers"));
    assert!(content.contains("exit 1"));
}

#[test]
fn native_re_registration_updates_launch_paths_while_preserving_steam_id_and_preferences() {
    let dir = tempdir().unwrap();
    let vdf_file = dir.path().join("shortcuts.vdf");
    let launcher_dir = dir.path().join("launchers");

    let mut tags = std::collections::BTreeMap::new();
    tags.insert("0".into(), "Favourites".into());

    let shortcut = SteamShortcut {
        app_id: 1234,
        app_name: "[EmuSync] Old".into(),
        exe: "old.exe".into(),
        start_dir: "old-dir".into(),
        allow_overlay: false,
        tags,
        ..SteamShortcut::default()
    };
    BinaryVdf::write(&vdf_file, &[shortcut]).unwrap();

    let manager = SteamShortcutManager::new()
        .with_custom_shortcuts_file(vdf_file.clone())
        .with_custom_launcher_dir(launcher_dir);

    let mut native = NativeGame::default();
    native.common.name = "New".into();
    native.common.executable_path = "/games/new.exe".into();
    native.common.steam_app_id = Some(1234);

    let updated = manager.register_entry(&GameEntry::Native(native)).unwrap();
    assert_eq!(updated.app_id, 1234);
    assert_eq!(updated.exe, "\"/games/new.exe\"");
    assert_eq!(updated.app_name, "[EmuSync] New");
    assert!(!updated.allow_overlay);
    assert_eq!(
        updated.tags.get("0").map(String::as_str),
        Some("Favourites")
    );
}
