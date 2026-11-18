use emusync_domain::sync::{
    RemoteFile, UnverifiedDuplicates, canonical_cloud_files, canonical_cloud_files_with,
    is_windows_file_name, name_key, remote_path_segments,
};

fn remote(id: &str, name: &str, modified: i128, sha: Option<&str>) -> RemoteFile {
    RemoteFile {
        id: id.into(),
        name: name.into(),
        modified,
        is_folder: false,
        sha256: sha.map(str::to_owned),
        version: None,
    }
}

fn unverified(name: &str) -> Result<Vec<RemoteFile>, UnverifiedDuplicates> {
    Err(UnverifiedDuplicates { name: name.into() })
}

#[test]
fn paths_reject_absolute_and_traversal_names() {
    for invalid in [
        "", " x", "x ", "/save", "\\save", "C:save", "a//b", "a/../b", "a/./b", "a/", "a\0b",
    ] {
        assert_eq!(remote_path_segments(invalid), None, "{invalid:?}");
    }
    assert_eq!(
        remote_path_segments("slot\\sub/save.dat"),
        Some(vec!["slot", "sub", "save.dat"])
    );
}

#[test]
fn duplicate_remote_files_require_verified_equal_content() {
    let file = RemoteFile {
        id: "a".into(),
        name: "save".into(),
        modified: 3,
        is_folder: false,
        sha256: Some("ABCD".into()),
        version: None,
    };
    let mut other = file.clone();
    other.id = "b".into();
    other.sha256 = Some("abcd".into());
    assert_eq!(
        canonical_cloud_files(&[file.clone(), other.clone()]).unwrap(),
        vec![other.clone()]
    );
    other.sha256 = None;
    assert_eq!(
        canonical_cloud_files(&[file.clone(), other.clone()]),
        Err(UnverifiedDuplicates {
            name: "save".into()
        })
    );
    other.sha256 = Some("different".into());
    assert_eq!(
        canonical_cloud_files(&[file, other]),
        Err(UnverifiedDuplicates {
            name: "save".into()
        })
    );
}

#[test]
fn canonical_cloud_files_keep_first_appearance_order_and_break_ties_by_utf16_id() {
    let remote = |id: &str, name: &str| RemoteFile {
        id: id.into(),
        name: name.into(),
        modified: 7,
        is_folder: false,
        sha256: Some("aa".into()),
        version: None,
    };
    // U+FF5E sorts before U+1F600 by code point but after it in UTF-16 (0xFF5E > surrogate 0xD83D).
    let files = [
        remote("\u{1F600}", "b"),
        remote("x", "a"),
        remote("\u{FF5E}", "b"),
        remote("y", "c"),
    ];
    let ids: Vec<_> = canonical_cloud_files(&files)
        .unwrap()
        .into_iter()
        .map(|f| f.id)
        .collect();
    assert_eq!(ids, ["\u{FF5E}", "x", "y"]);
}

#[test]
fn remote_paths_reject_traversal_and_absolute_variants_with_either_separator() {
    for invalid in [
        "..",
        ".",
        "..\\x",
        "a\\..\\b",
        "a/b\\..",
        "a/..",
        "./a",
        "a/ /b",
        "a\\",
        "\t",
        "c:",
        "C:\\Windows",
        "C:/x",
        "a/b\0",
        "\u{a0}x",
        "x\u{3000}",
    ] {
        assert_eq!(remote_path_segments(invalid), None, "{invalid:?}");
    }
}

#[test]
fn remote_paths_accept_dotted_and_spaced_names_that_are_not_traversal() {
    for (input, expected) in [
        ("save.dat", vec!["save.dat"]),
        ("..hidden", vec!["..hidden"]),
        ("...", vec!["..."]),
        ("a..b", vec!["a..b"]),
        (".config", vec![".config"]),
        ("a b/c d", vec!["a b", "c d"]),
        ("a\\b\\c", vec!["a", "b", "c"]),
        ("1:save", vec!["1:save"]),
        ("CC:save", vec!["CC:save"]),
    ] {
        assert_eq!(remote_path_segments(input), Some(expected), "{input:?}");
    }
}

#[test]
fn duplicates_are_refused_when_any_copy_lacks_or_differs_in_hash() {
    assert_eq!(
        canonical_cloud_files(&[remote("a", "s", 1, None), remote("b", "s", 1, Some("aa"))]),
        unverified("s")
    );
    assert_eq!(
        canonical_cloud_files(&[remote("a", "s", 1, None), remote("b", "s", 1, None)]),
        unverified("s")
    );
    assert_eq!(
        canonical_cloud_files(&[
            remote("a", "s", 1, Some("aa")),
            remote("b", "s", 1, Some("AA")),
            remote("c", "s", 1, Some("bb")),
        ]),
        unverified("s")
    );
    assert_eq!(
        canonical_cloud_files(&[
            remote("1", "ok", 1, None),
            remote("2", "bad1", 1, None),
            remote("3", "bad2", 1, None),
            remote("4", "bad2", 1, None),
            remote("5", "bad1", 1, None),
        ]),
        unverified("bad1")
    );
}

#[test]
fn verified_duplicates_keep_newest_copy_and_unique_files_need_no_hash() {
    assert_eq!(
        canonical_cloud_files(&[
            remote("z", "s", 9, Some("aa")),
            remote("a", "s", 10, Some("AA")),
            remote("m", "s", 2, Some("aa")),
        ]),
        Ok(vec![remote("a", "s", 10, Some("AA"))])
    );
    let ids: Vec<_> = canonical_cloud_files(&[
        remote("1", "b", 1, None),
        remote("2", "a", 5, Some("aa")),
        remote("3", "a", 6, Some("AA")),
        remote("4", "c", 1, None),
    ])
    .unwrap()
    .into_iter()
    .map(|f| f.id)
    .collect();
    assert_eq!(ids, ["1", "3", "4"]);
    assert_eq!(canonical_cloud_files(&[]), Ok(vec![]));
}

#[test]
fn cloud_names_differing_only_in_case_collide_where_the_local_file_system_ignores_case() {
    let upper = remote("a", "Save.dat", 1, Some("aa"));
    let lower = remote("b", "save.dat", 2, Some("aa"));
    let files = [upper.clone(), lower.clone()];

    // Case-sensitive file system: two distinct files, both kept.
    assert_eq!(
        canonical_cloud_files_with(&files, false).unwrap(),
        vec![upper.clone(), lower.clone()]
    );
    // Case-insensitive: one local file, so one copy, the newest.
    assert_eq!(
        canonical_cloud_files_with(&files, true).unwrap(),
        vec![lower.clone()]
    );

    // Different or unverifiable content must never be resolved by guessing.
    for other_hash in [Some("bb"), None] {
        let conflicting = remote("b", "save.dat", 2, other_hash);
        assert_eq!(
            canonical_cloud_files_with(&[upper.clone(), conflicting.clone()], true),
            unverified("Save.dat")
        );
        assert_eq!(
            canonical_cloud_files_with(&[upper.clone(), conflicting], false)
                .unwrap()
                .len(),
            2
        );
    }
}

#[test]
fn name_keys_fold_one_to_one_like_ntfs_and_never_merge_distinct_names() {
    assert_eq!(name_key("Save.DAT", true), name_key("save.dat", true));
    assert_eq!(name_key("Étoile", true), name_key("éTOILE", true));
    // NTFS keeps these distinct: the uppercase of ß expands, so it must not fold.
    assert_ne!(name_key("straße", true), name_key("STRASSE", true));
    assert_ne!(name_key("a", true), name_key("b", true));
    // Case-sensitive comparison leaves names untouched.
    assert_ne!(name_key("Save", false), name_key("save", false));
    assert_eq!(name_key("Save", false), "Save");
}

#[test]
fn windows_file_names_reject_what_windows_cannot_or_will_not_create_verbatim() {
    for invalid in [
        "",
        "CON",
        "con",
        "Nul.txt",
        "aux.tar.gz",
        "COM1",
        "lpt9.log",
        "COM0",
        "COM\u{b9}",
        "CON .txt",
        "CONIN$",
        "conout$.x",
        "save.dat:stream",
        "a<b",
        "a>b",
        "a\"b",
        "a|b",
        "a?b",
        "a*b",
        "a/b",
        "a\\b",
        "tab\there",
        "ctl\u{1f}",
        "trailing.",
        "trailing ",
    ] {
        assert!(!is_windows_file_name(invalid), "{invalid:?}");
    }
    for valid in [
        "save.dat",
        "CONSOLE",
        "COM10",
        "LPT",
        "COMX",
        "nul_save",
        "console.sav",
        ".hidden",
        "a b",
        "..hidden",
        "ÉTÉ.sav",
        "slot-1",
    ] {
        assert!(is_windows_file_name(valid), "{valid:?}");
    }
}
