//! `GameRunner`: lanzamiento directo y por Steam con lanzadores, sondeos y preparación inyectados.
//!
//! Todo lanzamiento por Steam usa un `steam_process_name` único para que el sondeo real de
//! `/proc` (instantánea de PIDs previa al traspaso) devuelva siempre un conjunto vacío, y nunca
//! se llama al lanzador de URI por defecto.

mod common;

use common::{
    AliveProbe, MockSteamPreparation, ScriptedProbe, fast_polling, guard, polling_with_timeout,
    unique_process_name,
};
use emusync_domain::{
    execution::{GameLaunchRequest, GameLauncher, SteamLaunchPreparation},
    library::{Emulator, GameEntry, NativeGame},
};
use emusync_platform_desktop::{
    ExternalProcessTracker, GameRunner, ProcessPolling, SteamLaunchOutcome, SteamUriLauncherFn,
};
use std::{
    fs,
    io::ErrorKind,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::tempdir;

#[tokio::test]
async fn direct_launch_executes_process_and_returns_exit_code_and_duration() {
    let prep = Arc::new(MockSteamPreparation::new());
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
async fn native_game_with_a_bare_command_name_starts_in_the_current_directory() {
    // A command found through PATH has an empty parent, which cannot be a working directory.
    let runner = GameRunner::new(Arc::new(MockSteamPreparation::new()));
    let mut native = NativeGame::default();
    native.common.name = "PathCommand".into();
    (native.common.executable_path, native.common.arguments) = if cfg!(windows) {
        ("cmd".into(), vec!["/C".into(), "exit 0".into()])
    } else {
        ("sh".into(), vec!["-c".into(), "exit 0".into()])
    };

    let result = guard(runner.launch(GameLaunchRequest {
        entry: GameEntry::Native(native),
        rom_path: None,
        steam_app_id: None,
    }))
    .await
    .unwrap();

    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn launch_via_steam_prepares_launch_and_tracks_process() {
    let prep = Arc::new(MockSteamPreparation::new());
    let prepared = prep.prepared_rom.clone();

    // Provide mock tracker that simulates process appearing and then exiting
    let tracker = ExternalProcessTracker::new(
        Arc::new(|_| {
            let mut s = std::collections::HashSet::new();
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

    let mut emu = Emulator::new(
        "SteamEmu",
        "/bin/true",
        temp.path().to_str().unwrap(),
        vec!["rom".into()],
    );
    emu.steam_process_name = Some(unique_process_name("tracks"));
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

struct FailingPreparation;

impl SteamLaunchPreparation for FailingPreparation {
    type Error = std::io::Error;

    fn prepare(&self, _entry: &Emulator, _rom_path: &str) -> Result<(), Self::Error> {
        Err(std::io::Error::other("disk full"))
    }
}

fn steam_emulator(name: &str) -> Emulator {
    let mut emu = Emulator::new(name, "/bin/true", "/roms", vec![]);
    emu.steam_process_name = Some(unique_process_name("steam"));
    emu
}

fn steam_emulator_request(rom_path: Option<String>) -> GameLaunchRequest {
    GameLaunchRequest {
        entry: GameEntry::Emulator(steam_emulator("SteamEmu")),
        rom_path,
        steam_app_id: Some(123456),
    }
}

fn counting_launcher(
    calls: &Arc<AtomicUsize>,
    outcome: Option<SteamLaunchOutcome>,
) -> SteamUriLauncherFn {
    let calls = Arc::clone(calls);
    let outcome = Mutex::new(outcome);
    Arc::new(move |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        outcome.lock().unwrap().take()
    })
}

fn slow_polling() -> ProcessPolling {
    ProcessPolling {
        appearance_timeout: Duration::from_secs(60),
        appearance_interval: Duration::from_secs(1),
        exit_interval: Duration::from_secs(1),
        reuse_confirmation_delay: Duration::from_secs(1),
    }
}

fn outcome(exit_code: Option<i32>, is_alive: bool, handoff_succeeded: bool) -> SteamLaunchOutcome {
    SteamLaunchOutcome {
        exit_code,
        is_alive,
        handoff_succeeded,
    }
}

/// Lanzador que anota cada URI (y `"uri"` en el log opcional) y devuelve siempre `result()`.
fn recording_launcher(
    uris: &Arc<Mutex<Vec<String>>>,
    log: Option<Arc<Mutex<Vec<String>>>>,
    result: impl Fn() -> SteamLaunchOutcome + Send + Sync + 'static,
) -> SteamUriLauncherFn {
    let uris = Arc::clone(uris);
    Arc::new(move |uri| {
        uris.lock().unwrap().push(uri.to_string());
        if let Some(log) = &log {
            log.lock().unwrap().push("uri".to_string());
        }
        Some(result())
    })
}

fn tracker_with(matching: &ScriptedProbe, alive: &AliveProbe) -> ExternalProcessTracker {
    ExternalProcessTracker::new(matching.matching_fn(), Arc::new(|| false), alive.alive_fn())
}

#[tokio::test]
async fn steam_launch_does_not_open_steam_when_prepare_fails() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runner =
        GameRunner::new(Arc::new(FailingPreparation)).with_steam_uri_launcher(counting_launcher(
            &calls,
            Some(SteamLaunchOutcome {
                exit_code: Some(0),
                is_alive: false,
                handoff_succeeded: true,
            }),
        ));
    let err = runner
        .launch(steam_emulator_request(Some("/roms/game.rom".into())))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Could not prepare Steam launch"));
    assert!(err.to_string().contains("disk full"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn steam_launch_of_emulator_without_rom_is_invalid_input() {
    let prep = Arc::new(MockSteamPreparation::new());
    let prepared = prep.prepared_rom.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let runner = GameRunner::new(prep).with_steam_uri_launcher(counting_launcher(&calls, None));
    let err = runner
        .launch(steam_emulator_request(None))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(err.to_string().contains("A ROM file must be provided"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(prepared.lock().unwrap().is_none());
}

#[tokio::test]
async fn steam_launch_fails_fast_when_steam_cannot_be_opened() {
    let calls = Arc::new(AtomicUsize::new(0));
    let matching = ScriptedProbe::new(vec![vec![]]);
    let alive = AliveProbe::new(&[]);
    let runner = GameRunner::with_tracker(
        Arc::new(MockSteamPreparation::new()),
        tracker_with(&matching, &alive),
    )
    .with_steam_uri_launcher(counting_launcher(&calls, None))
    .with_polling(slow_polling());

    let err = guard(runner.launch(steam_emulator_request(Some("/roms/game.rom".into()))))
        .await
        .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(err.to_string(), "Steam could not be opened");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(matching.calls(), 0, "the tracker must never be consulted");
    assert_eq!(alive.total_calls(), 0);
}

#[tokio::test]
async fn steam_uri_uses_unsigned_shortcut_game_id_for_negative_and_extreme_app_ids() {
    let uris = Arc::new(Mutex::new(Vec::new()));
    let cases = [
        (123456, "steam://rungameid/530239516049408"),
        (-1234, "steam://rungameid/18446738773753462784"),
        (i32::MIN, "steam://rungameid/9223372036888330240"),
    ];
    for (app_id, expected) in cases {
        uris.lock().unwrap().clear();
        let matching = ScriptedProbe::new(vec![vec![]]);
        let runner = GameRunner::with_tracker(
            Arc::new(MockSteamPreparation::new()),
            tracker_with(&matching, &AliveProbe::new(&[])),
        )
        .with_steam_uri_launcher(recording_launcher(&uris, None, || {
            outcome(Some(1), false, false)
        }))
        .with_polling(fast_polling());
        let mut request = steam_emulator_request(Some("/roms/game.rom".into()));
        request.steam_app_id = Some(app_id);

        let result = guard(runner.launch(request)).await.unwrap();

        assert_eq!(result.exit_code, 1, "handoff failed on purpose");
        assert_eq!(*uris.lock().unwrap(), [expected], "app id {app_id}");
    }
}

#[tokio::test]
async fn steam_launch_prepares_signal_before_opening_uri_and_tracks_after() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let uris = Arc::new(Mutex::new(Vec::new()));
    let matching = ScriptedProbe::with_log(vec![vec![7], vec![]], Arc::clone(&log));
    let runner = GameRunner::with_tracker(
        Arc::new(MockSteamPreparation::with_log(Arc::clone(&log))),
        tracker_with(&matching, &AliveProbe::new(&[])),
    )
    .with_steam_uri_launcher(recording_launcher(&uris, Some(Arc::clone(&log)), || {
        outcome(Some(0), false, true)
    }))
    .with_polling(fast_polling());

    let result = guard(runner.launch(steam_emulator_request(Some("/roms/game.rom".into()))))
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0);
    let log = log.lock().unwrap().clone();
    assert_eq!(log[0], "prepare", "{log:?}");
    assert_eq!(log[1], "uri", "{log:?}");
    assert!(log.len() >= 3, "{log:?}");
    assert!(log[2..].iter().all(|entry| entry == "match"), "{log:?}");
    assert_eq!(uris.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn failed_steam_handoff_aborts_immediately_without_waiting_for_timeout() {
    let uris = Arc::new(Mutex::new(Vec::new()));
    let matching = ScriptedProbe::new(vec![vec![]]);
    let alive = AliveProbe::new(&[]);
    let shader_calls = Arc::new(AtomicUsize::new(0));
    let tracker = {
        let shader_calls = Arc::clone(&shader_calls);
        ExternalProcessTracker::new(
            matching.matching_fn(),
            Arc::new(move || {
                shader_calls.fetch_add(1, Ordering::SeqCst);
                false
            }),
            alive.alive_fn(),
        )
    };
    let runner = GameRunner::with_tracker(Arc::new(MockSteamPreparation::new()), tracker)
        .with_steam_uri_launcher(recording_launcher(&uris, None, || {
            outcome(Some(1), false, false)
        }))
        .with_polling(polling_with_timeout(Duration::from_secs(3600)));

    let result = guard(runner.launch(steam_emulator_request(Some("/roms/game.rom".into()))))
        .await
        .unwrap();

    assert_eq!(result.exit_code, 1);
    assert_eq!(matching.calls(), 1);
    assert_eq!(shader_calls.load(Ordering::SeqCst), 0);
    assert_eq!(alive.total_calls(), 0);
}

#[tokio::test]
async fn native_steam_launch_skips_preparation_and_waits_for_configured_process() {
    let unique = unique_process_name("native");
    let prep = Arc::new(MockSteamPreparation::new());
    let uris = Arc::new(Mutex::new(Vec::new()));
    let matching = ScriptedProbe::new(vec![vec![7], vec![]]);
    let alive = AliveProbe::new(&[]);
    let runner = GameRunner::with_tracker(Arc::clone(&prep), tracker_with(&matching, &alive))
        .with_steam_uri_launcher(recording_launcher(&uris, None, || {
            outcome(Some(0), false, true)
        }))
        .with_polling(fast_polling());
    let mut native = NativeGame::default();
    native.common.name = "Native".into();
    native.common.executable_path = "/usr/bin/native-game".into();
    native.wait_for_process = Some(unique.clone());

    let result = guard(runner.launch(GameLaunchRequest {
        entry: GameEntry::Native(native),
        rom_path: None,
        steam_app_id: Some(4242),
    }))
    .await
    .unwrap();

    assert_eq!(result.exit_code, 0);
    assert_eq!(prep.calls.load(Ordering::SeqCst), 0);
    assert_eq!(uris.lock().unwrap().len(), 1);
    let names = matching.names();
    assert!(!names.is_empty());
    assert!(names.iter().all(|name| *name == unique), "{names:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn direct_emulator_launch_passes_hostile_rom_as_single_argv_and_runs_in_rom_directory() {
    let dir = tempdir().unwrap();
    let rom_dir = dir.path().join("roms dir");
    fs::create_dir_all(&rom_dir).unwrap();
    let rom = rom_dir.join("a\"b'c $(touch PWN).iso");
    fs::write(&rom, b"ROM").unwrap();
    let out = dir.path().join("observed.txt");
    let mut emu = Emulator::new("Shell", "/bin/sh", "/roms", vec![]);
    emu.fullscreen_args = Some(String::new());
    emu.common.arguments = vec![
        "-c".into(),
        format!(
            "printf '%s\\n%s' \"$0\" \"$(pwd -P)\" > '{}'",
            out.display()
        ),
        "{ROM}".into(),
    ];
    let runner = GameRunner::new(Arc::new(MockSteamPreparation::new()));

    let result = guard(runner.launch(GameLaunchRequest {
        entry: GameEntry::Emulator(emu),
        rom_path: Some(rom.to_str().unwrap().into()),
        steam_app_id: None,
    }))
    .await
    .unwrap();

    assert_eq!(result.exit_code, 0);
    assert_eq!(
        fs::read_to_string(&out).unwrap(),
        format!(
            "{}\n{}",
            rom.display(),
            fs::canonicalize(&rom_dir).unwrap().display()
        )
    );
    assert!(!rom_dir.join("PWN").exists());
    assert!(!dir.path().join("PWN").exists());
}

#[cfg(unix)]
fn shell_emulator(script: &str, rom_dir: &std::path::Path) -> (GameEntry, String) {
    let rom = rom_dir.join("game.rom");
    fs::write(&rom, b"ROM").unwrap();
    let mut emu = Emulator::new("Shell", "/bin/sh", "/roms", vec![]);
    emu.fullscreen_args = Some(String::new());
    emu.common.arguments = vec!["-c".into(), script.into()];
    (GameEntry::Emulator(emu), rom.to_str().unwrap().to_string())
}

#[cfg(unix)]
#[tokio::test]
async fn direct_launch_maps_nonzero_exit_status_to_failure() {
    let dir = tempdir().unwrap();
    let (entry, rom) = shell_emulator("exit 3", dir.path());
    let runner = GameRunner::new(Arc::new(MockSteamPreparation::new()));

    let result = guard(runner.launch(GameLaunchRequest {
        entry,
        rom_path: Some(rom),
        steam_app_id: None,
    }))
    .await
    .unwrap();

    assert_eq!(result.exit_code, 1);
}

#[tokio::test]
async fn direct_launch_of_missing_executable_is_not_found_error() {
    let dir = tempdir().unwrap();
    let rom = dir.path().join("game.rom");
    fs::write(&rom, b"ROM").unwrap();
    let mut emu = Emulator::new(
        "Missing",
        dir.path().join("no-such-emulator").to_str().unwrap(),
        "/roms",
        vec![],
    );
    emu.fullscreen_args = Some(String::new());
    let runner = GameRunner::new(Arc::new(MockSteamPreparation::new()));

    let err = guard(runner.launch(GameLaunchRequest {
        entry: GameEntry::Emulator(emu),
        rom_path: Some(rom.to_str().unwrap().into()),
        steam_app_id: None,
    }))
    .await
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::NotFound);
}

#[cfg(unix)]
#[tokio::test]
async fn direct_emulator_launch_without_rom_or_with_bad_fullscreen_quotes_spawns_nothing() {
    let dir = tempdir().unwrap();
    let mark = dir.path().join("MARK");
    let exe = common::write_script(
        &dir.path().join("bin/emu.sh"),
        &format!("#!/bin/sh\ntouch '{}'\n", mark.display()),
    );
    let rom = dir.path().join("game.rom");
    fs::write(&rom, b"ROM").unwrap();
    let runner = GameRunner::new(Arc::new(MockSteamPreparation::new()));

    let without_rom = Emulator::new("NoRom", exe.to_str().unwrap(), "/roms", vec![]);
    let err = guard(runner.launch(GameLaunchRequest {
        entry: GameEntry::Emulator(without_rom),
        rom_path: None,
        steam_app_id: None,
    }))
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(!mark.exists());

    let mut bad_quotes = Emulator::new("BadQuotes", exe.to_str().unwrap(), "/roms", vec![]);
    bad_quotes.fullscreen_args = Some("\"x".into());
    let err = guard(runner.launch(GameLaunchRequest {
        entry: GameEntry::Emulator(bad_quotes),
        rom_path: Some(rom.to_str().unwrap().into()),
        steam_app_id: None,
    }))
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(!mark.exists());
}

#[cfg(unix)]
fn waiting_native(unique: &str, shell_script: &str) -> GameEntry {
    let mut native = NativeGame::default();
    native.common.name = "Waiting".into();
    native.common.executable_path = "/bin/sh".into();
    native.common.arguments = vec!["-c".into(), shell_script.into()];
    native.wait_for_process = Some(unique.to_string());
    GameEntry::Native(native)
}

#[cfg(unix)]
#[tokio::test]
async fn native_wait_for_process_success_is_decided_by_tracker_not_intermediary_exit() {
    let request = |entry| GameLaunchRequest {
        entry,
        rom_path: None,
        steam_app_id: None,
    };

    // El intermediario falla (exit 7) pero el proceso esperado aparece y termina.
    let unique = unique_process_name("wait-ok");
    let matching = ScriptedProbe::new(vec![vec![4242], vec![]]);
    let alive = AliveProbe::new(&[]);
    let runner = GameRunner::with_tracker(
        Arc::new(MockSteamPreparation::new()),
        tracker_with(&matching, &alive),
    )
    .with_polling(fast_polling());
    let result = guard(runner.launch(request(waiting_native(&unique, "exit 7"))))
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(alive.calls_for(4242), 1);

    // El intermediario sale bien (exit 0) pero el proceso esperado nunca aparece.
    let unique = unique_process_name("wait-missing");
    let matching = ScriptedProbe::new(vec![vec![]]);
    let runner = GameRunner::with_tracker(
        Arc::new(MockSteamPreparation::new()),
        tracker_with(&matching, &AliveProbe::new(&[])),
    )
    .with_polling(polling_with_timeout(Duration::from_millis(30)));
    let result = guard(runner.launch(request(waiting_native(&unique, "exit 0"))))
        .await
        .unwrap();
    assert_eq!(result.exit_code, 1);
    assert!(matching.names().iter().all(|name| *name == unique));
}

#[cfg(unix)]
#[tokio::test]
async fn open_emulator_drops_rom_placeholder_arguments_and_runs_in_executable_directory() {
    let dir = tempdir().unwrap();
    let bin = dir.path().join("bin");
    let out = dir.path().join("observed.txt");
    let exe = common::write_script(
        &bin.join("emu.sh"),
        &format!(
            "#!/bin/sh\nprintf '%s|%s|%s' \"$#\" \"$1\" \"$(pwd -P)\" > '{}'\n",
            out.display()
        ),
    );
    let mut emu = Emulator::new("Open", exe.to_str().unwrap(), "/roms", vec![]);
    emu.common.arguments = vec!["--fixed".into(), "--rom={ROM}".into(), "{ROM}".into()];
    let runner = GameRunner::new(Arc::new(MockSteamPreparation::new()));

    // Un `fork` concurrente de otro test puede provocar `ETXTBSY` sobre el script recién escrito.
    let mut attempts = 0;
    let mut child = loop {
        match runner.open_emulator(&emu).await {
            Err(err) if err.kind() == ErrorKind::ExecutableFileBusy && attempts < 50 => {
                attempts += 1;
                tokio::task::yield_now().await;
            }
            other => break other.unwrap(),
        }
    };
    assert!(guard(child.wait()).await.unwrap().success());

    assert_eq!(
        fs::read_to_string(&out).unwrap(),
        format!("1|--fixed|{}", fs::canonicalize(&bin).unwrap().display())
    );
}
