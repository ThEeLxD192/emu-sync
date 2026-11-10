//! `DesktopUpdateInstaller`: instalación por plataforma, relanzamiento y salida ordenada.

use emusync_domain::update::UpdateInstaller;
use emusync_platform_desktop::DesktopUpdateInstaller;
use std::{
    fs, io,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tempfile::tempdir;

type Log = Arc<Mutex<Vec<String>>>;

fn log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

/// Instalador Linux cuyo relanzamiento y salida se anotan en `log`; el contenido del
/// AppImage en el momento del relanzamiento se guarda en `seen_at_launch`.
fn linux_installer(
    current: &Path,
    log: &Log,
    seen_at_launch: &Arc<Mutex<Option<String>>>,
    game_mode: bool,
) -> DesktopUpdateInstaller {
    let (launch_log, exit_log) = (Arc::clone(log), Arc::clone(log));
    let (seen, current_for_launch) = (Arc::clone(seen_at_launch), current.to_path_buf());
    DesktopUpdateInstaller::new()
        .with_is_windows(false)
        .with_steam_game_mode(game_mode)
        .with_current_app_image_path(Some(current.to_path_buf()))
        .with_linux_launcher(move |_| {
            *seen.lock().unwrap() = fs::read_to_string(&current_for_launch).ok();
            launch_log.lock().unwrap().push("launch".into());
            Ok(())
        })
        .with_exit_application(move |code| exit_log.lock().unwrap().push(format!("exit:{code}")))
}

fn linux_files(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let current = dir.join("EmuSync.AppImage");
    let update = dir.join("update.AppImage");
    fs::write(&current, b"OLD").unwrap();
    fs::write(&update, b"NEW").unwrap();
    (current, update)
}

#[test]
fn apply_windows_update_and_restart_launches_msiexec_for_msi() {
    let dir = tempdir().unwrap();
    let msi_file = dir.path().join("EmuSync-Setup.msi");
    fs::write(&msi_file, b"").unwrap();

    let executed_command = Arc::new(Mutex::new(Vec::new()));
    let executed_clone = executed_command.clone();

    let installer = DesktopUpdateInstaller::new()
        .with_is_windows(true)
        .with_windows_launcher(move |cmd| {
            *executed_clone.lock().unwrap() = cmd.iter().map(|s| s.to_string()).collect();
            Ok(())
        });

    let success = installer
        .apply_windows_update_and_restart(&msi_file)
        .unwrap();
    assert!(success);

    let recorded = executed_command.lock().unwrap().clone();
    assert_eq!(recorded, vec!["msiexec", "/i", msi_file.to_str().unwrap()]);
}

#[test]
fn apply_windows_update_and_restart_launches_exe_directly() {
    let dir = tempdir().unwrap();
    let exe_file = dir.path().join("EmuSync-Setup.exe");
    fs::write(&exe_file, b"").unwrap();

    let executed_command = Arc::new(Mutex::new(Vec::new()));
    let executed_clone = executed_command.clone();

    let installer = DesktopUpdateInstaller::new()
        .with_is_windows(true)
        .with_windows_launcher(move |cmd| {
            *executed_clone.lock().unwrap() = cmd.iter().map(|s| s.to_string()).collect();
            Ok(())
        });

    let success = installer
        .apply_windows_update_and_restart(&exe_file)
        .unwrap();
    assert!(success);

    let recorded = executed_command.lock().unwrap().clone();
    assert_eq!(recorded, vec![exe_file.to_str().unwrap()]);
}

#[test]
fn apply_windows_update_and_restart_returns_false_for_non_installer_files() {
    let dir = tempdir().unwrap();
    let txt_file = dir.path().join("readme.txt");
    fs::write(&txt_file, b"").unwrap();

    let installer = DesktopUpdateInstaller::new().with_is_windows(true);
    let success = installer
        .apply_windows_update_and_restart(&txt_file)
        .unwrap();
    assert!(!success);
}

#[test]
fn apply_linux_update_and_restart_replaces_appimage_and_calls_launcher() {
    let dir = tempdir().unwrap();
    let current_appimage = dir.path().join("EmuSync.AppImage");
    let downloaded_update = dir.path().join("update.AppImage");

    fs::write(&current_appimage, b"OLD_VERSION").unwrap();
    fs::write(&downloaded_update, b"NEW_VERSION").unwrap();

    let launched_path = Arc::new(Mutex::new(String::new()));
    let launched_clone = launched_path.clone();

    let installer = DesktopUpdateInstaller::new()
        .with_is_windows(false)
        .with_steam_game_mode(false)
        .with_current_app_image_path(Some(current_appimage.clone()))
        .with_linux_launcher(move |path| {
            *launched_clone.lock().unwrap() = path.to_string();
            Ok(())
        });

    let success = installer
        .apply_linux_update_and_restart(&downloaded_update)
        .unwrap();
    assert!(success);
    assert_eq!(
        *launched_path.lock().unwrap(),
        current_appimage.to_str().unwrap()
    );
    assert_eq!(
        fs::read_to_string(&current_appimage).unwrap(),
        "NEW_VERSION"
    );
    assert!(!downloaded_update.exists());
}

#[test]
fn apply_linux_update_in_steam_game_mode_does_not_spawn_separate_process() {
    let dir = tempdir().unwrap();
    let current_appimage = dir.path().join("EmuSync.AppImage");
    let downloaded_update = dir.path().join("update.AppImage");

    fs::write(&current_appimage, b"OLD_VERSION").unwrap();
    fs::write(&downloaded_update, b"NEW_VERSION").unwrap();

    let spawned = Arc::new(Mutex::new(false));
    let spawned_clone = spawned.clone();

    let installer = DesktopUpdateInstaller::new()
        .with_is_windows(false)
        .with_steam_game_mode(true) // Gaming Mode active
        .with_current_app_image_path(Some(current_appimage.clone()))
        .with_linux_launcher(move |_| {
            *spawned_clone.lock().unwrap() = true;
            Ok(())
        });

    let success = installer
        .apply_linux_update_and_restart(&downloaded_update)
        .unwrap();
    assert!(success);
    // In Steam Game Mode, separate launcher process is NOT spawned!
    assert!(!*spawned.lock().unwrap());
    assert_eq!(
        fs::read_to_string(&current_appimage).unwrap(),
        "NEW_VERSION"
    );
}

#[test]
fn apply_update_propagates_launcher_and_replace_errors() {
    let dir = tempdir().unwrap();
    let exe_file = dir.path().join("EmuSync-Setup.exe");
    fs::write(&exe_file, b"").unwrap();
    let installer = DesktopUpdateInstaller::new()
        .with_is_windows(true)
        .with_exit_application(|_| panic!("must not exit on failure"))
        .with_windows_launcher(|_| Err(io::Error::other("launcher failed")));
    let err = installer.apply_update_and_restart(&exe_file).unwrap_err();
    assert_eq!(err.to_string(), "launcher failed");

    // Un fallo de instalación en Linux no es "no aplica".
    let current = dir.path().join("EmuSync.AppImage");
    let update = dir.path().join("update.AppImage");
    fs::write(&current, b"OLD").unwrap();
    fs::write(&update, b"NEW").unwrap();
    let installer = DesktopUpdateInstaller::new()
        .with_is_windows(false)
        .with_steam_game_mode(false)
        .with_current_app_image_path(Some(current.clone()))
        .with_exit_application(|_| panic!("must not exit on failure"))
        .with_linux_launcher(|_| Err(io::Error::other("spawn failed")));
    assert!(installer.apply_linux_update_and_restart(&update).is_err());
    assert_eq!(fs::read_to_string(&current).unwrap(), "OLD");
}

#[test]
fn apply_update_reports_not_applicable_as_ok_false() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("missing.AppImage");
    let installer = DesktopUpdateInstaller::new();
    assert!(!installer.apply_update_and_restart(&missing).unwrap());

    let update = dir.path().join("update.AppImage");
    fs::write(&update, b"NEW").unwrap();
    let installer = DesktopUpdateInstaller::new()
        .with_is_windows(false)
        .with_current_app_image_path(None);
    assert!(!installer.apply_linux_update_and_restart(&update).unwrap());
}

#[test]
fn successful_linux_update_exits_once_with_zero_after_replace_and_relaunch() {
    let dir = tempdir().unwrap();
    let (current, update) = linux_files(dir.path());
    let (log, seen) = (log(), Arc::new(Mutex::new(None)));
    let installer = linux_installer(&current, &log, &seen, false);

    assert!(installer.apply_update_and_restart(&update).unwrap());

    assert_eq!(entries(&log), ["launch", "exit:0"]);
    assert_eq!(seen.lock().unwrap().as_deref(), Some("NEW"));
    assert_eq!(fs::read_to_string(&current).unwrap(), "NEW");
    assert!(!update.exists());
}

#[test]
fn game_mode_update_exits_without_relaunching() {
    let dir = tempdir().unwrap();
    let (current, update) = linux_files(dir.path());
    let (log, seen) = (log(), Arc::new(Mutex::new(None)));
    let installer = linux_installer(&current, &log, &seen, true);

    assert!(installer.apply_update_and_restart(&update).unwrap());

    assert_eq!(entries(&log), ["exit:0"]);
    assert_eq!(*seen.lock().unwrap(), None);
    assert_eq!(fs::read_to_string(&current).unwrap(), "NEW");
}

#[test]
fn windows_installer_exit_follows_launcher_and_uppercase_extensions_are_recognized() {
    let dir = tempdir().unwrap();
    let msi = dir.path().join("SETUP.MSI");
    let exe = dir.path().join("SETUP.EXE");
    fs::write(&msi, b"").unwrap();
    fs::write(&exe, b"").unwrap();

    for (file, expected_launch) in [
        (&msi, format!("launch:msiexec /i {}", msi.display())),
        (&exe, format!("launch:{}", exe.display())),
    ] {
        let log = log();
        let (launch_log, exit_log) = (Arc::clone(&log), Arc::clone(&log));
        let installer = DesktopUpdateInstaller::new()
            .with_is_windows(true)
            .with_windows_launcher(move |cmd| {
                launch_log
                    .lock()
                    .unwrap()
                    .push(format!("launch:{}", cmd.join(" ")));
                Ok(())
            })
            .with_exit_application(move |code| {
                exit_log.lock().unwrap().push(format!("exit:{code}"))
            });

        assert!(installer.apply_update_and_restart(file).unwrap());

        assert_eq!(entries(&log), [expected_launch, "exit:0".to_string()]);
    }
}

#[test]
fn not_applicable_updates_never_exit() {
    let dir = tempdir().unwrap();
    let exits = Arc::new(AtomicUsize::new(0));
    let counting = |installer: DesktopUpdateInstaller| {
        let exits = Arc::clone(&exits);
        installer.with_exit_application(move |_| {
            exits.fetch_add(1, Ordering::SeqCst);
        })
    };

    let txt = dir.path().join("readme.txt");
    fs::write(&txt, b"text").unwrap();
    let windows = counting(DesktopUpdateInstaller::new().with_is_windows(true));
    assert!(!windows.apply_update_and_restart(&txt).unwrap());
    assert!(txt.exists());

    let missing = dir.path().join("missing.AppImage");
    assert!(!windows.apply_update_and_restart(&missing).unwrap());

    let update = dir.path().join("update.AppImage");
    fs::write(&update, b"NEW").unwrap();
    let no_appimage = counting(
        DesktopUpdateInstaller::new()
            .with_is_windows(false)
            .with_current_app_image_path(None),
    );
    assert!(!no_appimage.apply_update_and_restart(&update).unwrap());
    assert!(update.exists());

    let gone = dir.path().join("gone/EmuSync.AppImage");
    let stale_path = counting(
        DesktopUpdateInstaller::new()
            .with_is_windows(false)
            .with_current_app_image_path(Some(gone)),
    );
    assert!(!stale_path.apply_update_and_restart(&update).unwrap());

    assert_eq!(exits.load(Ordering::SeqCst), 0);
    assert_eq!(fs::read(&update).unwrap(), b"NEW");
    assert_eq!(fs::read(&txt).unwrap(), b"text");
}

#[test]
fn restart_trait_method_delegates_to_apply_update() {
    let dir = tempdir().unwrap();
    let (current, update) = linux_files(dir.path());
    let (log, seen) = (log(), Arc::new(Mutex::new(None)));
    let installer = linux_installer(&current, &log, &seen, false);

    assert!(installer.restart(update.to_str().unwrap()).unwrap());

    assert_eq!(entries(&log), ["launch", "exit:0"]);
    assert_eq!(fs::read_to_string(&current).unwrap(), "NEW");
}
