//! `GameItem` title, cover and name resolution.

use emusync_domain::library::{Emulator, EntryCommon, GameEntry, NativeGame};
use emusync_presentation::GameItem;
use std::path::PathBuf;

fn emulator_with(titles: &[(&str, &str)], covers: &[(&str, &str)]) -> GameEntry {
    GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "GBA".to_string(),
            ..EntryCommon::default()
        },
        title_by_rom: titles
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        cover_path_by_rom: covers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..Emulator::default()
    })
}

#[test]
fn game_item_resolves_effective_title_and_cover_path() {
    let emulator = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "GBA".to_string(),
            ..EntryCommon::default()
        },
        title_by_rom: vec![("zelda.gba".to_string(), "The Minish Cap".to_string())],
        cover_path_by_rom: vec![("zelda.gba".to_string(), "/covers/zelda.png".to_string())],
        ..Emulator::default()
    });

    let overridden_item = GameItem::new(
        "zelda.gba",
        emulator.clone(),
        Some(PathBuf::from("/roms/gba/zelda.gba")),
    );
    assert_eq!(overridden_item.effective_title(), "The Minish Cap");
    assert_eq!(
        overridden_item.effective_cover_path(),
        Some("/covers/zelda.png".to_string())
    );

    let default_item = GameItem::new(
        "pokemon.gba",
        emulator,
        Some(PathBuf::from("/roms/gba/pokemon.gba")),
    );
    assert_eq!(default_item.effective_title(), "pokemon");
    assert_eq!(default_item.effective_cover_path(), None);

    let native = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Hades".to_string(),
            ..EntryCommon::default()
        },
        cover_path: Some("/covers/hades.jpg".to_string()),
        ..NativeGame::default()
    });

    let native_item = GameItem::new("Hades", native, None);
    assert_eq!(native_item.effective_title(), "Hades");
    assert_eq!(
        native_item.effective_cover_path(),
        Some("/covers/hades.jpg".to_string())
    );
}

#[test]
fn blank_title_override_falls_back_to_rom_stem() {
    let item = |title: &str| {
        GameItem::new(
            "z.gba",
            emulator_with(&[("z.gba", title)], &[]),
            Some(PathBuf::from("/r/z.gba")),
        )
    };
    assert_eq!(item("   ").effective_title(), "z");
    assert_eq!(item("  Minish  ").effective_title(), "Minish");
}

#[test]
fn blank_scanned_cover_falls_back_to_custom_and_blank_custom_is_none() {
    let rom = Some(PathBuf::from("/r/z.gba"));

    let with_custom = GameItem::new(
        "z.gba",
        emulator_with(&[], &[("z.gba", "/covers/custom.png")]),
        rom.clone(),
    )
    .with_cover_path(Some("  ".to_string()));
    assert_eq!(
        with_custom.effective_cover_path(),
        Some("/covers/custom.png".to_string())
    );

    let blank_custom = GameItem::new("z.gba", emulator_with(&[], &[("z.gba", "  ")]), rom)
        .with_cover_path(Some("  ".to_string()));
    assert_eq!(blank_custom.custom_cover_path(), None);
    assert_eq!(blank_custom.effective_cover_path(), None);
}

#[test]
fn emulator_custom_cover_needs_a_rom_file() {
    let item = GameItem::new(
        "z.gba",
        emulator_with(&[], &[("z.gba", "/covers/custom.png")]),
        None,
    );
    assert_eq!(item.custom_cover_path(), None);
}

#[test]
fn raw_name_strips_only_the_last_extension() {
    let entry = emulator_with(&[], &[]);
    let with_rom = GameItem::new("ignored", entry.clone(), Some(PathBuf::from("/r/a.b.gba")));
    assert_eq!(with_rom.raw_name(), "a.b");

    assert_eq!(
        GameItem::new("Plain Name", entry, None).raw_name(),
        "Plain Name"
    );
}
