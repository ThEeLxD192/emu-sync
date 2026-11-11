use emusync_data_local::resolve_config_location;
use std::{ffi::OsStr, path::PathBuf};

#[test]
fn explicit_argument_and_env_override_ignore_blank_values() {
    let explicit_path = PathBuf::from("/ruta/explicita/config.json");
    let result = resolve_config_location(Some(explicit_path.as_os_str())).unwrap();
    assert_eq!(result, explicit_path);

    let blank_result = resolve_config_location(Some(OsStr::new("   \t  "))).unwrap();
    assert_eq!(
        blank_result.file_name(),
        Some(OsStr::new("config.json")),
        "Blank explicit value must be ignored and fall back to a config.json path"
    );
    assert_ne!(
        blank_result.as_os_str(),
        "   \t  ",
        "Result must not be the blank string"
    );
}

#[cfg(unix)]
#[test]
fn unwritable_portable_config_and_parent_fall_back_to_platform_location() {
    if unsafe { libc::geteuid() == 0 } {
        // Root always satisfies W_OK regardless of permission bits
        return;
    }

    if std::env::var_os("EMUSYNC_TEST_RO_CHILD").is_some() {
        let cwd_config = std::env::current_dir().unwrap().join("config.json");
        let resolved = resolve_config_location(None).unwrap();
        assert_ne!(
            resolved, cwd_config,
            "Unwritable portable config must not be selected"
        );
        assert!(
            resolved.ends_with("emusync/config.json"),
            "Expected fallback to platform location, got: {resolved:?}"
        );
        return;
    }

    use std::{fs, os::unix::fs::PermissionsExt};

    let dir = tempfile::tempdir().unwrap();
    let ro_dir = dir.path().join("ro_dir");
    fs::create_dir(&ro_dir).unwrap();
    let config_file = ro_dir.join("config.json");
    fs::write(&config_file, b"{}").unwrap();

    struct PermGuard {
        dir: PathBuf,
        file: PathBuf,
    }
    impl Drop for PermGuard {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700));
            let _ = fs::set_permissions(&self.file, fs::Permissions::from_mode(0o600));
        }
    }

    let _guard = PermGuard {
        dir: ro_dir.clone(),
        file: config_file.clone(),
    };

    // Remove write permissions from both file and parent directory
    fs::set_permissions(&config_file, fs::Permissions::from_mode(0o400)).unwrap();
    fs::set_permissions(&ro_dir, fs::Permissions::from_mode(0o500)).unwrap();

    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("unwritable_portable_config_and_parent_fall_back_to_platform_location")
        .arg("--exact")
        .current_dir(&ro_dir)
        .env("EMUSYNC_TEST_RO_CHILD", "1")
        .env_remove("EMUSYNC_CONFIG")
        .env_remove("APPIMAGE")
        .status()
        .unwrap();

    assert!(status.success(), "Subprocess child failed");
}
