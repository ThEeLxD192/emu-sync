//! Launchers generados (`.sh`, `.bat`, `.json`, `.ps1`): inyección, argv exacto y formato estable.

mod common;

use common::linux_manager;
use emusync_domain::library::Emulator;
use emusync_platform_desktop::{SteamShortcutManager, WindowsLaunchConfiguration};
use std::fs;
#[cfg(target_os = "linux")]
use std::path::Path;
use tempfile::tempdir;

#[test]
fn linux_wrapper_rejects_nul_argument_without_writing_script() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let mut emu = Emulator::new("Nul", "/usr/bin/emu", "/roms", vec![]);
    emu.common.arguments = vec!["bad\0arg".into()];

    let err = manager.create_linux_wrapper_script(&emu).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    let script = manager
        .get_launcher_dir()
        .join(format!("launch_{}.sh", manager.launcher_key("Nul")));
    assert!(!script.exists());
}

#[cfg(unix)]
#[test]
fn linux_wrapper_script_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path());
    let emu = Emulator::new("Exec", "/usr/bin/emu", "/roms", vec![]);
    let script = manager.create_linux_wrapper_script(&emu).unwrap();
    let mode = fs::metadata(script).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755);
}

#[test]
fn windows_launcher_script_bytes_are_stable() {
    use sha2::{Digest, Sha256};
    let dir = tempdir().unwrap();
    let manager = SteamShortcutManager::new()
        .with_custom_launcher_dir(dir.path().join("launchers"))
        .with_simulate_windows(true);
    let emu = Emulator::new("Win", "C:\\emu\\emu.exe", "C:\\roms", vec![]);
    manager.create_windows_wrapper_script(&emu).unwrap();
    let bytes = fs::read(dir.path().join("launchers/windows-launcher-v1.ps1")).unwrap();
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        digest,
        "bd6a54261feb86108cccb7fce4156de1907752cb1ff54b534e30d5917c5fe742"
    );
}

/// Emulador falso que vuelca su argv (separado por NUL) en `out`.
#[cfg(target_os = "linux")]
fn recorder_script(path: &Path, out: &Path) {
    assert!(!out.to_string_lossy().contains('\''));
    common::write_script(
        path,
        &format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\0' \"$a\"; done > '{}'\n",
            out.display()
        ),
    );
}

#[cfg(target_os = "linux")]
fn recorded_argv(out: &Path) -> Vec<Vec<u8>> {
    let mut parts: Vec<Vec<u8>> = fs::read(out)
        .unwrap()
        .split(|b| *b == 0)
        .map(<[u8]>::to_vec)
        .collect();
    assert_eq!(parts.pop(), Some(Vec::new()), "argv must end with NUL");
    parts
}

#[cfg(target_os = "linux")]
#[test]
fn generated_linux_launcher_passes_hostile_rom_and_arguments_as_exact_argv() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let manager = linux_manager(root);
    let out = root.join("argv.out");
    let exe = root.join("emu $HOME \"q\" `x`").join("rec.sh");
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    recorder_script(&exe, &out);

    let rom_dir = root.join("roms dir");
    fs::create_dir_all(&rom_dir).unwrap();
    let rom = rom_dir.join("a\"b'c $(touch PWN2) `touch PWN3`.iso");
    fs::write(&rom, b"ROM").unwrap();

    let mut emu = Emulator::new("Hostile", exe.to_str().unwrap(), "/roms", vec![]);
    // `parse_arguments` solo reconoce comillas dobles.
    emu.fullscreen_args = Some("-a \"b c\"".into());
    emu.common.arguments = vec![
        "--load".into(),
        "{ROM}".into(),
        "--lit=${HOME}`id`$(touch PWN1)".into(),
    ];
    manager.prepare_launch(&emu, &rom).unwrap();
    let script = manager.create_linux_wrapper_script(&emu).unwrap();

    let output = common::run_bash(&script, root);

    assert!(output.status.success(), "{output:?}");
    let expected: Vec<Vec<u8>> = [
        "-a",
        "b c",
        "--load",
        rom.to_str().unwrap(),
        "--lit=${HOME}`id`$(touch PWN1)",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect();
    assert_eq!(recorded_argv(&out), expected);
    for marker in ["PWN1", "PWN2", "PWN3"] {
        assert!(!root.join(marker).exists(), "{marker} was created");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn generated_linux_launcher_fails_without_signal_or_with_missing_rom_and_never_execs_emulator() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let manager = linux_manager(root);
    let out = root.join("argv.out");
    let exe = root.join("bin/rec.sh");
    recorder_script(&exe, &out);
    let emu = Emulator::new("Guarded", exe.to_str().unwrap(), "/roms", vec![]);
    let script = manager.create_linux_wrapper_script(&emu).unwrap();

    let no_signal = common::run_bash(&script, root);
    assert_eq!(no_signal.status.code(), Some(1));
    assert!(!out.exists());

    let missing_rom = root.join("gone.rom");
    manager.prepare_launch(&emu, &missing_rom).unwrap();
    let stale_signal = common::run_bash(&script, root);
    assert_eq!(stale_signal.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&stale_signal.stdout).contains("ROM file not found"),
        "{stale_signal:?}"
    );
    assert!(!out.exists());
}

#[cfg(target_os = "linux")]
#[test]
fn emulator_name_with_newlines_cannot_inject_commands_into_launcher() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let manager = linux_manager(root);
    let out = root.join("argv.out");
    let exe = root.join("bin/rec.sh");
    recorder_script(&exe, &out);
    let rom = root.join("game.rom");
    fs::write(&rom, b"ROM").unwrap();
    let injected = format!("touch {}/PWN", root.display());
    let name = format!("Evil\n{injected}\n#");
    let emu = Emulator::new(&name, exe.to_str().unwrap(), "/roms", vec![]);
    manager.prepare_launch(&emu, &rom).unwrap();

    let script = manager.create_linux_wrapper_script(&emu).unwrap();

    let contents = fs::read_to_string(&script).unwrap();
    assert!(
        contents.lines().all(|line| line != injected),
        "newline in the name became its own script line"
    );
    let output = common::run_bash(&script, root);
    assert!(output.status.success(), "{output:?}");
    assert!(out.exists(), "the legitimate emulator must still run");
    assert!(!root.join("PWN").exists());
}

#[test]
fn windows_wrapper_rejects_nul_in_executable_path_without_writing_any_file() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path()).with_simulate_windows(true);

    let bad_executable = Emulator::new("WinNul", "C:\\emu\0.exe", "C:\\roms", vec![]);
    let err = manager
        .create_windows_wrapper_script(&bad_executable)
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(!manager.get_launcher_dir().exists());

    let mut bad_argument = Emulator::new("WinNul", "C:\\emu\\emu.exe", "C:\\roms", vec![]);
    bad_argument.common.arguments = vec!["--x\0y".into()];
    let err = manager
        .create_windows_wrapper_script(&bad_argument)
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(!manager.get_launcher_dir().exists());
}

#[test]
fn windows_config_lists_fullscreen_tokens_then_raw_arguments_with_rom_placeholder_intact() {
    let dir = tempdir().unwrap();
    let manager = linux_manager(dir.path()).with_simulate_windows(true);
    let mut emu = Emulator::new("WinArgs", "C:\\emu\\emu.exe", "C:\\roms", vec![]);
    emu.fullscreen_args = Some("-x \"a b\"".into());
    emu.common.arguments = vec!["{ROM}".into(), "--y".into()];

    manager.create_windows_wrapper_script(&emu).unwrap();

    let config_file = manager
        .get_launcher_dir()
        .join(format!("launch_{}.json", manager.launcher_key("WinArgs")));
    let config: WindowsLaunchConfiguration =
        serde_json::from_str(&fs::read_to_string(config_file).unwrap()).unwrap();
    assert_eq!(config.arguments, ["-x", "a b", "{ROM}", "--y"]);
    assert_eq!(
        config.signal_file,
        manager.get_signal_file(&emu).to_string_lossy()
    );
    assert_eq!(config.executable, "C:\\emu\\emu.exe");
}

fn quoted_literals(command: &str) -> Vec<String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    command
        .split("FromBase64String('")
        .skip(1)
        .map(|part| {
            let literal = part.split("')").next().unwrap();
            String::from_utf8(STANDARD.decode(literal).unwrap()).unwrap()
        })
        .collect()
}

#[test]
fn encoded_command_round_trips_launcher_paths_with_quotes_and_unicode() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let dir = tempdir().unwrap();
    let launcher_dir = dir.path().join("O'Brien ü $x");
    let manager = SteamShortcutManager::new()
        .with_custom_launcher_dir(launcher_dir.clone())
        .with_simulate_windows(true);
    let key = manager.launcher_key("Quoted");

    let encoded = manager.encoded_command(&key);

    let bytes = STANDARD.decode(encoded).unwrap();
    let utf16: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    let command = String::from_utf16(&utf16).unwrap();
    assert!(command.ends_with("\nexit $LASTEXITCODE"), "{command}");
    assert_eq!(
        quoted_literals(&command),
        [
            launcher_dir
                .join("windows-launcher-v1.ps1")
                .to_string_lossy()
                .into_owned(),
            launcher_dir
                .join(format!("launch_{key}.json"))
                .to_string_lossy()
                .into_owned(),
        ]
    );
}
