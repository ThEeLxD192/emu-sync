use emusync_domain::library::{CoverFiles, Emulator, GameEntry, NativeGame};
use emusync_platform_desktop::DesktopCoverFiles;
use std::fs;
use tempfile::tempdir;

#[test]
fn resolves_cover_when_image_is_in_rom_directory_with_exact_name() {
    let temp = tempdir().unwrap();
    let rom_dir = temp.path().join("roms");
    fs::create_dir_all(&rom_dir).unwrap();

    let rom_file = rom_dir.join("Crash Bandicoot.iso");
    fs::write(&rom_file, b"dummy").unwrap();

    let cover_file = rom_dir.join("Crash Bandicoot.png");
    fs::write(&cover_file, b"img").unwrap();

    let system = GameEntry::Emulator(Emulator::new(
        "PS1",
        "/bin/true",
        rom_dir.to_str().unwrap(),
        vec!["iso".into()],
    ));

    let finder = DesktopCoverFiles;
    let found = finder
        .find("Crash Bandicoot", &system, Some(rom_file.to_str().unwrap()))
        .unwrap();

    assert_eq!(found, Some(cover_file.to_str().unwrap().to_string()));
}

#[test]
fn resolves_cover_from_custom_covers_directory() {
    let temp = tempdir().unwrap();
    let rom_dir = temp.path().join("roms");
    let covers_dir = temp.path().join("custom_covers");
    fs::create_dir_all(&rom_dir).unwrap();
    fs::create_dir_all(&covers_dir).unwrap();

    let rom_file = rom_dir.join("God of War.iso");
    fs::write(&rom_file, b"dummy").unwrap();

    let cover_file = covers_dir.join("God of War.jpg");
    fs::write(&cover_file, b"img").unwrap();

    let mut emu = Emulator::new(
        "PS2",
        "/bin/true",
        rom_dir.to_str().unwrap(),
        vec!["iso".into()],
    );
    emu.covers_directory = Some(covers_dir.to_str().unwrap().to_string());
    let system = GameEntry::Emulator(emu);

    let finder = DesktopCoverFiles;
    let found = finder
        .find("God of War", &system, Some(rom_file.to_str().unwrap()))
        .unwrap();

    assert_eq!(found, Some(cover_file.to_str().unwrap().to_string()));
}

#[test]
fn resolves_cover_from_covers_subfolder_with_region_tag_stripped() {
    let temp = tempdir().unwrap();
    let rom_dir = temp.path().join("roms");
    let covers_subdir = rom_dir.join("covers");
    fs::create_dir_all(&covers_subdir).unwrap();

    let rom_file = rom_dir.join("Midnight Club 3 - DUB Edition Remix (USA).iso");
    fs::write(&rom_file, b"dummy").unwrap();

    let cover_file = covers_subdir.join("Midnight Club 3 - DUB Edition Remix.webp");
    fs::write(&cover_file, b"img").unwrap();

    let system = GameEntry::Emulator(Emulator::new(
        "PS2",
        "/bin/true",
        rom_dir.to_str().unwrap(),
        vec!["iso".into()],
    ));

    let finder = DesktopCoverFiles;
    let found = finder
        .find(
            "Midnight Club 3 - DUB Edition Remix (USA)",
            &system,
            Some(rom_file.to_str().unwrap()),
        )
        .unwrap();

    assert_eq!(found, Some(cover_file.to_str().unwrap().to_string()));
}

#[test]
fn resolves_cover_for_native_game_with_custom_cover_path() {
    let temp = tempdir().unwrap();
    let game_dir = temp.path().join("spelunky");
    fs::create_dir_all(&game_dir).unwrap();

    let exec_file = game_dir.join("Spelunky.exe");
    fs::write(&exec_file, b"dummy").unwrap();

    let cover_file = temp.path().join("my_custom_cover.png");
    fs::write(&cover_file, b"img").unwrap();

    let mut native = NativeGame::default();
    native.common.name = "Spelunky".into();
    native.common.executable_path = exec_file.to_str().unwrap().to_string();
    native.cover_path = Some(cover_file.to_str().unwrap().to_string());
    let system = GameEntry::Native(native);

    let finder = DesktopCoverFiles;
    let found = finder.find("Spelunky", &system, None).unwrap();

    assert_eq!(found, Some(cover_file.to_str().unwrap().to_string()));
}

#[test]
fn resolves_custom_cover_path_by_rom_with_highest_priority() {
    let temp = tempdir().unwrap();
    let rom_dir = temp.path().join("roms");
    let fallback_dir = rom_dir.join("covers");
    let custom_covers_dir = temp.path().join("custom");
    fs::create_dir_all(&fallback_dir).unwrap();
    fs::create_dir_all(&custom_covers_dir).unwrap();

    let rom_file = rom_dir.join("Pokemon Emerald.gba");
    fs::write(&rom_file, b"dummy").unwrap();
    fs::write(rom_dir.join("Pokemon Emerald.png"), b"same-folder").unwrap();
    fs::write(fallback_dir.join("Pokemon Emerald.png"), b"fallback").unwrap();

    let individual_custom_cover = custom_covers_dir.join("emerald_special.jpg");
    fs::write(&individual_custom_cover, b"custom").unwrap();

    let mut emu = Emulator::new(
        "GBA",
        "/bin/true",
        rom_dir.to_str().unwrap(),
        vec!["gba".into()],
    );
    emu.cover_path_by_rom.push((
        "Pokemon Emerald.gba".into(),
        individual_custom_cover.to_str().unwrap().to_string(),
    ));
    let system = GameEntry::Emulator(emu);

    let finder = DesktopCoverFiles;
    let found = finder
        .find("Pokemon Emerald", &system, Some(rom_file.to_str().unwrap()))
        .unwrap();

    // Priority 1 wins: individual custom cover
    assert_eq!(
        found,
        Some(individual_custom_cover.to_str().unwrap().to_string())
    );
}

#[test]
fn resolves_same_folder_image_over_fallback_subfolder_when_individual_cover_is_not_set() {
    let temp = tempdir().unwrap();
    let rom_dir = temp.path().join("roms");
    let fallback_dir = rom_dir.join("covers");
    fs::create_dir_all(&fallback_dir).unwrap();

    let rom_file = rom_dir.join("Metroid Fusion.gba");
    fs::write(&rom_file, b"dummy").unwrap();

    let same_folder_cover = rom_dir.join("Metroid Fusion.png");
    fs::write(&same_folder_cover, b"same-folder").unwrap();
    fs::write(fallback_dir.join("Metroid Fusion.png"), b"fallback").unwrap();

    let emu = Emulator::new(
        "GBA",
        "/bin/true",
        rom_dir.to_str().unwrap(),
        vec!["gba".into()],
    );
    let system = GameEntry::Emulator(emu);

    let finder = DesktopCoverFiles;
    let found = finder
        .find("Metroid Fusion", &system, Some(rom_file.to_str().unwrap()))
        .unwrap();

    // Priority 2 wins: same folder as ROM
    assert_eq!(found, Some(same_folder_cover.to_str().unwrap().to_string()));
}

#[test]
fn resolves_fallback_subfolder_when_no_individual_cover_and_no_same_folder_image_exists() {
    let temp = tempdir().unwrap();
    let rom_dir = temp.path().join("roms");
    let fallback_dir = rom_dir.join("covers");
    fs::create_dir_all(&fallback_dir).unwrap();

    let rom_file = rom_dir.join("Castlevania - Aria of Sorrow (USA).gba");
    fs::write(&rom_file, b"dummy").unwrap();

    let fallback_cover = fallback_dir.join("Castlevania - Aria of Sorrow.png");
    fs::write(&fallback_cover, b"fallback").unwrap();

    let emu = Emulator::new(
        "GBA",
        "/bin/true",
        rom_dir.to_str().unwrap(),
        vec!["gba".into()],
    );
    let system = GameEntry::Emulator(emu);

    let finder = DesktopCoverFiles;
    let found = finder
        .find(
            "Castlevania - Aria of Sorrow (USA)",
            &system,
            Some(rom_file.to_str().unwrap()),
        )
        .unwrap();

    // Priority 3 wins: fallback in covers/ with region stripped
    assert_eq!(found, Some(fallback_cover.to_str().unwrap().to_string()));
}

#[test]
fn custom_cover_pointing_to_directory_or_missing_file_falls_back_to_adjacent() {
    let temp = tempdir().unwrap();
    let rom_dir = temp.path().join("roms");
    let directory_cover = temp.path().join("not_a_file.png");
    fs::create_dir_all(&rom_dir).unwrap();
    fs::create_dir_all(&directory_cover).unwrap();
    let missing_cover = temp.path().join("missing.png");
    let rom_file = rom_dir.join("Game.iso");
    fs::write(&rom_file, b"dummy").unwrap();
    let adjacent = rom_dir.join("Game.png");
    fs::write(&adjacent, b"img").unwrap();
    let finder = DesktopCoverFiles;

    for broken_custom in [&directory_cover, &missing_cover] {
        let mut emu = Emulator::new("PS1", "/bin/true", rom_dir.to_str().unwrap(), vec![]);
        emu.cover_path_by_rom
            .push(("Game.iso".into(), broken_custom.to_str().unwrap().into()));
        let found = finder
            .find(
                "Game",
                &GameEntry::Emulator(emu),
                Some(rom_file.to_str().unwrap()),
            )
            .unwrap();
        assert_eq!(found, Some(adjacent.to_str().unwrap().to_string()));
    }

    let game_dir = temp.path().join("native");
    fs::create_dir_all(&game_dir).unwrap();
    let native_adjacent = game_dir.join("Hero.png");
    fs::write(&native_adjacent, b"img").unwrap();
    for broken_custom in [&directory_cover, &missing_cover] {
        let mut native = NativeGame::default();
        native.common.name = "Hero".into();
        native.common.executable_path = game_dir.join("Hero.exe").to_str().unwrap().into();
        native.cover_path = Some(broken_custom.to_str().unwrap().into());
        let found = finder
            .find("Hero", &GameEntry::Native(native), None)
            .unwrap();
        assert_eq!(found, Some(native_adjacent.to_str().unwrap().to_string()));
    }
}

#[test]
fn native_custom_cover_path_is_trimmed() {
    let temp = tempdir().unwrap();
    let cover = temp.path().join("c.png");
    fs::write(&cover, b"img").unwrap();
    let mut native = NativeGame::default();
    native.common.name = "Trimmed".into();
    native.common.executable_path = temp
        .path()
        .join("elsewhere/Trimmed.exe")
        .to_str()
        .unwrap()
        .into();
    native.cover_path = Some(format!("  {}  ", cover.display()));

    let found = DesktopCoverFiles
        .find("Trimmed", &GameEntry::Native(native), None)
        .unwrap();

    assert_eq!(found, Some(cover.to_str().unwrap().to_string()));
}

#[test]
fn uppercase_extension_and_case_insensitive_stem_are_found_adjacent() {
    let temp = tempdir().unwrap();
    let rom_dir = temp.path().join("roms");
    fs::create_dir_all(&rom_dir).unwrap();
    let rom_file = rom_dir.join("game.iso");
    fs::write(&rom_file, b"dummy").unwrap();
    let cover = rom_dir.join("GAME.PNG");
    fs::write(&cover, b"img").unwrap();
    let emu = Emulator::new("PS1", "/bin/true", rom_dir.to_str().unwrap(), vec![]);

    let found = DesktopCoverFiles
        .find(
            "game",
            &GameEntry::Emulator(emu),
            Some(rom_file.to_str().unwrap()),
        )
        .unwrap()
        .expect("the cover is found");

    // Same file; Windows may return the requested spelling of a case-insensitive name.
    assert_eq!(
        fs::canonicalize(found).unwrap(),
        fs::canonicalize(&cover).unwrap()
    );
}
