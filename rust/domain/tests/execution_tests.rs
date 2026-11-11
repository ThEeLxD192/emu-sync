use emusync_domain::execution::{format_arguments, parse_arguments};

#[test]
fn arguments_match_literal_examples() {
    let args = vec![
        "--profile".into(),
        "My Profile".into(),
        "".into(),
        "a\"b".into(),
        "C:\\Games\\".into(),
    ];
    assert_eq!(
        format_arguments(&args),
        "--profile \"My Profile\" \"\" \"a\\\"b\" \"C:\\\\Games\\\\\""
    );
    assert_eq!(
        parse_arguments("--profile \"My Profile\" \"\" \"a\\\"b\" \"C:\\\\Games\\\\\""),
        Some(args)
    );
    assert_eq!(
        parse_arguments("\"C:\\Games\\My Game\""),
        Some(vec!["C:\\Games\\My Game".into()])
    );
    assert_eq!(
        parse_arguments("prefix\" with spaces \"suffix"),
        Some(vec!["prefix with spaces suffix".into()])
    );
    assert_eq!(parse_arguments("\"unfinished\\"), None);
    assert_eq!(parse_arguments("\"unfinished"), None);
}

#[test]
fn arguments_preserve_non_special_escapes_and_format_literals() {
    assert_eq!(
        parse_arguments("\"a\\\"b\" \"a\\\\b\" \"a\\qb\""),
        Some(vec!["a\"b".into(), "a\\b".into(), "a\\qb".into()])
    );
    assert_eq!(
        format_arguments(&["plain".into(), "a\tb".into(), "a\\b".into()]),
        "plain \"a\tb\" \"a\\\\b\""
    );
}

#[test]
fn parse_keeps_literal_backslashes_outside_quotes_and_rejects_unterminated_input() {
    let s = |items: &[&str]| Some(items.iter().map(|i| (*i).to_owned()).collect::<Vec<_>>());
    assert_eq!(parse_arguments("C:\\Games\\x"), s(&["C:\\Games\\x"]));
    assert_eq!(parse_arguments("a\\ b"), s(&["a\\", "b"]));
    assert_eq!(parse_arguments("a\\"), s(&["a\\"]));
    assert_eq!(parse_arguments("\\\"x\\\""), None);
    assert_eq!(parse_arguments("a \""), None);
    assert_eq!(parse_arguments("\"\\"), None);
    assert_eq!(parse_arguments("\"a\\\\\""), s(&["a\\"]));
    assert_eq!(parse_arguments("\"a\\\\\\\"\""), s(&["a\\\""]));
    assert_eq!(parse_arguments("--x=\"a b\" y"), s(&["--x=a b", "y"]));
    assert_eq!(parse_arguments("a\"\"b"), s(&["ab"]));
}

#[test]
fn parse_distinguishes_no_arguments_from_empty_arguments() {
    let s = |items: &[&str]| Some(items.iter().map(|i| (*i).to_owned()).collect::<Vec<_>>());
    assert_eq!(parse_arguments(""), s(&[]));
    assert_eq!(parse_arguments("   \t\n"), s(&[]));
    assert_eq!(parse_arguments("\"\""), s(&[""]));
    assert_eq!(parse_arguments("\"\" \"\""), s(&["", ""]));
    assert_eq!(
        parse_arguments("\"\" prefix\" with spaces \"suffix \"\""),
        s(&["", "prefix with spaces suffix", ""])
    );
    assert_eq!(parse_arguments("\" \""), s(&[" "]));
    assert_eq!(parse_arguments("\"a\tb\""), s(&["a\tb"]));
}

#[test]
fn format_then_parse_round_trips_hostile_arguments() {
    let list: Vec<String> = [
        "\\",
        "\"",
        "a\\",
        "\\\"",
        " ",
        "\t",
        "a\nb",
        "\u{a0}",
        "a\u{3000}b",
        "",
        "\\\\",
        "\"\"",
        "--k=v",
        "a\\\"b\\",
        "{ROM}",
    ]
    .iter()
    .map(|a| (*a).to_owned())
    .collect();
    let formatted = format_arguments(&list);
    assert_eq!(parse_arguments(&formatted), Some(list));
    assert_eq!(format_arguments(&[]), "");
    assert_eq!(parse_arguments(""), Some(vec![]));
    assert!(formatted.ends_with(" {ROM}"), "{formatted}");
    assert!(formatted.contains(" --k=v "), "{formatted}");
}
