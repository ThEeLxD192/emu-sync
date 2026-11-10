//! Limpieza del entorno AppImage heredado por emuladores, selectores y navegadores hijos.

mod common;

use emusync_platform_desktop::AppImageEnvironment;
use std::collections::HashMap;

#[test]
fn removes_appimage_paths_and_variables_while_preserving_other_library_paths() {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut env = HashMap::new();
    env.insert("APPDIR".into(), "/tmp/.mount_EmuSync123".into());
    env.insert("ARGV0".into(), "EmuSync".into());
    env.insert("OWD".into(), "/home/user".into());
    env.insert(
        "LD_LIBRARY_PATH".into(),
        [
            "/tmp/.mount_EmuSync123/lib/app",
            "/usr/lib",
            "/tmp/.mount_OtherApp456/usr/lib",
            "/opt/emulator/lib",
        ]
        .join(sep),
    );

    AppImageEnvironment::clean(&mut env);

    assert_eq!(
        env.get("LD_LIBRARY_PATH").map(String::as_str),
        Some(["/usr/lib", "/opt/emulator/lib"].join(sep).as_str())
    );
    assert!(!env.contains_key("APPDIR"));
    assert!(!env.contains_key("ARGV0"));
    assert!(!env.contains_key("OWD"));
}

#[test]
fn removes_library_path_when_it_contains_only_appimage_runtime_paths() {
    let mut env = HashMap::new();
    env.insert(
        "LD_LIBRARY_PATH".into(),
        "/tmp/.mount_EmuSync123/lib/app".into(),
    );

    AppImageEnvironment::clean(&mut env);

    assert!(!env.contains_key("LD_LIBRARY_PATH"));
}

#[cfg(unix)]
#[test]
fn appdir_prefix_match_respects_path_boundaries() {
    let mut env = HashMap::new();
    env.insert("APPDIR".to_string(), "/opt/app/".to_string());
    env.insert(
        "LD_LIBRARY_PATH".to_string(),
        "/opt/app:/opt/app/lib:/opt/application/lib:/usr/lib".to_string(),
    );

    AppImageEnvironment::clean(&mut env);

    assert_eq!(
        env.get("LD_LIBRARY_PATH").map(String::as_str),
        Some("/opt/application/lib:/usr/lib")
    );
    assert!(!env.contains_key("APPDIR"));
}

#[cfg(unix)]
#[test]
fn root_appdir_does_not_strip_every_library_path() {
    let mut env = HashMap::new();
    env.insert("APPDIR".to_string(), "/".to_string());
    env.insert("LD_LIBRARY_PATH".to_string(), "/usr/lib:/lib".to_string());

    AppImageEnvironment::clean(&mut env);

    assert_eq!(
        env.get("LD_LIBRARY_PATH").map(String::as_str),
        Some("/usr/lib:/lib")
    );
    assert!(!env.contains_key("APPDIR"));
}

#[cfg(unix)]
#[test]
fn appimage_environment_is_removed_from_launched_emulators() {
    use std::os::unix::ffi::OsStringExt;
    let dir = tempfile::tempdir().unwrap();
    for (index, app_dir, library_path, expected) in [
        (
            0,
            "/synthetic/.mount_EmuSync",
            "/synthetic/.mount_EmuSync/usr/lib",
            "unset",
        ),
        (
            1,
            "/synthetic/.mount_EmuSync",
            "/synthetic/.mount_EmuSync/usr/lib:/usr/lib",
            "/usr/lib",
        ),
        // Un APPDIR raíz no debe eliminar todas las rutas de bibliotecas.
        (2, "/", "/usr/lib", "/usr/lib"),
    ] {
        let probe_dir = dir.path().join(index.to_string());
        std::fs::create_dir(&probe_dir).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "appimage_child_process_probe", "--nocapture"])
            .env("EMUSYNC_RUN_APPIMAGE_PROBE", "1")
            .env("EMUSYNC_PROBE_DIRECTORY", &probe_dir)
            .env("EMUSYNC_PRESERVED", "preserved")
            .env("EMUSYNC_NON_UTF8", std::ffi::OsString::from_vec(vec![0xff]))
            .env("APPDIR", app_dir)
            .env("ARGV0", "EmuSync")
            .env("OWD", "/synthetic")
            .env("LD_LIBRARY_PATH", library_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for name in ["open", "launch", "chooser"] {
            let observed = std::fs::read_to_string(probe_dir.join(name)).unwrap();
            assert_eq!(observed, format!("unset|unset|unset|{expected}|preserved"));
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn appimage_child_process_probe() {
    use emusync_domain::{
        execution::{GameLaunchRequest, GameLauncher},
        library::{Emulator, GameEntry},
    };
    use emusync_platform_desktop::GameRunner;
    use std::sync::Arc;

    if std::env::var_os("EMUSYNC_RUN_APPIMAGE_PROBE").is_none() {
        return;
    }
    let prep = Arc::new(common::MockSteamPreparation::new());
    let runner = GameRunner::new(prep);
    let directory = std::path::PathBuf::from(std::env::var_os("EMUSYNC_PROBE_DIRECTORY").unwrap());
    let rom = directory.join("game.rom");
    std::fs::write(&rom, b"synthetic").unwrap();
    let script = |name: &str| {
        format!(
            "test -n \"$EMUSYNC_NON_UTF8\" || exit 1; printf '%s|%s|%s|%s|%s' \"${{APPDIR-unset}}\" \"${{ARGV0-unset}}\" \"${{OWD-unset}}\" \"${{LD_LIBRARY_PATH-unset}}\" \"$EMUSYNC_PRESERVED\" > \"$EMUSYNC_PROBE_DIRECTORY/{name}\""
        )
    };
    let mut emulator = Emulator::new(
        "probe",
        "/bin/sh",
        directory.to_str().unwrap(),
        vec!["rom".into()],
    );
    emulator.common.arguments = vec!["-c".into(), script("open")];
    assert!(
        runner
            .open_emulator(&emulator)
            .await
            .unwrap()
            .wait()
            .await
            .unwrap()
            .success()
    );
    emulator.common.arguments = vec!["-c".into(), script("launch")];
    let result = runner
        .launch(GameLaunchRequest {
            entry: GameEntry::Emulator(emulator),
            rom_path: Some(rom.to_string_lossy().into_owned()),
            steam_app_id: None,
        })
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    let chooser = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        format!("{}; printf '/synthetic/selected'", script("chooser")),
    ];
    assert_eq!(
        emusync_platform_desktop::run_external_choosers(&[chooser]),
        emusync_platform_desktop::ExternalChooserResult::Selected("/synthetic/selected".into())
    );
}
