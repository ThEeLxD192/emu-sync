//! Adaptador `DesktopSteamLibrary` sobre `SteamShortcutManager`.

mod common;

use common::{emulator_entry, linux_manager, shortcut};
use emusync_domain::{
    execution::SteamLaunchPreparation,
    library::{Emulator, SteamLibrary},
};
use emusync_platform_desktop::{BinaryVdf, DesktopSteamLibrary};
use std::fs;
use tempfile::tempdir;

#[test]
fn migrate_launchers_reports_first_app_id_for_duplicate_names() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let vdf_file = dir.path().join("config/shortcuts.vdf");
    BinaryVdf::write(
        &vdf_file,
        &[
            shortcut(10, "A"),
            shortcut(20, "Foreign"),
            shortcut(30, "A"),
        ],
    )
    .unwrap();
    let library = DesktopSteamLibrary::new(manager);

    let mut reported = library.migrate_launchers(&[]).unwrap();
    reported.sort();

    assert_eq!(
        reported,
        vec![("A".to_string(), 10), ("Foreign".to_string(), 20)]
    );
}

#[test]
fn register_returns_persisted_app_id_and_prepare_writes_signal() {
    let dir = tempdir().unwrap();
    let library = DesktopSteamLibrary::new(linux_manager(dir.path()));
    let entry = emulator_entry("Lib", "/usr/bin/emu");

    let returned = library.register(&entry).unwrap();

    let persisted = library.manager().load_shortcuts().unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(returned, persisted[0].app_id);
    assert!(library.is_registered(&entry).unwrap());

    let emu = Emulator::new("Lib", "/usr/bin/emu", "/roms", vec![]);
    library.prepare(&emu, "/r.iso").unwrap();
    assert_eq!(
        fs::read_to_string(library.manager().get_signal_file(&emu)).unwrap(),
        "/r.iso"
    );
}
