mod common;

use emusync_platform_desktop::{BinaryVdf, SteamShortcut, VdfValue};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, ErrorKind},
    sync::Barrier,
};
use tempfile::tempdir;

#[test]
fn round_trip_encode_and_decode_steam_shortcuts_binary_vdf() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("shortcuts.vdf");

    let mut tags1 = BTreeMap::new();
    tags1.insert("0".into(), "EmuSync".into());
    tags1.insert("1".into(), "Emulator".into());

    let mut extra = BTreeMap::new();
    extra.insert("FutureField".into(), VdfValue::String("preserved".into()));

    let original = vec![
        SteamShortcut {
            app_id: -123456789,
            app_name: "[EmuSync] Duckstation".into(),
            exe: "\"/usr/bin/duckstation\"".into(),
            start_dir: "/home/user".into(),
            launch_options: "--bigpicture -fullscreen".into(),
            tags: tags1,
            extra_fields: extra,
            ..SteamShortcut::default()
        },
        SteamShortcut {
            app_id: -987654321,
            app_name: "Spelunky 2".into(),
            exe: "\"/games/spelunky/spelunky2\"".into(),
            start_dir: "/games/spelunky".into(),
            ..SteamShortcut::default()
        },
    ];

    BinaryVdf::write(&file, &original).unwrap();
    let read_back = BinaryVdf::read(&file).unwrap();

    assert_eq!(original.len(), read_back.len());
    for (orig, copy) in original.iter().zip(read_back.iter()) {
        assert_eq!(orig.app_id, copy.app_id);
        assert_eq!(orig.app_name, copy.app_name);
        assert_eq!(orig.exe, copy.exe);
        assert_eq!(orig.start_dir, copy.start_dir);
        assert_eq!(orig.launch_options, copy.launch_options);
        assert_eq!(orig.tags, copy.tags);
        assert_eq!(orig.extra_fields, copy.extra_fields);
    }
}

#[test]
fn append_new_shortcut_and_preserve_existing_shortcuts() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("shortcuts.vdf");

    let existing = vec![SteamShortcut {
        app_id: -111111,
        app_name: "Spotify".into(),
        exe: "\"/usr/bin/spotify\"".into(),
        start_dir: "/home/user".into(),
        ..SteamShortcut::default()
    }];

    BinaryVdf::write(&file, &existing).unwrap();

    let new_entry = SteamShortcut {
        app_id: -222222,
        app_name: "[EmuSync] PCSX2".into(),
        exe: "\"/usr/bin/pcsx2\"".into(),
        start_dir: "/home/user".into(),
        ..SteamShortcut::default()
    };

    let mut combined = existing;
    combined.push(new_entry);

    BinaryVdf::write(&file, &combined).unwrap();
    let result = BinaryVdf::read(&file).unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].app_name, "Spotify");
    assert_eq!(result[1].app_name, "[EmuSync] PCSX2");
}

#[test]
fn handles_empty_shortcuts_list() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("empty.vdf");
    BinaryVdf::write(&file, &[]).unwrap();

    let result = BinaryVdf::read(&file).unwrap();
    assert!(result.is_empty());
}

#[test]
fn legacy_aliases_remain_fallbacks_and_empty_canonical_fields_retain_precedence() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("shortcuts.vdf");

    // Write manually using binary markers:
    // Root: 0x00 shortcuts\0
    //   0x00 0\0
    //     0x01 appname\0 Legacy\0
    //     0x01 exe\0 legacy.exe\0
    //     0x01 startdir\0 legacy\0
    //   0x08
    // 0x08 0x08
    let mut bytes = Vec::new();
    bytes.push(0x00);
    bytes.extend_from_slice(b"shortcuts\0");
    bytes.push(0x00);
    bytes.extend_from_slice(b"0\0");
    bytes.push(0x01);
    bytes.extend_from_slice(b"appname\0Legacy\0");
    bytes.push(0x01);
    bytes.extend_from_slice(b"exe\0legacy.exe\0");
    bytes.push(0x01);
    bytes.extend_from_slice(b"startdir\0legacy\0");
    bytes.push(0x08);
    bytes.push(0x08);
    bytes.push(0x08);

    fs::write(&file, &bytes).unwrap();
    let legacy = BinaryVdf::read(&file).unwrap().remove(0);
    assert_eq!(legacy.app_name, "Legacy");
    assert_eq!(legacy.exe, "legacy.exe");
    assert_eq!(legacy.start_dir, "legacy");
    assert!(legacy.allow_desktop_config);
    assert!(legacy.allow_overlay);

    // Canonical fields with empty string retain precedence
    let mut bytes2 = Vec::new();
    bytes2.push(0x00);
    bytes2.extend_from_slice(b"shortcuts\0");
    bytes2.push(0x00);
    bytes2.extend_from_slice(b"0\0");
    bytes2.push(0x01);
    bytes2.extend_from_slice(b"appname\0Legacy\0");
    bytes2.push(0x01);
    bytes2.extend_from_slice(b"AppName\0\0"); // Empty canonical AppName
    bytes2.push(0x01);
    bytes2.extend_from_slice(b"Exe\0new.exe\0");
    bytes2.push(0x01);
    bytes2.extend_from_slice(b"StartDir\0new\0");
    bytes2.push(0x08);
    bytes2.push(0x08);
    bytes2.push(0x08);

    fs::write(&file, &bytes2).unwrap();
    let canonical = BinaryVdf::read(&file).unwrap().remove(0);
    assert_eq!(canonical.app_name, "");
    assert_eq!(canonical.exe, "new.exe");
    assert_eq!(canonical.start_dir, "new");
}

#[test]
fn interrupted_vdf_replacement_leaves_complete_previous_file() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("shortcuts.vdf");

    let original = vec![SteamShortcut {
        app_id: 1,
        app_name: "Original".into(),
        exe: "game".into(),
        start_dir: ".".into(),
        ..SteamShortcut::default()
    }];
    BinaryVdf::write(&file, &original).unwrap();
    let bytes = fs::read(&file).unwrap();

    let updated = vec![SteamShortcut {
        app_id: 1,
        app_name: "New".into(),
        exe: "game".into(),
        start_dir: ".".into(),
        ..SteamShortcut::default()
    }];

    let err = BinaryVdf::write_with_hook(&file, &updated, |_| Err(io::Error::other("disk error")));

    assert!(err.is_err());
    assert_eq!(fs::read(&file).unwrap(), bytes);

    let entries: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["shortcuts.vdf"]);
}

#[test]
fn nul_in_vdf_fields_is_rejected_before_touching_original_file() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("shortcuts.vdf");

    let original = vec![SteamShortcut {
        app_id: 1,
        app_name: "Original".into(),
        exe: "game".into(),
        start_dir: ".".into(),
        ..SteamShortcut::default()
    }];
    BinaryVdf::write(&file, &original).unwrap();
    let bytes = fs::read(&file).unwrap();

    let invalid = vec![SteamShortcut {
        app_id: 1,
        app_name: "bad\0value".into(),
        exe: "game".into(),
        start_dir: ".".into(),
        ..SteamShortcut::default()
    }];

    let result = BinaryVdf::write(&file, &invalid);
    assert!(result.is_err());
    assert_eq!(fs::read(&file).unwrap(), bytes);
}

#[test]
fn truncated_vdf_values_are_rejected() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("shortcuts.vdf");

    BinaryVdf::write(
        &file,
        &[SteamShortcut {
            app_id: 1,
            app_name: "Original".into(),
            exe: "game".into(),
            start_dir: ".".into(),
            ..SteamShortcut::default()
        }],
    )
    .unwrap();
    let bytes = fs::read(&file).unwrap();

    for len in [1, 5, 12, bytes.len() - 3] {
        let truncated = &bytes[..len];
        fs::write(&file, truncated).unwrap();
        assert!(BinaryVdf::read(&file).is_err());
    }
}

/// Bytes de un atajo con `depth` objetos `extra` anidados dentro de `shortcuts."0"`.
fn nested(depth: usize) -> Vec<u8> {
    let mut bytes = vec![0x00];
    bytes.extend_from_slice(b"shortcuts\0");
    bytes.push(0x00);
    bytes.extend_from_slice(b"0\0");
    bytes.push(0x01);
    bytes.extend_from_slice(b"AppName\0Deep\0");
    for _ in 0..depth {
        bytes.push(0x00);
        bytes.extend_from_slice(b"extra\0");
    }
    bytes.extend(std::iter::repeat_n(0x08, depth));
    // Fin del atajo, fin de `shortcuts` y el cierre final que escribe Steam.
    bytes.extend_from_slice(&[0x08, 0x08, 0x08]);
    bytes
}

/// Un único atajo con un campo crudo (`marker`, nombre y valor ya codificado).
fn shortcut_bytes_with(extra: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00];
    bytes.extend_from_slice(b"shortcuts\0");
    bytes.push(0x00);
    bytes.extend_from_slice(b"0\0");
    bytes.extend_from_slice(extra);
    bytes.extend_from_slice(&[0x08, 0x08, 0x08]);
    bytes
}

fn named(id: i32, name: &str) -> SteamShortcut {
    SteamShortcut {
        app_id: id,
        app_name: name.into(),
        exe: "game".into(),
        start_dir: ".".into(),
        ..SteamShortcut::default()
    }
}

#[test]
fn nesting_up_to_64_levels_is_accepted_and_65_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("shortcuts.vdf");

    let accepted = BinaryVdf::read_bytes(&nested(64)).unwrap();
    assert_eq!(accepted.len(), 1);
    assert!(accepted[0].extra_fields.contains_key("extra"));

    BinaryVdf::write(&file, &accepted).unwrap();
    let round_trip = BinaryVdf::read(&file).unwrap();
    assert_eq!(round_trip[0].extra_fields, accepted[0].extra_fields);

    let err = BinaryVdf::read_bytes(&nested(65)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
}

#[test]
fn pathologically_deep_nesting_errors_without_stack_overflow() {
    let err = BinaryVdf::read_bytes(&nested(100_000)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidData);
}

#[test]
fn unknown_type_marker_and_wrong_root_are_rejected() {
    let mut uint64 = vec![0x07];
    uint64.extend_from_slice(b"big\0");
    uint64.extend_from_slice(&[0u8; 8]);
    let unknown_marker = shortcut_bytes_with(&uint64);

    let mut wrong_root = unknown_marker.clone();
    wrong_root.splice(1..11, b"shortcutz\0".iter().copied());
    assert_eq!(&wrong_root[1..11], b"shortcutz\0");

    let mut wrong_first_byte = nested(0);
    wrong_first_byte[0] = 0x01;

    for (label, bytes) in [
        ("unknown type marker", unknown_marker),
        ("wrong root name", wrong_root),
        ("wrong first byte", wrong_first_byte),
    ] {
        let err = BinaryVdf::read_bytes(&bytes).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData, "{label}");
    }
}

#[test]
fn rewrite_of_writer_produced_bytes_is_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.vdf");
    let second = dir.path().join("second.vdf");

    let mut inner = BTreeMap::new();
    inner.insert("depth".to_string(), VdfValue::Int(2));
    inner.insert("note".to_string(), VdfValue::String("deep".into()));
    let mut extra = BTreeMap::new();
    extra.insert("Negative".to_string(), VdfValue::Int(-5));
    extra.insert("Text".to_string(), VdfValue::String("keep me".into()));
    extra.insert("Nested".to_string(), VdfValue::Object(inner));
    let mut tags = BTreeMap::new();
    tags.insert("0".to_string(), "EmuSync".to_string());
    let list = vec![
        SteamShortcut {
            tags,
            extra_fields: extra,
            ..named(-5, "[EmuSync] Extras")
        },
        named(9, "Plain"),
    ];

    BinaryVdf::write(&first, &list).unwrap();
    let read_back = BinaryVdf::read(&first).unwrap();
    assert_eq!(read_back, list);
    BinaryVdf::write(&second, &read_back).unwrap();

    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
}

#[test]
fn non_sequential_index_names_are_normalized_preserving_order() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("shortcuts.vdf");
    let mut bytes = vec![0x00];
    bytes.extend_from_slice(b"shortcuts\0");
    for (index, name) in [("5", "First"), ("9", "Second")] {
        bytes.push(0x00);
        bytes.extend_from_slice(index.as_bytes());
        bytes.push(0);
        bytes.push(0x01);
        bytes.extend_from_slice(format!("AppName\0{name}\0").as_bytes());
        bytes.push(0x08);
    }
    bytes.extend_from_slice(&[0x08, 0x08]);

    let list = BinaryVdf::read_bytes(&bytes).unwrap();
    BinaryVdf::write(&file, &list).unwrap();

    let names: Vec<String> = BinaryVdf::read(&file)
        .unwrap()
        .into_iter()
        .map(|s| s.app_name)
        .collect();
    assert_eq!(names, ["First", "Second"]);
    let written = fs::read(&file).unwrap();
    let contains = |needle: &[u8]| written.windows(needle.len()).any(|w| w == needle);
    assert!(contains(b"\x000\0"));
    assert!(contains(b"\x001\0"));
    assert!(!contains(b"\x005\0"));
    assert!(!contains(b"\x009\0"));
}

#[test]
fn legacy_alias_keys_are_not_reemitted_after_rewrite() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("shortcuts.vdf");
    let bytes = shortcut_bytes_with(b"\x01appname\0Legacy\0\x01exe\0legacy.exe\0");

    let list = BinaryVdf::read_bytes(&bytes).unwrap();
    assert_eq!(list[0].app_name, "Legacy");
    assert_eq!(list[0].exe, "legacy.exe");
    assert!(list[0].extra_fields.is_empty());

    BinaryVdf::write(&file, &list).unwrap();
    let written = fs::read(&file).unwrap();
    let contains = |needle: &[u8]| written.windows(needle.len()).any(|w| w == needle);
    assert!(!contains(b"\x01appname\0"));
    assert!(!contains(b"\x01exe\0"));
    assert!(contains(b"\x01AppName\0Legacy\0"));
    assert!(BinaryVdf::read(&file).unwrap()[0].extra_fields.is_empty());
}

#[test]
fn non_string_tag_values_are_dropped_and_string_tags_kept() {
    let mut tags = vec![0x00];
    tags.extend_from_slice(b"tags\0");
    tags.push(0x01);
    tags.extend_from_slice(b"0\0EmuSync\0");
    tags.push(0x02);
    tags.extend_from_slice(b"1\0");
    tags.extend_from_slice(&7i32.to_le_bytes());
    tags.push(0x08);

    let list = BinaryVdf::read_bytes(&shortcut_bytes_with(&tags)).unwrap();

    let mut expected = BTreeMap::new();
    expected.insert("0".to_string(), "EmuSync".to_string());
    assert_eq!(list[0].tags, expected);
    assert!(list[0].extra_fields.is_empty());
}

#[test]
fn concurrent_writers_leave_exactly_one_complete_list_and_no_temporaries() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("shortcuts.vdf");
    let list_a = vec![named(1, "Writer A one"), named(2, "Writer A two")];
    let list_b = vec![
        named(3, "Writer B one"),
        named(4, "Writer B two"),
        named(5, "Writer B three"),
    ];
    let barrier = Barrier::new(2);

    std::thread::scope(|scope| {
        for list in [&list_a, &list_b] {
            let (barrier, file) = (&barrier, &file);
            scope.spawn(move || {
                barrier.wait();
                for _ in 0..50 {
                    BinaryVdf::write(file, list).unwrap();
                }
            });
        }
    });

    let result = BinaryVdf::read(&file).unwrap();
    assert!(result == list_a || result == list_b, "{result:?}");
    assert_eq!(common::dir_entries(dir.path()), ["shortcuts.vdf"]);
}

#[test]
fn write_onto_directory_target_fails_and_leaves_no_temporary() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("shortcuts.vdf");
    fs::create_dir(&target).unwrap();

    let result = BinaryVdf::write(&target, &[named(1, "Doomed")]);

    assert!(result.is_err());
    assert!(target.is_dir());
    assert_eq!(common::dir_entries(dir.path()), ["shortcuts.vdf"]);
    assert_eq!(common::dir_entries(&target), Vec::<String>::new());
}

#[test]
fn write_creates_missing_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a/b/c/shortcuts.vdf");

    BinaryVdf::write(&file, &[named(1, "Nested")]).unwrap();

    let list = BinaryVdf::read(&file).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].app_name, "Nested");
    assert_eq!(
        common::dir_entries(file.parent().unwrap()),
        ["shortcuts.vdf"]
    );
}
