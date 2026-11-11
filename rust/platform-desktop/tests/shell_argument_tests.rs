//! `shell_argument`: escape y expansión controlada de `{ROM}` verificados con bash real.

use emusync_platform_desktop::shell_argument;

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

#[cfg(unix)]
#[test]
fn shell_argument_literal_round_trips_through_bash_for_hostile_corpus() {
    let corpus = [
        "",
        "a b",
        "\"",
        "\\",
        "\\\"",
        "$HOME",
        "${X}",
        "$(id)",
        "`id`",
        "!x",
        "a\nb",
        "'",
        "{ROM}",
        "${ROM_PATH}",
        "ü€",
    ];
    for value in corpus {
        let literal = shell_argument(value, false).unwrap();
        let output = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!("printf '%s' {literal}"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{value:?}: {output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            value,
            "literal {literal} did not round-trip"
        );
    }
}

#[cfg(unix)]
#[test]
fn expanded_rom_placeholder_is_substituted_once_without_reevaluation() {
    let dir = tempfile::tempdir().unwrap();
    let literal = shell_argument("--rom={ROM}", true).unwrap();

    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(format!("ROM_PATH=\"$1\"; printf '%s' {literal}"))
        .arg("_")
        .arg("a b$(touch PWN)")
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "--rom=a b$(touch PWN)"
    );
    assert!(!dir.path().join("PWN").exists());
}
