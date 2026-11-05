use emusync_domain::{library::*, sync::*};

fn target(path: &str, kind: SaveTargetType, cloud: Option<&str>) -> SaveTarget {
    SaveTarget {
        path: path.into(),
        kind,
        cloud_path: cloud.map(str::to_owned),
    }
}
#[test]
fn titles_follow_kotlin_fixtures_and_priority() {
    for (input, expected) in [
        ("Super_Mario_World_[U]_[!].sfc", "Super Mario World"),
        ("Crash_Bandicoot_(USA)_(Disc 1)", "Crash Bandicoot (Disc 1)"),
        (
            "Metal_Gear_Solid_(Europe)_(CD 1)",
            "Metal Gear Solid (CD 1)",
        ),
        ("Shenmue (USA) (Part 3)", "Shenmue (Part 3)"),
        ("[!]", "[!]"),
        ("(USA)", "(USA)"),
    ] {
        assert_eq!(clean_game_title(input), expected);
    }
    assert_eq!(
        normalize_game_key("Super_Mario_World_[U].sfc"),
        "super mario world"
    );
    let mut emulator = Emulator {
        title_by_rom: vec![
            ("Super_Mario_World_[U].sfc".into(), " First ".into()),
            ("Super Mario World".into(), " Stem ".into()),
            ("Super Mario World.sfc".into(), " Exact ".into()),
        ],
        ..Emulator::default()
    };
    assert_eq!(
        emulator.effective_title("Super Mario World.sfc", "ignored"),
        "Exact"
    );
    emulator.title_by_rom[2].1 = " ".into();
    assert_eq!(
        emulator.effective_title("Super Mario World.sfc", "ignored"),
        "Stem"
    );
    emulator.title_by_rom[1].1 = " ".into();
    assert_eq!(
        emulator.effective_title("Super Mario World.sfc", "ignored"),
        "First"
    );
    assert_eq!(
        emulator.effective_title("!!!.sfc", "Fallback (USA).sfc"),
        "Fallback"
    );
}
#[test]
fn cloud_folder_and_rom_routing_match_reference_examples() {
    let mut emu = Emulator::default();
    emu.common.name = "RPCS3".into();
    emu.common.group = Some(" PlayStation ".into());
    let entry = GameEntry::Emulator(emu.clone());
    let global = vec![target(" /mnt/Saves/PS3/ ", SaveTargetType::Directory, None)];
    let copied = vec![target("\\mnt\\Saves\\PS3", SaveTargetType::Unknown, None)];
    let distinct = vec![target("/mnt/Saves/PS3/Black", SaveTargetType::File, None)];
    assert_eq!(
        cloud_path_segments(&entry, Some("Black.iso"), Some("Black"), &global, &[]),
        ["PlayStation"]
    );
    assert_eq!(
        cloud_path_segments(&entry, Some("Black.iso"), Some("Black"), &global, &copied),
        ["PlayStation"]
    );
    assert_eq!(
        cloud_path_segments(
            &entry,
            Some("Black.iso"),
            Some("Black/Edition"),
            &global,
            &distinct
        ),
        ["PlayStation", "Black-Edition"]
    );
    assert_eq!(
        cloud_path_segments(&entry, Some("Black.iso"), Some("Black"), &[], &[]),
        ["PlayStation", "Black"]
    );
    let mut native = NativeGame::default();
    native.common.name = "Spelunky Classic".into();
    native.common.group = Some("Indie".into());
    assert_eq!(
        cloud_path_segments(&GameEntry::Native(native.clone()), None, None, &[], &[]),
        ["Indie", "Spelunky Classic"]
    );
    native.common.cloud_folder = Some("Custom".into());
    assert_eq!(
        cloud_path_segments(&GameEntry::Native(native), None, None, &[], &[]),
        ["Custom"]
    );
    emu.common.cloud_folder = Some("   ".into());
    assert_eq!(emu.common.effective_cloud_folder(), "PlayStation");
    assert_eq!(sanitize_cloud_folder_name("  /\\  "), "--");
    assert_eq!(sanitize_cloud_folder_name("   "), "Unknown");
}
#[test]
fn scoping_directories_avoids_existing_prefixes_and_is_stable() {
    let input = vec![
        target("a", SaveTargetType::Directory, Some("save-target-1/sub")),
        target("b", SaveTargetType::Directory, None),
        target("c", SaveTargetType::Directory, None),
        target("d", SaveTargetType::File, None),
    ];
    let result = scope_new_targets(&input);
    assert_eq!(
        result
            .iter()
            .map(|t| t.cloud_path.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("save-target-1/sub"),
            Some("save-target-2"),
            Some("save-target-3"),
            None
        ]
    );
    assert_eq!(scope_new_targets(&result), result);
    assert_eq!(
        scope_new_targets(&[target("a", SaveTargetType::Directory, None)]),
        vec![target("a", SaveTargetType::Directory, None)]
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
