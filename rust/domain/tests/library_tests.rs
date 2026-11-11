use emusync_domain::library::{
    Emulator, EntryCommon, NativeGame, SaveTarget, SaveTargetType, clean_game_title,
    folder_cloud_names, is_excluded, normalize_game_key, scope_new_targets,
};

fn target(path: &str, kind: SaveTargetType, cloud: Option<&str>) -> SaveTarget {
    SaveTarget {
        path: path.into(),
        kind,
        cloud_path: cloud.map(str::to_owned),
        excluded_folders: Vec::new(),
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
fn clean_title_matches_kotlin_literals_for_hostile_extensions_and_tags() {
    for (input, expected) in [
        ("Game.v1.0", "Game.v1"),
        ("Game.backup", "Game.backup"),
        ("a.12345", "a"),
        ("a.123456", "a.123456"),
        ("game.sfć", "game.sfć"),
        ("Mario.Bros.sfc", "Mario.Bros"),
        ("no_ext.", "no ext."),
        (".sfc", ".sfc"),
        ("(DISC2)", "(DISC2)"),
        ("(Disc)", "(Disc)"),
        ("Night (Disco 1)", "Night"),
        ("Game (Part II)", "Game"),
        ("Game (cd 12)", "Game (cd 12)"),
        ("Game (Disc\t3)", "Game (Disc 3)"),
        ("Game (Disc 1x)", "Game (Disc 1x)"),
        ("Game (Disc 1) (Disc 2)", "Game (Disc 1) (Disc 2)"),
        ("Game (Disc 1", "Game (Disc 1"),
        ("Game (USA", "Game (USA"),
        ("Game [b1", "Game [b1"),
        ("Game (USA (Disc 1))", "Game )"),
        ("(Disc 1 (USA))", "(Disc 1 )"),
        ("Game ()", "Game"),
        ("Game []", "Game"),
        ("ゼルダの伝説_(Japan)_[!].sfc", "ゼルダの伝説"),
        ("[é]é(é)é", "éé"),
    ] {
        assert_eq!(clean_game_title(input), expected, "input {input:?}");
    }
}

#[test]
fn normalized_key_reduces_everything_but_ascii_alphanumerics_to_single_spaces() {
    for (input, expected) in [
        ("SUPER-MARIO_world!!", "super mario world"),
        ("Super Mario World (USA).sfc", "super mario world"),
        ("Pokémon", "pok mon"),
        ("ＡＢＣ", ""),
        ("ゼルダの伝説.sfc", ""),
    ] {
        assert_eq!(normalize_game_key(input), expected, "input {input:?}");
    }
}

#[test]
fn normalized_title_lookup_skips_blank_matches_and_never_matches_empty_keys() {
    let emulator = Emulator {
        title_by_rom: vec![
            ("Super Mario World (USA).sfc".into(), " ".into()),
            ("Super_Mario_World_[U].sfc".into(), " First title ".into()),
            (
                "Super Mario World (Europe).sfc".into(),
                "Second title".into(),
            ),
        ],
        ..Emulator::default()
    };
    assert_eq!(
        emulator.effective_title("Super Mario World.sfc", "x"),
        "First title"
    );
    let emulator = Emulator {
        title_by_rom: vec![("???".into(), "Wrong".into())],
        ..Emulator::default()
    };
    assert_eq!(
        emulator.effective_title("!!!.sfc", "Fallback (USA).sfc"),
        "Fallback"
    );
}

#[test]
fn emulator_constructor_keeps_rom_placeholder_argument() {
    let emulator = Emulator::new("GBA", "/bin/mgba", "/roms/gba", vec!["gba".into()]);
    assert_eq!(emulator.common.name, "GBA");
    assert_eq!(emulator.common.executable_path, "/bin/mgba");
    assert_eq!(emulator.common.arguments, ["{ROM}"]);
    assert_eq!(emulator.roms_directory, "/roms/gba");
    assert_eq!(emulator.extensions, ["gba"]);
    assert!(emulator.title_by_rom.is_empty());
    assert!(emulator.save_targets_by_rom.is_empty());
    assert_eq!(Emulator::default().common.arguments, ["{ROM}"]);
    assert!(NativeGame::default().common.arguments.is_empty());
}

#[test]
fn effective_cloud_folder_falls_back_to_name_when_overrides_are_blank() {
    let mut common = EntryCommon {
        name: "Citra".into(),
        group: Some("   ".into()),
        cloud_folder: Some("   ".into()),
        ..EntryCommon::default()
    };
    assert_eq!(common.effective_cloud_folder(), "Citra");
    common.group = Some("  Nintendo 3DS  ".into());
    assert_eq!(common.effective_cloud_folder(), "Nintendo 3DS");
}

#[test]
#[ignore = "known Kotlin divergence in clean_game_title; needs a fix in src/library.rs"]
fn clean_title_matches_kotlin_for_interleaved_tags_and_unicode_spaces() {
    let mut failures = Vec::new();
    for (input, expected) in [
        ("(a [b) c]", "(a"),
        ("Game (Disc [b1] 2)", "Game (Disc 2)"),
        ("A\u{3000}B (Japan).sfc", "A\u{3000}B"),
        ("(Disc\u{a0}1)", "(Disc\u{a0}1)"),
    ] {
        let actual = clean_game_title(input);
        if actual != expected {
            failures.push(format!("{input:?}: expected {expected:?}, got {actual:?}"));
        }
    }
    assert!(failures.is_empty(), "divergences:\n{}", failures.join("\n"));
}

fn cloud_paths(targets: &[SaveTarget]) -> Vec<Option<&str>> {
    targets.iter().map(|t| t.cloud_path.as_deref()).collect()
}

#[test]
fn new_directories_are_named_after_where_their_paths_differ() {
    use SaveTargetType::{Directory, File};
    let input = vec![
        target(
            "/run/media/deck/sd/Saves/savedata/1/CUSA13795",
            Directory,
            None,
        ),
        target(
            "/run/media/deck/sd/Saves/trophies/CUSA13795/",
            Directory,
            None,
        ),
        target("/run/media/deck/sd/Saves/config.ini", File, None),
    ];

    let scoped = scope_new_targets(&input, &[true, true, true]);

    assert_eq!(
        cloud_paths(&scoped),
        vec![Some("savedata"), Some("trophies"), None]
    );
    // Reapplying changes nothing, and Windows paths give the same names.
    assert_eq!(scope_new_targets(&scoped, &[true, true, true]), scoped);
    assert_eq!(
        folder_cloud_names(&[
            r"D:\Games\Saves\savedata\1\CUSA13795",
            r"D:\Games\Saves\trophies\CUSA13795"
        ]),
        vec!["savedata", "trophies"]
    );
}

#[test]
fn names_do_not_depend_on_the_order_folders_were_added() {
    use SaveTargetType::Directory;
    let savedata = target("/saves/savedata/1/X", Directory, None);
    let trophies = target("/saves/trophies/X", Directory, None);

    let forward = scope_new_targets(&[savedata.clone(), trophies.clone()], &[]);
    let backward = scope_new_targets(&[trophies, savedata], &[]);

    assert_eq!(
        cloud_paths(&forward),
        vec![Some("savedata"), Some("trophies")]
    );
    assert_eq!(
        cloud_paths(&backward),
        vec![Some("trophies"), Some("savedata")]
    );
}

#[test]
fn saved_directories_keep_their_cloud_layout_and_only_new_ones_are_scoped() {
    use SaveTargetType::Directory;
    // A folder already synced at the cloud root keeps it; the one added next does not
    // share it.
    let input = vec![
        target("/saves/savedata/1/X", Directory, None),
        target("/saves/trophies/X", Directory, None),
    ];

    assert_eq!(
        cloud_paths(&scope_new_targets(&input, &[false, true])),
        vec![None, Some("trophies")]
    );
    assert_eq!(scope_new_targets(&input, &[false, false]), input);
    // A single directory stays at the cloud root.
    assert_eq!(scope_new_targets(&input[..1], &[true]), input[..1].to_vec());
}

#[test]
fn scoped_names_never_overlap_paths_already_in_use() {
    use SaveTargetType::{Directory, File, Unknown};
    let input = vec![
        target("/s/savedata", Directory, None),
        target("/s/savedata/savedata", Directory, None),
        target("/s/trophies", Directory, None),
        target("/s/other", Directory, Some("trophies/x")),
        target("/s/slot.sav", File, Some("savedata-2")),
        target("/s/unknown", Unknown, None),
        target("/s/blank", Directory, Some("  ")),
    ];

    let scoped = scope_new_targets(&input, &[]);

    assert_eq!(
        cloud_paths(&scoped),
        vec![
            Some("savedata"),
            Some("savedata-3"),
            Some("trophies-2"),
            Some("trophies/x"),
            Some("savedata-2"),
            None,
            Some("blank"),
        ]
    );
}

#[test]
fn excluded_folders_hide_only_files_strictly_inside_them() {
    let excluded = vec!["1/CUSA/RaceReplay".to_string()];
    for inside in [
        "1/CUSA/RaceReplay/race.rep",
        "1/CUSA/RaceReplay/deep/nested.rep",
        "1\\CUSA\\RaceReplay\\race.rep",
    ] {
        assert!(is_excluded(&excluded, inside), "{inside}");
    }
    for outside in [
        "1/CUSA/RaceReplayIndex/index.dat",
        "1/CUSA/savedata0/SYS-DATA.sav",
        "1/CUSA/RaceReplay",
        "RaceReplay/race.rep",
    ] {
        assert!(!is_excluded(&excluded, outside), "{outside}");
    }
    let separators = vec!["./1\\CUSA/RaceReplay/".to_string()];
    assert!(is_excluded(&separators, "1/CUSA/RaceReplay/race.rep"));
}

#[test]
fn a_blank_excluded_folder_never_hides_the_whole_save() {
    for blank in ["", "/", ".", "./"] {
        assert!(!is_excluded(&[blank.to_string()], "save.dat"), "{blank:?}");
    }
}
