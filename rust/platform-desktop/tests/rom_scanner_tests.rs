mod common;

use emusync_platform_desktop::scan_roms;
use std::fs;
use tempfile::tempdir;

#[test]
fn scans_rom_files_recursively_and_ignores_non_rom_files() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    let files_to_create = [
        "pokemon_emerald.gba",
        "metroid_fusion.gba",
        "zelda_minish_cap.GBA", // uppercase extension should match
        "advance_wars.gbc",
        "readme.txt",                // excluded
        "cover.png",                 // excluded
        "subfolder/hidden_game.gba", // recursive match
    ];

    for relative in &files_to_create {
        let p = root.join(relative);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, b"ROM").unwrap();
    }

    let exts = vec!["gba".into(), "gbc".into()];
    let roms = scan_roms(root, &exts);

    assert_eq!(roms.len(), 5);
    assert!(
        roms.iter()
            .all(|r| r.extension == "gba" || r.extension == "gbc")
    );
    assert!(roms.iter().all(|r| r.name != "readme" && r.name != "cover"));
    assert!(roms.iter().any(|r| r.name == "hidden_game"));

    // Sorted alphabetically
    let names: Vec<String> = roms.iter().map(|r| r.name.to_lowercase()).collect();
    let mut sorted_names = names.clone();
    sorted_names.sort();
    assert_eq!(names, sorted_names);
}

#[test]
fn returns_empty_list_when_scanning_nonexistent_directory() {
    let roms = scan_roms(
        std::path::Path::new("/nonexistent/directory/path/12345"),
        &["gba".into()],
    );
    assert!(roms.is_empty());
}

#[test]
fn handles_empty_extensions_gracefully() {
    let temp = tempdir().unwrap();
    fs::write(temp.path().join("game.gba"), b"ROM").unwrap();
    let roms = scan_roms(temp.path(), &[]);
    assert!(roms.is_empty());
}

#[test]
fn extensions_accept_whitespace_dots_and_mixed_case() {
    let temp = tempdir().unwrap();
    fs::write(temp.path().join("game.GBA"), b"ROM").unwrap();
    let roms = scan_roms(temp.path(), &[" .gBa ".into()]);
    assert_eq!(roms.len(), 1);
    assert_eq!(roms[0].name, "game");
}

#[test]
fn signature_files_identify_complete_games_and_exclude_individual_roms() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    for signature in ["PS3_DISC", "EBOOT", "APP", "MAIN"] {
        let game_folder = root.join(format!("Game-{signature}"));
        fs::create_dir_all(&game_folder).unwrap();
        fs::write(game_folder.join("ordinary.bin"), b"ROM").unwrap();
        let nested = game_folder.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("hidden.bin"), b"ROM").unwrap();
        fs::write(game_folder.join(format!("{signature}.BIN")), b"Signature").unwrap();
    }

    let roms = scan_roms(root, &["bin".into()]);
    // Each game folder becomes a single ROM entry, no nested descent
    assert_eq!(roms.len(), 4);
    assert!(roms.iter().all(|r| r.name.starts_with("Game-")));
}

#[cfg(unix)]
#[test]
fn symlink_cycles_and_aliases_do_not_duplicate_games_or_loop() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    let games = root.join("games");
    fs::create_dir_all(&games).unwrap();
    fs::write(games.join("game.gba"), b"ROM").unwrap();

    let loop_symlink = games.join("loop");
    let alias_symlink = root.join("alias");

    let _ = std::os::unix::fs::symlink(root, loop_symlink);
    let _ = std::os::unix::fs::symlink(&games, alias_symlink);

    let roms = scan_roms(root, &["gba".into()]);
    assert_eq!(roms.len(), 1);
    assert_eq!(roms[0].name, "game");
}

#[test]
fn directory_with_rom_extension_is_descended_not_listed() {
    let temp = tempdir().unwrap();
    let pack = temp.path().join("Pack.gba");
    fs::create_dir_all(&pack).unwrap();
    fs::write(pack.join("inner.gba"), b"ROM").unwrap();

    let roms = scan_roms(temp.path(), &["gba".into()]);

    assert_eq!(roms.len(), 1, "{roms:?}");
    assert_eq!(roms[0].name, "inner");
    assert_eq!(roms[0].file, pack.join("inner.gba"));
}

#[cfg(unix)]
#[test]
fn unreadable_subdirectory_is_skipped_without_losing_other_results() {
    use std::os::unix::fs::PermissionsExt;
    if common::permissions_are_not_enforced() {
        eprintln!("skipped: the test process ignores directory permissions (root?)");
        return;
    }
    struct Restore<'a>(&'a std::path::Path);
    impl Drop for Restore<'_> {
        fn drop(&mut self) {
            let _ = fs::set_permissions(self.0, fs::Permissions::from_mode(0o755));
        }
    }

    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("visible.gba"), b"ROM").unwrap();
    fs::create_dir_all(root.join("open")).unwrap();
    fs::write(root.join("open/also_visible.gba"), b"ROM").unwrap();
    let locked = root.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("secret.gba"), b"ROM").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let _restore = Restore(&locked);

    let names: Vec<String> = scan_roms(root, &["gba".into()])
        .into_iter()
        .map(|r| r.name)
        .collect();

    assert_eq!(names, ["also_visible", "visible"]);
}

#[test]
fn signature_in_root_returns_root_as_single_game() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("EBOOT.BIN"), b"Signature").unwrap();
    fs::write(root.join("ordinary.bin"), b"ROM").unwrap();

    let roms = scan_roms(root, &["bin".into()]);

    assert_eq!(roms.len(), 1, "{roms:?}");
    assert_eq!(roms[0].file, root);
    assert_eq!(
        roms[0].name,
        root.file_name().unwrap().to_string_lossy().as_ref()
    );
    assert_eq!(roms[0].extension, "bin");
}

#[test]
fn duplicate_extension_spellings_do_not_duplicate_results() {
    let temp = tempdir().unwrap();
    fs::write(temp.path().join("game.gba"), b"ROM").unwrap();

    let roms = scan_roms(temp.path(), &["gba".into(), ".GBA".into(), " gba ".into()]);

    assert_eq!(roms.len(), 1, "{roms:?}");
    assert_eq!(roms[0].name, "game");
}
