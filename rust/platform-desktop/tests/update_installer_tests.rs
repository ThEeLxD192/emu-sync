use emusync_platform_desktop::{AppImageInstaller, DesktopUpdateInstaller};
use std::{fs, io, sync::Arc, sync::Mutex};
use tempfile::tempdir;

#[test]
fn appimage_installer_replaces_target_atomically_and_cleans_up() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("EmuSync.AppImage");
    let source = dir.path().join("download.AppImage");

    fs::write(&target, b"ORIGINAL").unwrap();
    fs::write(&source, b"UPDATE").unwrap();

    let mut launched = false;
    let installer = AppImageInstaller::new();
    installer
        .replace(&source, &target, || {
            assert_eq!(fs::read_to_string(&target).unwrap(), "UPDATE");
            launched = true;
            Ok(())
        })
        .unwrap();

    assert!(launched);
    assert_eq!(fs::read_to_string(&target).unwrap(), "UPDATE");
    assert!(!source.exists());

    // Only target should remain in directory
    let files: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(files, vec!["EmuSync.AppImage"]);
}

#[test]
fn appimage_installer_restores_original_when_launch_fails() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("EmuSync.AppImage");
    let source = dir.path().join("download.AppImage");

    fs::write(&target, b"ORIGINAL").unwrap();
    fs::write(&source, b"UPDATE").unwrap();

    let installer = AppImageInstaller::new();
    let result = installer.replace(&source, &target, || Err(io::Error::other("Launch failed")));

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "ORIGINAL");
    assert_eq!(fs::read_to_string(&source).unwrap(), "UPDATE");
}

#[test]
fn appimage_installer_failed_rollback_keeps_backup_available_for_recovery() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("EmuSync.AppImage");
    let source = dir.path().join("download.AppImage");

    fs::write(&target, b"ORIGINAL").unwrap();
    fs::write(&source, b"UPDATE").unwrap();

    // Hook move_fn to fail when moving backup back to target
    let target_clone = target.clone();
    let installer = AppImageInstaller::with_hooks(
        |src, dst| {
            fs::copy(src, dst)?;
            Ok(())
        },
        move |src, dst| {
            if src.to_string_lossy().contains(".old.") && dst == target_clone.as_path() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Filesystem locked",
                ));
            }
            fs::rename(src, dst)
        },
    );

    let result = installer.replace(&source, &target, || {
        Err(io::Error::other("Installation error"))
    });

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&source).unwrap(), "UPDATE");

    // The backup must still exist for manual recovery!
    let backup_exists = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().ends_with(".bak"));
    assert!(backup_exists);
}

#[test]
fn appimage_installer_rejects_replacing_file_with_itself() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("EmuSync.AppImage");
    fs::write(&target, b"ORIGINAL").unwrap();

    let installer = AppImageInstaller::new();
    let result = installer.replace(&target, &target, || Ok(()));
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "ORIGINAL");
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

    let success = installer.apply_windows_update_and_restart(&msi_file);
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

    let success = installer.apply_windows_update_and_restart(&exe_file);
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
    let success = installer.apply_windows_update_and_restart(&txt_file);
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

    let success = installer.apply_linux_update_and_restart(&downloaded_update);
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

    let success = installer.apply_linux_update_and_restart(&downloaded_update);
    assert!(success);
    // In Steam Game Mode, separate launcher process is NOT spawned!
    assert!(!*spawned.lock().unwrap());
    assert_eq!(
        fs::read_to_string(&current_appimage).unwrap(),
        "NEW_VERSION"
    );
}
