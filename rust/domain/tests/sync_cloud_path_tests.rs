use emusync_domain::library::{Emulator, GameEntry, NativeGame, SaveTarget, SaveTargetType};
use emusync_domain::sync::{cloud_path_segments, sanitize_cloud_folder_name};

fn target(path: &str, kind: SaveTargetType, cloud: Option<&str>) -> SaveTarget {
    SaveTarget {
        path: path.into(),
        kind,
        cloud_path: cloud.map(str::to_owned),
        excluded_folders: Vec::new(),
    }
}

fn switch_emulator() -> Emulator {
    let mut emulator = Emulator::default();
    emulator.common.name = "Eden".into();
    emulator.common.group = Some("Switch".into());
    emulator
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
fn rom_folder_depends_on_cloud_path_differences_and_requires_a_rom() {
    use SaveTargetType::Directory;
    let entry = GameEntry::Emulator(switch_emulator());
    let global = vec![target("/s", Directory, Some("save-target-1"))];
    assert_eq!(
        cloud_path_segments(
            &entry,
            Some("r.nsp"),
            Some("T"),
            &global,
            &[target("/s", Directory, None)]
        ),
        ["Switch", "T"]
    );
    assert_eq!(
        cloud_path_segments(
            &entry,
            Some("r.nsp"),
            Some("T"),
            &global,
            &[target("/s", Directory, Some("save-target-1"))]
        ),
        ["Switch"]
    );
    assert_eq!(
        cloud_path_segments(
            &entry,
            None,
            Some("T"),
            &[target("/s", Directory, None)],
            &[target("/other", Directory, None)]
        ),
        ["Switch"]
    );
}

#[test]
fn missing_title_uses_override_then_cleaned_rom_and_is_sanitized() {
    let mut emulator = switch_emulator();
    emulator.title_by_rom = vec![("CTR_NF.nsp".into(), " Crash/Team ".into())];
    let entry = GameEntry::Emulator(emulator);
    assert_eq!(
        cloud_path_segments(
            &entry,
            Some("CTR_NF.nsp"),
            None,
            &[],
            &[target("/saves/x", SaveTargetType::Unknown, None)]
        ),
        ["Switch", "Crash-Team"]
    );
    assert_eq!(
        cloud_path_segments(&entry, Some("Other_(USA).nsp"), None, &[], &[]),
        ["Switch", "Other"]
    );
    assert_eq!(
        cloud_path_segments(&entry, Some("r.nsp"), Some("  "), &[], &[]),
        ["Switch", "Unknown"]
    );
    assert_eq!(
        cloud_path_segments(&entry, Some("r.nsp"), Some(" T "), &[], &[]),
        ["Switch", "T"]
    );
}

#[test]
fn native_routing_follows_raw_presence_of_group_and_cloud_folder() {
    let mut native = NativeGame::default();
    native.common.name = "Cave Story".into();
    let route = |native: &NativeGame| {
        cloud_path_segments(
            &GameEntry::Native(native.clone()),
            Some("x"),
            Some("y"),
            &[],
            &[],
        )
    };
    assert_eq!(route(&native), ["Cave Story"]);
    native.common.group = Some("  ".into());
    assert_eq!(route(&native), ["Cave Story", "Cave Story"]);
    native.common.group = Some("Indie".into());
    native.common.cloud_folder = Some("  ".into());
    assert_eq!(route(&native), ["Indie"]);
    native.common.cloud_folder = None;
    native.common.name = "A/B\\C".into();
    assert_eq!(route(&native), ["Indie", "A-B-C"]);
}
