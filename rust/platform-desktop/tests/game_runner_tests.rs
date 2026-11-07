use emusync_domain::{
    execution::{GameLaunchRequest, GameLauncher, SteamLaunchPreparation},
    library::{Emulator, GameEntry},
};
use emusync_platform_desktop::{ExternalProcessTracker, GameRunner, SteamLaunchOutcome};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
use tempfile::tempdir;

struct MockSteamPreparation {
    prepared_rom: Arc<Mutex<Option<String>>>,
}

impl SteamLaunchPreparation for MockSteamPreparation {
    type Error = std::io::Error;

    fn prepare(&self, _entry: &Emulator, rom_path: &str) -> Result<(), Self::Error> {
        *self.prepared_rom.lock().unwrap() = Some(rom_path.to_string());
        Ok(())
    }
}

#[tokio::test]
async fn direct_launch_executes_process_and_returns_exit_code_and_duration() {
    let prep = Arc::new(MockSteamPreparation {
        prepared_rom: Arc::new(Mutex::new(None)),
    });
    let runner = GameRunner::new(prep);

    let temp = tempdir().unwrap();
    let rom = temp.path().join("game.rom");
    std::fs::write(&rom, b"ROM").unwrap();

    let mut emu = Emulator::new(
        "TestEmu",
        "echo",
        temp.path().to_str().unwrap(),
        vec!["rom".into()],
    );
    emu.common.arguments = vec!["Hello".into(), "{ROM}".into()];
    let entry = GameEntry::Emulator(emu);

    let request = GameLaunchRequest {
        entry,
        rom_path: Some(rom.to_str().unwrap().to_string()),
        steam_app_id: None,
    };

    let result = runner.launch(request).await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.duration_ms >= 0);
}

#[tokio::test]
async fn launch_via_steam_prepares_launch_and_tracks_process() {
    let prepared = Arc::new(Mutex::new(None));
    let prep = Arc::new(MockSteamPreparation {
        prepared_rom: prepared.clone(),
    });

    // Provide mock tracker that simulates process appearing and then exiting
    let tracker = ExternalProcessTracker::new(
        Arc::new(|_| {
            let mut s = HashSet::new();
            s.insert(12345);
            s
        }),
        Arc::new(|| false),
        Arc::new(|_| false),
    );

    let runner = GameRunner::with_tracker(prep, tracker).with_steam_uri_launcher(Arc::new(|_| {
        Some(SteamLaunchOutcome {
            exit_code: Some(0),
            is_alive: false,
            handoff_succeeded: true,
        })
    }));

    let temp = tempdir().unwrap();
    let rom = temp.path().join("game.rom");
    std::fs::write(&rom, b"ROM").unwrap();

    let emu = Emulator::new(
        "SteamEmu",
        "/bin/true",
        temp.path().to_str().unwrap(),
        vec!["rom".into()],
    );
    let entry = GameEntry::Emulator(emu);

    let request = GameLaunchRequest {
        entry,
        rom_path: Some(rom.to_str().unwrap().to_string()),
        steam_app_id: Some(123456),
    };

    let result = runner.launch(request).await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(
        *prepared.lock().unwrap(),
        Some(rom.to_str().unwrap().to_string())
    );
}

#[cfg(unix)]
#[test]
fn appimage_environment_is_removed_from_launched_emulators() {
    use std::os::unix::ffi::OsStringExt;
    let dir = tempdir().unwrap();
    for (index, library_path, expected) in [
        (0, "/synthetic/.mount_EmuSync/usr/lib", "unset"),
        (1, "/synthetic/.mount_EmuSync/usr/lib:/usr/lib", "/usr/lib"),
    ] {
        let probe_dir = dir.path().join(index.to_string());
        std::fs::create_dir(&probe_dir).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "appimage_child_process_probe", "--nocapture"])
            .env("EMUSYNC_RUN_APPIMAGE_PROBE", "1")
            .env("EMUSYNC_PROBE_DIRECTORY", &probe_dir)
            .env("EMUSYNC_PRESERVED", "preserved")
            .env("EMUSYNC_NON_UTF8", std::ffi::OsString::from_vec(vec![0xff]))
            .env("APPDIR", "/synthetic/.mount_EmuSync")
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
    if std::env::var_os("EMUSYNC_RUN_APPIMAGE_PROBE").is_none() {
        return;
    }
    let prep = Arc::new(MockSteamPreparation {
        prepared_rom: Arc::new(Mutex::new(None)),
    });
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
