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
