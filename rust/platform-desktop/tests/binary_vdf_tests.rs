use emusync_platform_desktop::{BinaryVdf, SteamShortcut, VdfValue};
use std::{collections::BTreeMap, fs, io};
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
