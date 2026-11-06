use emusync_platform_desktop::{AppImageEnvironment, shell_argument};
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

#[test]
fn shell_argument_escapes_and_expands_rom() {
    let plain = shell_argument("my game", false).unwrap();
    assert_eq!(plain, "\"my game\"");

    let with_rom = shell_argument("--rom={ROM}", true).unwrap();
    assert_eq!(with_rom, "\"--rom=${ROM_PATH}\"");

    let with_quotes = shell_argument("hello \"world\"", false).unwrap();
    assert_eq!(with_quotes, "\"hello \\\"world\\\"\"");

    let with_dollar = shell_argument("$HOME `date`", false).unwrap();
    assert_eq!(with_dollar, "\"\\$HOME \\`date\\`\"");

    assert!(shell_argument("bad\0arg", false).is_err());
}
