//! Contratos de `SteamShortcutManager` (registro, baja y consulta) sobre `shortcuts.vdf` sintéticos.

mod common;

use common::{dir_entries, emulator_entry, linux_manager, read_bytes, shortcut};
use emusync_domain::library::{Emulator, GameEntry, NativeGame};
use emusync_platform_desktop::{
    BinaryVdf, SteamShortcut, SteamShortcutManager, WindowsLaunchConfiguration,
};
use std::{fs, io::ErrorKind, path::Path};
use tempfile::tempdir;

/// `shortcuts.vdf` con un atajo ajeno cuyo último campo es `field` (tipo 0x07 o UTF-8 inválido).
fn write_raw_vdf(path: &Path, field: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00];
    bytes.extend_from_slice(b"shortcuts\0");
    bytes.push(0x00);
    bytes.extend_from_slice(b"0\0");
    bytes.push(0x01);
    bytes.extend_from_slice(b"AppName\0Foreign\0");
    bytes.extend_from_slice(field);
    bytes.extend_from_slice(&[0x08, 0x08, 0x08]);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, &bytes).unwrap();
    bytes
}

fn launch_scripts(manager: &SteamShortcutManager) -> Vec<String> {
    let dir = manager.get_launcher_dir();
    if !dir.exists() {
        return Vec::new();
    }
    common::dir_entries(&dir)
        .into_iter()
        .filter(|n| n.starts_with("launch_") && n.ends_with(".sh"))
        .collect()
}

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

#[test]
fn re_registration_replaces_by_position_and_keeps_foreign_shortcut_with_same_app_id() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    let foreign = SteamShortcut {
        app_id: 777,
        app_name: "Foreign".into(),
        exe: "\"/opt/foreign\"".into(),
        ..SteamShortcut::default()
    };
    let ours = SteamShortcut {
        app_id: 777,
        app_name: "[EmuSync] Mine".into(),
        exe: "\"/old/launcher\"".into(),
        ..SteamShortcut::default()
    };
    BinaryVdf::write(&vdf_file, &[foreign.clone(), ours]).unwrap();

    let entry = GameEntry::Emulator(Emulator::new("Mine", "/usr/bin/emu", "/roms", vec![]));
    let updated = manager.register_entry(&entry).unwrap();
    assert_eq!(updated.app_id, 777);

    let loaded = manager.load_shortcuts().unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0], foreign);
    assert_eq!(loaded[1].app_name, "[EmuSync] Mine");
    assert_ne!(loaded[1].exe, "\"/old/launcher\"");
}

#[test]
fn is_registered_propagates_corrupt_vdf_errors() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    fs::create_dir_all(vdf_file.parent().unwrap()).unwrap();
    fs::write(&vdf_file, b"not a vdf").unwrap();
    let entry = GameEntry::Emulator(Emulator::new("Any", "/usr/bin/emu", "/roms", vec![]));
    assert!(manager.is_registered(&entry).is_err());
}

#[test]
fn unmatched_fullscreen_quotes_are_rejected_and_nothing_is_registered() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let mut emu = Emulator::new("Quotes", "/usr/bin/emu", "/roms", vec![]);
    emu.fullscreen_args = Some("\"unterminated".into());

    let err = manager.create_linux_wrapper_script(&emu).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        "Fullscreen arguments contain unmatched quotes"
    );
    assert!(manager.create_windows_wrapper_script(&emu).is_err());
    assert!(manager.register_entry(&GameEntry::Emulator(emu)).is_err());
    assert!(manager.load_shortcuts().unwrap().is_empty());
}

#[test]
fn register_refuses_to_rewrite_vdf_with_unknown_type_marker() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    // 0x07 (uint64) no está soportado: el archivo no se puede parsear completo.
    let mut field = vec![0x07];
    field.extend_from_slice(b"big\0");
    field.extend_from_slice(&[0u8; 8]);
    let original = write_raw_vdf(&vdf_file, &field);

    let err = manager
        .register_entry(&emulator_entry("Any", "/usr/bin/emu"))
        .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::InvalidData);
    assert_eq!(read_bytes(&vdf_file), original);
    assert!(launch_scripts(&manager).is_empty());
    assert!(!manager.get_launcher_dir().exists());
}

#[test]
fn register_with_invalid_utf8_string_fails_without_modifying_vdf() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    let mut field = vec![0x01];
    field.extend_from_slice(b"Exe\0");
    field.extend_from_slice(&[0xff, 0xfe, 0x00]);
    let original = write_raw_vdf(&vdf_file, &field);

    let err = manager
        .register_entry(&emulator_entry("Any", "/usr/bin/emu"))
        .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::InvalidData);
    assert_eq!(read_bytes(&vdf_file), original);
    assert!(launch_scripts(&manager).is_empty());
}

#[test]
fn register_aborts_without_writing_when_backup_cannot_be_created() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    BinaryVdf::write(&vdf_file, &[shortcut(5, "[EmuSync] Old")]).unwrap();
    let original = read_bytes(&vdf_file);
    // Un directorio en la ruta del `.bak` hace fallar `fs::copy`.
    fs::create_dir(dir.path().join("config/shortcuts.vdf.bak")).unwrap();

    let result = manager.register_entry(&emulator_entry("Old", "/usr/bin/emu"));

    assert!(result.is_err());
    assert_eq!(read_bytes(&vdf_file), original);
    assert!(launch_scripts(&manager).is_empty());
}

#[test]
fn unregister_aborts_without_writing_when_backup_cannot_be_created() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    BinaryVdf::write(&vdf_file, &[shortcut(5, "[EmuSync] Old")]).unwrap();
    let original = read_bytes(&vdf_file);
    fs::create_dir(dir.path().join("config/shortcuts.vdf.bak")).unwrap();

    let result = manager.unregister_entry(&emulator_entry("Old", "/usr/bin/emu"));

    assert!(result.is_err());
    assert_eq!(read_bytes(&vdf_file), original);
}

#[test]
fn unregister_without_shortcuts_file_creates_nothing() {
    let dir = tempdir().unwrap();
    let missing_dir = dir.path().join("nested/config");
    let vdf_file = missing_dir.join("shortcuts.vdf");
    let manager = SteamShortcutManager::new()
        .with_custom_shortcuts_file(vdf_file.clone())
        .with_custom_launcher_dir(dir.path().join("launchers"))
        .with_simulate_windows(false);

    manager
        .unregister_entry(&emulator_entry("Gone", "/usr/bin/emu"))
        .unwrap();

    assert!(!vdf_file.exists());
    assert!(!missing_dir.join("shortcuts.vdf.bak").exists());
    assert!(!missing_dir.exists());
    assert_eq!(dir_entries(dir.path()), Vec::<String>::new());
}

#[test]
fn unregister_removes_by_prefixed_name_and_by_app_id_and_keeps_unrelated() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    BinaryVdf::write(
        &vdf_file,
        &[
            shortcut(1, "[EmuSync] Target"),
            shortcut(9, "[EmuSync] Renamed"),
            shortcut(3, "Target"),
            shortcut(4, "[EmuSync] Other"),
        ],
    )
    .unwrap();
    let before = read_bytes(&vdf_file);

    let mut emu = Emulator::new("Target", "/usr/bin/emu", "/roms", vec![]);
    emu.common.steam_app_id = Some(9);
    manager.unregister_entry(&GameEntry::Emulator(emu)).unwrap();

    let remaining: Vec<(String, i32)> = manager
        .load_shortcuts()
        .unwrap()
        .into_iter()
        .map(|s| (s.app_name, s.app_id))
        .collect();
    assert_eq!(
        remaining,
        vec![
            ("Target".to_string(), 3),
            ("[EmuSync] Other".to_string(), 4)
        ]
    );
    assert_eq!(
        read_bytes(&dir.path().join("config/shortcuts.vdf.bak")),
        before
    );
}

#[test]
fn unregister_without_app_id_only_removes_exact_prefixed_name() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    BinaryVdf::write(
        &vdf_file,
        &[
            shortcut(1, "[EmuSync] Mine"),
            shortcut(2, "[EmuSync] Mine 2"),
        ],
    )
    .unwrap();

    manager
        .unregister_entry(&emulator_entry("Mine", "/usr/bin/emu"))
        .unwrap();

    let remaining = manager.load_shortcuts().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].app_name, "[EmuSync] Mine 2");
    assert_eq!(remaining[0].app_id, 2);
}

#[test]
fn register_backup_contains_exact_pre_modification_bytes() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    let foreign = shortcut(3, "Foreign");
    BinaryVdf::write(&vdf_file, std::slice::from_ref(&foreign)).unwrap();
    let before = read_bytes(&vdf_file);

    manager
        .register_entry(&emulator_entry("New", "/usr/bin/emu"))
        .unwrap();

    assert_eq!(
        read_bytes(&dir.path().join("config/shortcuts.vdf.bak")),
        before
    );
    let after = manager.load_shortcuts().unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], foreign);
    assert_eq!(after[1].app_name, "[EmuSync] New");
}

#[cfg(unix)]
#[test]
fn reregistering_unchanged_emulator_does_not_replace_vdf_file() {
    use std::os::unix::fs::MetadataExt;
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    let entry = emulator_entry("Stable", "/usr/bin/emu");

    let first = manager.register_entry(&entry).unwrap();
    let inode = fs::metadata(&vdf_file).unwrap().ino();
    let bytes = read_bytes(&vdf_file);
    let second = manager.register_entry(&entry).unwrap();

    assert_eq!(second.app_id, first.app_id);
    assert_eq!(fs::metadata(&vdf_file).unwrap().ino(), inode);
    assert_eq!(read_bytes(&vdf_file), bytes);
}

#[test]
fn new_shortcut_app_id_is_negative_and_distinct_from_existing_ids() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    let mut existing: Vec<SteamShortcut> = (1001..=1050)
        .map(|n| shortcut(-n, &format!("Negative {n}")))
        .collect();
    existing.extend((1..=50).map(|n| shortcut(n, &format!("Positive {n}"))));
    BinaryVdf::write(&vdf_file, &existing).unwrap();

    let created = manager
        .register_entry(&emulator_entry("Fresh", "/usr/bin/emu"))
        .unwrap();

    assert!(created.app_id <= -1000, "app_id = {}", created.app_id);
    assert!(existing.iter().all(|s| s.app_id != created.app_id));
    let loaded = manager.load_shortcuts().unwrap();
    assert_eq!(loaded.len(), 101);
    assert_eq!(&loaded[..100], existing.as_slice());
}

#[test]
fn is_registered_matches_by_app_id_after_rename_and_rejects_unrelated() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    BinaryVdf::write(&vdf_file, &[shortcut(77, "[EmuSync] Old")]).unwrap();

    let mut renamed = Emulator::new("New", "/usr/bin/emu", "/roms", vec![]);
    renamed.common.steam_app_id = Some(77);
    assert!(
        manager
            .is_registered(&GameEntry::Emulator(renamed))
            .unwrap()
    );
    assert!(
        !manager
            .is_registered(&emulator_entry("New", "/usr/bin/emu"))
            .unwrap()
    );

    let other = tempdir().unwrap();
    let without_file = linux_manager(other.path());
    assert!(
        !without_file
            .is_registered(&emulator_entry("New", "/usr/bin/emu"))
            .unwrap()
    );
}

#[test]
fn prepare_launch_writes_rom_path_to_signal_file_creating_launcher_dir() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    assert!(!manager.get_launcher_dir().exists());
    let emu = Emulator::new("Signal", "/usr/bin/emu", "/roms", vec![]);

    manager
        .prepare_launch(&emu, Path::new("/roms/a b'$(x).iso"))
        .unwrap();

    assert_eq!(
        fs::read_to_string(manager.get_signal_file(&emu)).unwrap(),
        "/roms/a b'$(x).iso"
    );
}
