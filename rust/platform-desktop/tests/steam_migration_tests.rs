//! Migración de launchers legacy y puentes de renombrado (`migrate_legacy_launchers`, `bridge.rs`).

mod common;

use common::{linux_manager, read_bytes, shortcut};
use emusync_domain::library::{Emulator, GameEntry};
use emusync_platform_desktop::{BinaryVdf, SteamShortcut, SteamShortcutManager};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::tempdir;

/// Manager con VDF/launchers en un directorio plano, como en las migraciones históricas.
fn flat_manager(dir: &Path) -> (SteamShortcutManager, PathBuf, PathBuf) {
    let vdf_file = dir.join("shortcuts.vdf");
    let launcher_dir = dir.join("launchers");
    let manager = SteamShortcutManager::new()
        .with_custom_shortcuts_file(vdf_file.clone())
        .with_custom_launcher_dir(launcher_dir.clone())
        .with_simulate_windows(false);
    (manager, vdf_file, launcher_dir)
}

/// Un único atajo legacy `[EmuSync] Foo-Bar` con su script `launch_foo_bar.sh` y señal antigua.
fn legacy_setup(dir: &Path) -> (SteamShortcutManager, PathBuf, PathBuf, PathBuf) {
    let (manager, vdf_file, launcher_dir) = flat_manager(dir);
    fs::create_dir_all(&launcher_dir).unwrap();

    let legacy_script = launcher_dir.join("launch_foo_bar.sh");
    fs::write(&legacy_script, b"#!/bin/bash\nexit 99\n").unwrap();

    let rom_path = dir.join("previous.rom");
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
        start_dir: dir.to_str().unwrap().into(),
        allow_overlay: false,
        ..SteamShortcut::default()
    };
    BinaryVdf::write(&vdf_file, &[old_shortcut]).unwrap();
    (manager, vdf_file, legacy_script, rom_path)
}

/// Dos atajos (`Foo-Bar` y `Foo Bar`) que comparten el mismo alias legacy `foo_bar`.
fn ambiguous_setup(dir: &Path) -> (SteamShortcutManager, PathBuf, [GameEntry; 2]) {
    let (manager, vdf_file, launcher_dir) = flat_manager(dir);
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

    let entries = [
        GameEntry::Emulator(Emulator::new("Foo-Bar", "/bin/echo", "/roms", vec![])),
        GameEntry::Emulator(Emulator::new("Foo Bar", "/bin/echo", "/roms", vec![])),
    ];
    (manager, legacy_script, entries)
}

#[test]
fn legacy_launcher_migration_retains_ids_and_generates_forwarder() {
    let dir = tempdir().unwrap();
    let (manager, _vdf_file, legacy_script, rom_path) = legacy_setup(dir.path());
    let launcher_dir = manager.get_launcher_dir();

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
    let (manager, legacy_script, entries) = ambiguous_setup(dir.path());

    let migrated = manager.migrate_legacy_launchers(&entries).unwrap();
    assert_eq!(migrated.len(), 2);

    let content = fs::read_to_string(&legacy_script).unwrap();
    assert!(content.contains("Restart Steam to load the separate emulator launchers"));
    assert!(content.contains("exit 1"));
}

#[test]
fn invalid_emulator_arguments_do_not_disturb_other_entries_during_migration() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    BinaryVdf::write(
        &vdf_file,
        &[
            shortcut(11, "[EmuSync] Broken"),
            shortcut(22, "[EmuSync] Good"),
        ],
    )
    .unwrap();

    let mut broken = Emulator::new("Broken", "/usr/bin/emu", "/roms", vec![]);
    broken.fullscreen_args = Some("\"unterminated".into());
    let good = Emulator::new("Good", "/usr/bin/emu", "/roms", vec![]);
    let entries = [GameEntry::Emulator(broken), GameEntry::Emulator(good)];

    let migrated = manager.migrate_legacy_launchers(&entries).unwrap();

    assert_eq!(migrated.len(), 2);
    assert_eq!(migrated[0].app_id, 11);
    assert_eq!(migrated[0].exe, "\"/old/launcher\"");
    assert_eq!(migrated[1].app_id, 22);
    assert_ne!(migrated[1].exe, "\"/old/launcher\"");
    let persisted = manager.load_shortcuts().unwrap();
    assert_eq!(persisted[0].exe, "\"/old/launcher\"");
    assert_eq!(persisted[1].app_id, 22);
}

#[test]
fn migration_with_corrupt_vdf_returns_error_and_creates_no_launchers() {
    let dir = tempdir().unwrap();
    let (manager, vdf_file, launcher_dir) = flat_manager(dir.path());
    fs::write(&vdf_file, b"garbage").unwrap();

    let entries = [GameEntry::Emulator(Emulator::new(
        "Any",
        "/usr/bin/emu",
        "/roms",
        vec![],
    ))];
    let result = manager.migrate_legacy_launchers(&entries);

    assert!(result.is_err());
    assert_eq!(read_bytes(&vdf_file), b"garbage");
    assert!(!launcher_dir.exists());
    assert!(!dir.path().join("shortcuts.vdf.bak").exists());
}

#[test]
fn migration_when_backup_fails_returns_original_list_and_keeps_vdf_bytes() {
    let dir = tempdir().unwrap();
    let (manager, vdf_file, _launcher_dir) = flat_manager(dir.path());
    BinaryVdf::write(
        &vdf_file,
        &[SteamShortcut {
            app_id: 7,
            app_name: "[EmuSync] Emu".into(),
            exe: "\"/old\"".into(),
            ..SteamShortcut::default()
        }],
    )
    .unwrap();
    let original = read_bytes(&vdf_file);
    fs::create_dir(dir.path().join("shortcuts.vdf.bak")).unwrap();

    let entries = [GameEntry::Emulator(Emulator::new(
        "Emu",
        "/usr/bin/emu",
        "/roms",
        vec![],
    ))];
    let list = manager.migrate_legacy_launchers(&entries).unwrap();

    assert_eq!(list.len(), 1);
    assert_eq!(list[0].exe, "\"/old\"");
    assert_eq!(read_bytes(&vdf_file), original);
}

#[cfg(unix)]
#[test]
fn second_migration_is_noop_and_preserves_original_backup() {
    use std::os::unix::fs::MetadataExt;
    let dir = tempdir().unwrap();
    let (manager, vdf_file, _script, _rom) = legacy_setup(dir.path());
    let before_first = read_bytes(&vdf_file);
    let entries = [GameEntry::Emulator(Emulator::new(
        "Foo-Bar",
        "/bin/echo",
        "/roms",
        vec![],
    ))];

    manager.migrate_legacy_launchers(&entries).unwrap();
    let after_first = read_bytes(&vdf_file);
    assert_ne!(
        after_first, before_first,
        "first migration must rewrite the exe"
    );
    let backup = dir.path().join("shortcuts.vdf.bak");
    assert_eq!(read_bytes(&backup), before_first);
    let inode = fs::metadata(&vdf_file).unwrap().ino();

    manager.migrate_legacy_launchers(&entries).unwrap();

    assert_eq!(fs::metadata(&vdf_file).unwrap().ino(), inode);
    assert_eq!(read_bytes(&vdf_file), after_first);
    assert_eq!(read_bytes(&backup), before_first);
}

#[cfg(unix)]
#[test]
fn migration_with_no_matching_shortcuts_neither_writes_nor_backs_up() {
    use std::os::unix::fs::MetadataExt;
    let dir = tempdir().unwrap();
    let (manager, vdf_file, _launcher_dir) = flat_manager(dir.path());
    BinaryVdf::write(&vdf_file, &[shortcut(1, "Foreign"), shortcut(2, "Other")]).unwrap();
    let original = read_bytes(&vdf_file);
    let inode = fs::metadata(&vdf_file).unwrap().ino();

    let entries = [GameEntry::Emulator(Emulator::new(
        "Unrelated",
        "/usr/bin/emu",
        "/roms",
        vec![],
    ))];
    let list = manager.migrate_legacy_launchers(&entries).unwrap();

    assert_eq!(list.len(), 2);
    assert!(!dir.path().join("shortcuts.vdf.bak").exists());
    assert_eq!(fs::metadata(&vdf_file).unwrap().ino(), inode);
    assert_eq!(read_bytes(&vdf_file), original);
}

#[test]
fn legacy_backup_is_never_overwritten_by_repeated_migrations() {
    let dir = tempdir().unwrap();
    let (manager, _vdf_file, legacy_script, _rom) = legacy_setup(dir.path());
    let entries = [GameEntry::Emulator(Emulator::new(
        "Foo-Bar",
        "/bin/echo",
        "/roms",
        vec![],
    ))];

    manager.migrate_legacy_launchers(&entries).unwrap();
    manager.migrate_legacy_launchers(&entries).unwrap();

    let backup = manager
        .get_launcher_dir()
        .join("launch_foo_bar.sh.legacy.bak");
    let backup_text = fs::read_to_string(backup).unwrap();
    assert_eq!(backup_text, "#!/bin/bash\nexit 99\n");
    assert!(!backup_text.contains("exec"));
    assert!(fs::read_to_string(legacy_script).unwrap().contains("exec "));
}

#[test]
fn ambiguous_legacy_signal_is_not_copied_to_any_new_signal() {
    let dir = tempdir().unwrap();
    let (manager, _legacy_script, entries) = ambiguous_setup(dir.path());

    manager.migrate_legacy_launchers(&entries).unwrap();

    for entry in &entries {
        let GameEntry::Emulator(emu) = entry else {
            unreachable!()
        };
        assert!(
            !manager.get_signal_file(emu).exists(),
            "an ambiguous signal could launch the wrong ROM for {}",
            emu.common.name
        );
    }
}

#[test]
fn legacy_signal_does_not_overwrite_existing_new_signal() {
    let dir = tempdir().unwrap();
    let (manager, _vdf_file, _script, _rom) = legacy_setup(dir.path());
    let emu = Emulator::new("Foo-Bar", "/bin/echo", "/roms", vec![]);
    let new_signal = manager.get_signal_file(&emu);
    fs::write(&new_signal, b"new.rom").unwrap();
    fs::write(
        manager.get_launcher_dir().join(".current_rom_foo_bar"),
        b"old.rom",
    )
    .unwrap();

    manager
        .migrate_legacy_launchers(&[GameEntry::Emulator(emu)])
        .unwrap();

    assert_eq!(fs::read_to_string(new_signal).unwrap(), "new.rom");
}

/// `Old` registrado con su señal y un `New` que reutiliza su `steam_app_id` y ejecutable.
#[cfg(target_os = "linux")]
struct RenameFixture {
    manager: SteamShortcutManager,
    old: Emulator,
    new: Emulator,
    rom: PathBuf,
    out: PathBuf,
    app_id: i32,
    root: PathBuf,
}

#[cfg(target_os = "linux")]
fn rename_fixture(root: &Path) -> RenameFixture {
    let manager = linux_manager(root);
    let out = root.join("argv.out");
    let exe = root.join("bin/rec.sh");
    common::write_script(
        &exe,
        &format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\0' \"$a\"; done > '{}'\n",
            out.display()
        ),
    );
    let rom = root.join("game.rom");
    fs::write(&rom, b"ROM").unwrap();

    let old = Emulator::new("Old", exe.to_str().unwrap(), "/roms", vec![]);
    let registered = manager
        .register_entry(&GameEntry::Emulator(old.clone()))
        .unwrap();
    manager.prepare_launch(&old, &rom).unwrap();

    let mut new = Emulator::new("New", exe.to_str().unwrap(), "/roms", vec![]);
    new.common.steam_app_id = Some(registered.app_id);
    RenameFixture {
        manager,
        old,
        new,
        rom,
        out,
        app_id: registered.app_id,
        root: root.to_path_buf(),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn renaming_registered_emulator_bridges_previous_launcher_and_carries_signal() {
    let dir = tempdir().unwrap();
    let fx = rename_fixture(dir.path());
    let launcher_dir = fx.manager.get_launcher_dir();
    let old_script = launcher_dir.join(format!("launch_{}.sh", fx.manager.launcher_key("Old")));
    let new_script_name = format!("launch_{}.sh", fx.manager.launcher_key("New"));

    let updated = fx
        .manager
        .register_entry(&GameEntry::Emulator(fx.new.clone()))
        .unwrap();

    assert_eq!(updated.app_id, fx.app_id);
    let persisted = fx.manager.load_shortcuts().unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].app_name, "[EmuSync] New");
    let bridge = fs::read_to_string(&old_script).unwrap();
    assert!(bridge.contains("exec"), "{bridge}");
    assert!(bridge.contains(&new_script_name), "{bridge}");
    assert_eq!(
        fs::read_to_string(fx.manager.get_signal_file(&fx.new)).unwrap(),
        fx.rom.to_str().unwrap()
    );

    let output = common::run_bash(&old_script, &fx.root);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read(&fx.out).unwrap(),
        [fx.rom.to_str().unwrap().as_bytes(), b"\0"].concat()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn rename_does_not_overwrite_newer_signal_of_new_name() {
    let dir = tempdir().unwrap();
    let fx = rename_fixture(dir.path());
    let new_signal = fx.manager.get_signal_file(&fx.new);
    fs::write(&new_signal, b"newer.rom").unwrap();
    assert_ne!(fx.manager.get_signal_file(&fx.old), new_signal);

    fx.manager
        .register_entry(&GameEntry::Emulator(fx.new.clone()))
        .unwrap();

    assert_eq!(fs::read_to_string(new_signal).unwrap(), "newer.rom");
    assert_eq!(
        fs::read_to_string(fx.manager.get_signal_file(&fx.old)).unwrap(),
        fx.rom.to_str().unwrap()
    );
}
