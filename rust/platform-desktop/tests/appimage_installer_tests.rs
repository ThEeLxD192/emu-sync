//! `AppImageInstaller`: reemplazo atómico, rollback y limpieza de temporales con hooks de fallo.

mod common;

use common::dir_entries;
use emusync_platform_desktop::AppImageInstaller;
use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tempfile::tempdir;

const TARGET_NAME: &str = "EmuSync.AppImage";
const SOURCE_NAME: &str = "download.AppImage";

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempdir().unwrap();
    let target = dir.path().join(TARGET_NAME);
    let source = dir.path().join(SOURCE_NAME);
    fs::write(&target, b"ORIGINAL").unwrap();
    fs::write(&source, b"UPDATE").unwrap();
    (dir, target, source)
}

fn counter() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

fn untouched_pair(dir: &Path) {
    assert_eq!(
        fs::read_to_string(dir.join(TARGET_NAME)).unwrap(),
        "ORIGINAL"
    );
    assert_eq!(fs::read_to_string(dir.join(SOURCE_NAME)).unwrap(), "UPDATE");
    assert_eq!(dir_entries(dir), [TARGET_NAME, SOURCE_NAME]);
}

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

    // El error devuelto es el de la instalación, no el del rollback fallido.
    assert_eq!(result.unwrap_err().to_string(), "Installation error");
    assert_eq!(fs::read_to_string(&source).unwrap(), "UPDATE");
    assert_eq!(fs::read_to_string(&target).unwrap(), "UPDATE");

    // The backup must still exist for manual recovery!
    let backups: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".bak"))
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), "ORIGINAL");
    assert!(
        fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().ends_with(".tmp")),
        "no staged temporary may remain"
    );
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

#[cfg(unix)]
#[test]
fn replaced_appimage_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().unwrap();
    let target = dir.path().join("EmuSync.AppImage");
    let source = dir.path().join("download.AppImage");
    fs::write(&target, b"ORIGINAL").unwrap();
    fs::write(&source, b"UPDATE").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o644)).unwrap();

    AppImageInstaller::new()
        .replace(&source, &target, || Ok(()))
        .unwrap();
    let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755);
}

#[test]
fn copy_failure_leaves_target_and_source_untouched_and_no_temporaries() {
    let (dir, target, source) = fixture();
    let moves = counter();
    let installed = Arc::new(AtomicBool::new(false));
    let installer = {
        let moves = Arc::clone(&moves);
        AppImageInstaller::with_hooks(
            |_, _| Err(io::Error::other("copy refused")),
            move |src, dst| {
                moves.fetch_add(1, Ordering::SeqCst);
                fs::rename(src, dst)
            },
        )
    };

    let result = installer.replace(&source, &target, || {
        installed.store(true, Ordering::SeqCst);
        Ok(())
    });

    assert_eq!(result.unwrap_err().to_string(), "copy refused");
    assert_eq!(moves.load(Ordering::SeqCst), 0);
    assert!(!installed.load(Ordering::SeqCst));
    untouched_pair(dir.path());
}

#[test]
fn backup_move_failure_removes_staged_copy_and_keeps_target() {
    let (dir, target, source) = fixture();
    let moves = counter();
    let installed = Arc::new(AtomicBool::new(false));
    let installer = {
        let (moves, target) = (Arc::clone(&moves), target.clone());
        AppImageInstaller::with_hooks(
            |src, dst| fs::copy(src, dst).map(|_| ()),
            move |src, dst| {
                moves.fetch_add(1, Ordering::SeqCst);
                if src == target.as_path() && dst.to_string_lossy().contains(".old.") {
                    return Err(io::Error::other("backup refused"));
                }
                fs::rename(src, dst)
            },
        )
    };

    let result = installer.replace(&source, &target, || {
        installed.store(true, Ordering::SeqCst);
        Ok(())
    });

    assert_eq!(result.unwrap_err().to_string(), "backup refused");
    assert_eq!(moves.load(Ordering::SeqCst), 1, "no restore is needed");
    assert!(!installed.load(Ordering::SeqCst));
    untouched_pair(dir.path());
}

#[test]
fn install_move_failure_restores_original_and_cleans_backup_and_staged() {
    let (dir, target, source) = fixture();
    let installed = Arc::new(AtomicBool::new(false));
    let installer = {
        let target = target.clone();
        AppImageInstaller::with_hooks(
            |src, dst| fs::copy(src, dst).map(|_| ()),
            move |src, dst| {
                if dst == target.as_path() && src.to_string_lossy().contains(".update.") {
                    return Err(io::Error::other("install refused"));
                }
                fs::rename(src, dst)
            },
        )
    };

    let result = installer.replace(&source, &target, || {
        installed.store(true, Ordering::SeqCst);
        Ok(())
    });

    assert_eq!(result.unwrap_err().to_string(), "install refused");
    assert!(!installed.load(Ordering::SeqCst));
    assert_eq!(fs::read_to_string(&target).unwrap(), "ORIGINAL");
    assert_eq!(fs::read_to_string(&source).unwrap(), "UPDATE");
    assert_eq!(dir_entries(dir.path()), [TARGET_NAME, SOURCE_NAME]);
}

#[cfg(unix)]
#[test]
fn symlinked_source_pointing_to_target_is_rejected() {
    let (dir, target, source) = fixture();
    fs::remove_file(&source).unwrap();
    std::os::unix::fs::symlink(&target, &source).unwrap();
    let (copies, moves) = (counter(), counter());
    let installer = {
        let (copies, moves) = (Arc::clone(&copies), Arc::clone(&moves));
        AppImageInstaller::with_hooks(
            move |src, dst| {
                copies.fetch_add(1, Ordering::SeqCst);
                fs::copy(src, dst).map(|_| ())
            },
            move |src, dst| {
                moves.fetch_add(1, Ordering::SeqCst);
                fs::rename(src, dst)
            },
        )
    };

    let result = installer.replace(&source, &target, || Ok(()));

    assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidInput);
    assert_eq!(copies.load(Ordering::SeqCst), 0);
    assert_eq!(moves.load(Ordering::SeqCst), 0);
    assert_eq!(fs::read_to_string(&target).unwrap(), "ORIGINAL");
    assert!(
        fs::symlink_metadata(&source)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(dir_entries(dir.path()), [TARGET_NAME, SOURCE_NAME]);
}

#[test]
fn non_regular_files_are_rejected_before_any_operation() {
    let (copies, moves) = (counter(), counter());
    let build = || {
        let (copies, moves) = (Arc::clone(&copies), Arc::clone(&moves));
        AppImageInstaller::with_hooks(
            move |src, dst| {
                copies.fetch_add(1, Ordering::SeqCst);
                fs::copy(src, dst).map(|_| ())
            },
            move |src, dst| {
                moves.fetch_add(1, Ordering::SeqCst);
                fs::rename(src, dst)
            },
        )
    };

    let (dir, target, source) = fixture();
    fs::remove_file(&source).unwrap();
    fs::create_dir(&source).unwrap();
    let err = build().replace(&source, &target, || Ok(())).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(fs::read_to_string(&target).unwrap(), "ORIGINAL");
    assert!(source.is_dir());
    assert_eq!(dir_entries(dir.path()), [TARGET_NAME, SOURCE_NAME]);

    let (dir, target, source) = fixture();
    fs::remove_file(&target).unwrap();
    fs::create_dir(&target).unwrap();
    let err = build().replace(&source, &target, || Ok(())).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(fs::read_to_string(&source).unwrap(), "UPDATE");
    assert!(target.is_dir());
    assert_eq!(dir_entries(dir.path()), [TARGET_NAME, SOURCE_NAME]);

    assert_eq!(copies.load(Ordering::SeqCst), 0);
    assert_eq!(moves.load(Ordering::SeqCst), 0);
}
