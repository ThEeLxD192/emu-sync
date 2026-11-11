//! Game edit form: restore previews, custom cover and target scoping.

mod common;

use common::*;
use emusync_domain::library::{
    Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType,
};
use emusync_presentation::{GameEditFormState, GameItem};

fn celeste() -> GameItem {
    GameItem::new("Celeste", native_entry("Celeste"), None)
}

fn files() -> Vec<String> {
    vec!["a.sav".to_string()]
}

#[test]
fn game_edit_restore_waits_for_resolved_paths_and_returns_the_choice() {
    use emusync_presentation::RestoreChoice;

    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "Celeste".to_string(),
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    });
    let game = emusync_presentation::GameItem::new("Celeste", entry, None);
    let mut form = GameEditFormState::new(&game, &[]);
    let cloud_files = vec!["0.celeste".to_string(), "settings.celeste".to_string()];

    // Without a destination nothing can be saved or previewed.
    assert!(!form.can_save(&cloud_files));
    assert_eq!(form.take_restore_preview_request(&cloud_files), None);

    // Per-file restore asks once for the paths resolved by the composition.
    form.restore_destination_folder = " /local/celeste ".to_string();
    assert_eq!(
        form.take_restore_preview_request(&cloud_files),
        Some("/local/celeste".to_string())
    );
    assert_eq!(form.take_restore_preview_request(&cloud_files), None);
    assert!(
        !form.can_save(&cloud_files),
        "unresolved paths block saving"
    );

    // A preview for a destination no longer in the form is ignored.
    form.apply_restore_preview("/elsewhere", vec!["/elsewhere/0.celeste".to_string()]);
    assert!(form.selected_restore_paths(&cloud_files).is_empty());

    // Every name rejected by the resolver: still nothing to save.
    form.apply_restore_preview("/local/celeste", Vec::new());
    assert!(!form.can_save(&cloud_files));

    form.apply_restore_preview(
        "/local/celeste",
        vec!["/local/celeste/0.celeste".to_string()],
    );
    assert!(form.can_save(&cloud_files));
    let result = form.build_result(&cloud_files).expect("restore is ready");
    assert!(
        result.targets.is_empty(),
        "targets are rebuilt by the composition"
    );
    assert_eq!(
        result.restore,
        Some(RestoreChoice {
            destination: "/local/celeste".to_string(),
            whole_folder: false,
        })
    );
    assert_eq!(result.title, Some("Celeste".to_string()));

    // The whole folder needs no resolution.
    form.restore_whole_folder = true;
    assert_eq!(
        form.selected_restore_paths(&cloud_files),
        vec!["/local/celeste"]
    );
    assert_eq!(form.take_restore_preview_request(&cloud_files), None);
    let result = form
        .build_result(&cloud_files)
        .expect("whole folder is ready");
    assert_eq!(result.restore.map(|c| c.whole_folder), Some(true));
}

#[test]
fn game_edit_starts_from_the_custom_cover_only() {
    use std::path::PathBuf;

    let mut emulator = Emulator::new("GBA", "/usr/bin/mgba", "/roms", vec!["gba".into()]);
    emulator.cover_path_by_rom = vec![("zelda.gba".to_string(), "/covers/custom.png".to_string())];
    let rom = Some(PathBuf::from("/roms/zelda.gba"));

    // Auto-detected cover only: the field stays empty so automatic detection keeps working.
    let detected = emusync_presentation::GameItem::new(
        "zelda.gba",
        GameEntry::Emulator(Emulator::new("GBA", "/usr/bin/mgba", "/roms", vec![])),
        rom.clone(),
    )
    .with_cover_path(Some("/roms/zelda.png".to_string()));
    assert_eq!(GameEditFormState::new(&detected, &[]).cover_path, "");

    let custom =
        emusync_presentation::GameItem::new("zelda.gba", GameEntry::Emulator(emulator), rom)
            .with_cover_path(Some("/covers/custom.png".to_string()));
    assert_eq!(
        GameEditFormState::new(&custom, &[]).cover_path,
        "/covers/custom.png"
    );
}

#[test]
fn returning_to_a_resolved_destination_needs_no_new_request() {
    let cloud = files();
    let mut form = GameEditFormState::new(&celeste(), &[]);

    form.restore_destination_folder = "A".to_string();
    assert_eq!(
        form.take_restore_preview_request(&cloud),
        Some("A".to_string())
    );
    form.apply_restore_preview("A", vec!["A/a.sav".to_string()]);

    form.restore_destination_folder = "B".to_string();
    assert_eq!(
        form.take_restore_preview_request(&cloud),
        Some("B".to_string())
    );
    form.restore_destination_folder = "A".to_string();

    assert_eq!(form.take_restore_preview_request(&cloud), None);
    assert_eq!(form.selected_restore_paths(&cloud), vec!["A/a.sav"]);
}

#[test]
fn returning_to_an_unresolved_destination_asks_again() {
    let cloud = files();
    let mut form = GameEditFormState::new(&celeste(), &[]);

    form.restore_destination_folder = "A".to_string();
    assert_eq!(
        form.take_restore_preview_request(&cloud),
        Some("A".to_string())
    );
    form.restore_destination_folder = "B".to_string();
    assert_eq!(
        form.take_restore_preview_request(&cloud),
        Some("B".to_string())
    );
    form.restore_destination_folder = "A".to_string();

    assert_eq!(
        form.take_restore_preview_request(&cloud),
        Some("A".to_string())
    );
}

#[test]
fn no_cloud_files_never_requests_a_preview() {
    let mut form = GameEditFormState::new(&celeste(), &[]);
    form.restore_destination_folder = "/dest".to_string();

    assert_eq!(form.take_restore_preview_request(&[]), None);
    assert!(form.can_save(&[]));

    form.saves.update_path(0, "/saves".to_string());
    form.saves.update_cloud_path(0, "/absolute".to_string());
    assert!(!form.can_save(&[]));
}

#[test]
fn existing_targets_are_not_scoped_and_new_ones_are() {
    let dir = |path: &str| SaveTarget {
        path: path.to_string(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
    };

    let mut existing = GameEditFormState::new(&celeste(), &[dir("/d1"), dir("/d2")]);
    let result = existing.build_result(&[]).expect("valid");
    assert!(result.targets.iter().all(|t| t.cloud_path.is_none()));
    assert_eq!(result.targets.len(), 2);

    let mut fresh = GameEditFormState::new(&celeste(), &[]);
    fresh.saves.update_path(0, "/d1".to_string());
    fresh.saves.update_type(0, SaveTargetType::Directory);
    fresh.saves.add_row();
    fresh.saves.update_path(1, "/d2".to_string());
    fresh.saves.update_type(1, SaveTargetType::Directory);
    let result = fresh.build_result(&[]).expect("valid");
    let clouds: Vec<_> = result
        .targets
        .iter()
        .map(|t| t.cloud_path.as_deref())
        .collect();
    assert_eq!(clouds, vec![Some("save-target-1"), Some("save-target-2")]);
}

#[test]
fn blank_title_and_cover_become_none() {
    let mut form = GameEditFormState::new(&celeste(), &[]);
    form.custom_title = "  ".to_string();
    form.cover_path = " ".to_string();

    let result = form.build_result(&[]).expect("valid");

    assert_eq!(result.title, None);
    assert_eq!(result.cover_path, None);
}

// Parity with Kotlin: `GameEditFormState.canSave` (GameEditFormState.kt:35) checks
// `saves.cloudPathsAreValid` over every row, so an invalid cloud path blocks saving even
// on a row without local path. The entry form deliberately differs (EntryFormState.kt:55-60).
#[test]
fn game_edit_form_blocks_invalid_cloud_path_on_a_row_without_local_path_like_kotlin() {
    let item = GameItem::new("Game", native_entry("Game"), None);
    let mut form = GameEditFormState::new(&item, &[]);
    form.saves.update_cloud_path(0, "../escape".to_string());

    assert!(!form.can_save(&[]));
}
