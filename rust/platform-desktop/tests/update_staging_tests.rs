//! `UpdateStagingFiles`: destino de descarga, aislamiento frente a nombres hostiles y limpieza.

mod common;

use emusync_domain::update::{UpdateFiles, UpdateInfo};
use emusync_platform_desktop::UpdateStagingFiles;
use std::{fs, path::Path};
use tempfile::tempdir;

fn info(asset_name: &str) -> UpdateInfo {
    UpdateInfo {
        version: "1.2.0".into(),
        tag_name: "v1.2.0".into(),
        release_notes: String::new(),
        download_url: "http://127.0.0.1:9/never-used".into(),
        asset_size: 3,
        asset_name: asset_name.into(),
    }
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[tokio::test]
async fn destination_is_hidden_file_in_writable_appimage_directory_and_probe_is_removed() {
    let dir = tempdir().unwrap();
    let staging = UpdateStagingFiles::new(false, Some(dir.path().join("EmuSync.AppImage")));

    let destination = staging
        .prepare_destination(&info("EmuSync-1.2.AppImage"))
        .await
        .unwrap();

    assert_eq!(Path::new(&destination).parent(), Some(dir.path()));
    let name = file_name(&destination);
    assert!(name.starts_with(".EmuSync-update-"), "{name}");
    assert!(name.ends_with(".AppImage"), "{name}");
    assert!(!dir.path().join(".probe_write").exists());
    assert!(
        !Path::new(&destination).exists(),
        "only the path is reserved"
    );
    assert_eq!(common::dir_entries(dir.path()), Vec::<String>::new());
}

#[tokio::test]
async fn traversal_in_asset_name_cannot_escape_destination_directory() {
    let dir = tempdir().unwrap();
    let staging = UpdateStagingFiles::new(false, Some(dir.path().join("EmuSync.AppImage")));

    for asset_name in ["../../evil.sh", "x/../../y.AppImage", "..\\..\\z.msi", ""] {
        let destination = staging
            .prepare_destination(&info(asset_name))
            .await
            .unwrap();

        assert_eq!(
            Path::new(&destination).parent(),
            Some(dir.path()),
            "{asset_name:?} -> {destination}"
        );
        let name = file_name(&destination);
        assert!(!name.contains('/') && !name.contains(".."), "{name}");
        assert!(!name.contains('\\'), "{name}");
    }
    assert_eq!(common::dir_entries(dir.path()), Vec::<String>::new());
}

#[cfg(unix)]
#[tokio::test]
async fn unwritable_appimage_directory_falls_back_to_temp_dir() {
    use std::os::unix::fs::PermissionsExt;
    if common::permissions_are_not_enforced() {
        eprintln!("skipped: the test process can write to read-only directories (root?)");
        return;
    }
    struct Restore<'a>(&'a Path);
    impl Drop for Restore<'_> {
        fn drop(&mut self) {
            let _ = fs::set_permissions(self.0, fs::Permissions::from_mode(0o755));
        }
    }

    let dir = tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let _restore = Restore(&locked);
    let staging = UpdateStagingFiles::new(false, Some(locked.join("EmuSync.AppImage")));

    let destination = staging
        .prepare_destination(&info("EmuSync.AppImage"))
        .await
        .unwrap();

    assert_eq!(
        Path::new(&destination).parent(),
        Some(std::env::temp_dir().as_path())
    );
    assert_eq!(common::dir_entries(&locked), Vec::<String>::new());
}

#[tokio::test]
async fn missing_extension_defaults_per_platform() {
    let linux = UpdateStagingFiles::new(false, None);
    let destination = linux.prepare_destination(&info("EmuSync")).await.unwrap();
    assert!(
        file_name(&destination).ends_with(".AppImage"),
        "{destination}"
    );

    let windows = UpdateStagingFiles::new(true, None);
    let destination = windows.prepare_destination(&info("EmuSync")).await.unwrap();
    assert!(file_name(&destination).ends_with(".msi"), "{destination}");
    assert_eq!(
        Path::new(&destination).parent(),
        Some(std::env::temp_dir().as_path())
    );
}

#[test]
fn delete_is_idempotent_and_size_reports_missing_file() {
    let dir = tempdir().unwrap();
    let staging = UpdateStagingFiles::new(false, None);
    let missing = dir.path().join("missing.AppImage");
    let missing = missing.to_str().unwrap();

    staging.delete(missing).unwrap();
    assert!(staging.size(missing).is_err());

    let subdirectory = dir.path().join("subdirectory");
    fs::create_dir(&subdirectory).unwrap();
    assert!(staging.delete(subdirectory.to_str().unwrap()).is_err());
    assert!(subdirectory.is_dir());

    let file = dir.path().join("download.AppImage");
    fs::write(&file, b"abc").unwrap();
    assert_eq!(staging.size(file.to_str().unwrap()).unwrap(), 3);
    staging.delete(file.to_str().unwrap()).unwrap();
    assert!(!file.exists());
    staging.delete(file.to_str().unwrap()).unwrap();
}
