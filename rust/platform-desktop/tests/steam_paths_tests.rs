//! Claves de launcher, señales de ROM y localización de `shortcuts.vdf` por cuenta de Steam.

mod common;

use common::linux_manager;
use emusync_domain::library::{Emulator, SteamLibrary};
use emusync_platform_desktop::{DesktopSteamLibrary, SteamShortcutManager};
use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};
use tempfile::tempdir;

fn set_mtime(dir: &Path, seconds: u64) {
    // `File::open` cannot open a directory on Windows; filetime handles both platforms.
    filetime::set_file_mtime(
        dir,
        filetime::FileTime::from_system_time(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)),
    )
    .unwrap();
}

fn userdata_manager(userdata: &Path) -> SteamShortcutManager {
    SteamShortcutManager::new()
        .with_custom_userdata_dir(userdata.to_path_buf())
        .with_simulate_windows(false)
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
fn launcher_key_matches_stable_literal_for_existing_installations() {
    let manager = SteamShortcutManager::new();
    // Si la clave cambiara, quedarían huérfanos los lanzadores y señales ya instalados.
    assert_eq!(
        manager.launcher_key("Game Boy Advance"),
        "game_boy_advance-05a55c421654c3b07a7ef2920a7f2f75ec6f0658e43f9ba003414343e6a5a006"
    );
}

#[test]
fn launcher_key_of_symbol_only_name_uses_emulator_fallback() {
    let manager = SteamShortcutManager::new();
    assert_eq!(
        manager.launcher_key("!!!"),
        "emulator-e84c538e7fe250730ef62de220c40dfa808d3008c0cdb437181564b88b8714b8"
    );
}

#[test]
fn launcher_key_truncates_readable_part_to_40_ascii_chars_without_panicking_on_unicode() {
    let manager = SteamShortcutManager::new();
    let name = "Ñ".repeat(10) + &"a".repeat(60);

    let key = manager.launcher_key(&name);

    let prefix = "a".repeat(40) + "-";
    assert!(key.starts_with(&prefix), "{key}");
    assert_eq!(key.len(), 40 + 1 + 64);
    assert!(
        key[41..]
            .chars()
            .all(|c| matches!(c, '0'..='9' | 'a'..='f')),
        "{key}"
    );
}

#[test]
fn signal_file_lives_in_launcher_dir_with_hidden_key_name() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let emu = Emulator::new("Signal Name", "/usr/bin/emu", "/roms", vec![]);

    assert_eq!(
        manager.get_signal_file(&emu),
        manager.get_launcher_dir().join(format!(
            ".current_rom_{}",
            manager.launcher_key("Signal Name")
        ))
    );
}

#[test]
fn shortcuts_file_prefers_most_recent_account_with_config_and_ignores_account_zero() {
    let dir = tempdir().unwrap();
    let userdata = dir.path().join("userdata");
    fs::create_dir_all(userdata.join("0/config")).unwrap();
    fs::create_dir_all(userdata.join("111/config")).unwrap();
    fs::create_dir_all(userdata.join("222")).unwrap();
    fs::write(userdata.join("333"), b"not an account").unwrap();
    // Los mtimes se fijan al final: crear un hijo actualiza el mtime del padre.
    set_mtime(&userdata.join("111"), 1000);
    set_mtime(&userdata.join("222"), 2000);
    set_mtime(&userdata.join("0"), 3000);

    assert_eq!(
        userdata_manager(&userdata).find_shortcuts_file(),
        Some(userdata.join("111/config/shortcuts.vdf"))
    );
}

#[test]
fn shortcuts_file_falls_back_to_newest_account_when_none_has_config() {
    let dir = tempdir().unwrap();
    let userdata = dir.path().join("userdata");
    fs::create_dir_all(userdata.join("111")).unwrap();
    fs::create_dir_all(userdata.join("222")).unwrap();
    set_mtime(&userdata.join("111"), 1000);
    set_mtime(&userdata.join("222"), 2000);

    assert_eq!(
        userdata_manager(&userdata).find_shortcuts_file(),
        Some(userdata.join("222/config/shortcuts.vdf"))
    );
}

#[test]
fn shortcuts_file_is_none_when_userdata_missing_or_without_accounts() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("missing");
    let empty = dir.path().join("empty");
    let only_zero = dir.path().join("only_zero");
    fs::create_dir_all(&empty).unwrap();
    fs::create_dir_all(only_zero.join("0/config")).unwrap();

    for userdata in [&missing, &empty, &only_zero] {
        let manager = userdata_manager(userdata);
        assert_eq!(manager.find_shortcuts_file(), None, "{userdata:?}");
        assert!(!DesktopSteamLibrary::new(manager).is_available().unwrap());
    }
    assert!(!missing.exists(), "the lookup must not create directories");
}
