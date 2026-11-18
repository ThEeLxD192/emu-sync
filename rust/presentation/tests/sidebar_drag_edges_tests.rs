//! Dragging a sidebar row past the first or last slot it can take: the row stops at
//! that slot, keeps a partial drag pointing back into the list, and turns back after
//! exactly one row.

use emusync_domain::library::{EntryCommon, GameEntry, NativeGame};
use emusync_presentation::components::sidebar_order::{
    SidebarInteraction, SidebarRow, compute_sidebar_rows,
};
use std::collections::BTreeSet;

const ROW_HEIGHT: f32 = 52.0;

fn game(name: &str, group: Option<&str>) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: format!("/{}", name.to_lowercase()),
            group: group.map(str::to_string),
            ..EntryCommon::default()
        },
        ..NativeGame::default()
    })
}

fn loose(names: &[&str]) -> Vec<GameEntry> {
    names.iter().map(|name| game(name, None)).collect()
}

fn names(entries: &[GameEntry]) -> Vec<&str> {
    entries
        .iter()
        .map(|entry| entry.common().name.as_str())
        .collect()
}

fn entry_row(entries: &[GameEntry], name: &str) -> SidebarRow {
    compute_sidebar_rows(entries, &BTreeSet::new(), None)
        .into_iter()
        .find(|row| matches!(row, SidebarRow::Entry { entry, .. } if entry.common().name == name))
        .unwrap_or_else(|| panic!("no entry row named {name}"))
}

fn folder_row(entries: &[GameEntry], group: &str) -> SidebarRow {
    compute_sidebar_rows(entries, &BTreeSet::new(), None)
        .into_iter()
        .find(
            |row| matches!(row, SidebarRow::FolderHeader { group_name, .. } if group_name == group),
        )
        .unwrap_or_else(|| panic!("no folder row named {group}"))
}

fn dragging(row: &SidebarRow) -> SidebarInteraction {
    let mut state = SidebarInteraction::default();
    state.start_drag(row.key());
    state
}

/// A three-entry folder between two loose entries.
fn middle_folder() -> Vec<GameEntry> {
    vec![
        game("Top", None),
        game("G1", Some("Group")),
        game("G2", Some("Group")),
        game("G3", Some("Group")),
        game("Bottom", None),
    ]
}

#[test]
fn entry_reaching_last_slot_with_leftover_drag_rests_at_its_slot_and_turns_back_after_one_row() {
    let mut entries = loose(&["A", "B", "C", "D"]);
    let row = entry_row(&entries, "C");
    let mut state = dragging(&row);

    entries = state
        .drag_entry(&entries, &row, 60.0, ROW_HEIGHT)
        .expect("one row down");
    assert_eq!(names(&entries), ["A", "B", "D", "C"]);
    assert_eq!(state.drag_offset_y(), 0.0);

    assert_eq!(state.drag_entry(&entries, &row, -51.0, ROW_HEIGHT), None);
    assert_eq!(state.drag_offset_y(), -51.0);
    let back = state
        .drag_entry(&entries, &row, -1.0, ROW_HEIGHT)
        .expect("one whole row back up");
    assert_eq!(names(&back), ["A", "B", "C", "D"]);
    assert_eq!(state.drag_offset_y(), 0.0);
}

#[test]
fn folder_overshooting_the_top_turns_back_after_exactly_one_row() {
    let entries = vec![
        game("G1", Some("Group")),
        game("G2", Some("Group")),
        game("X", None),
        game("Y", None),
    ];
    let row = folder_row(&entries, "Group");
    let mut state = dragging(&row);

    assert_eq!(state.drag_folder(&entries, &row, -500.0, ROW_HEIGHT), None);
    assert_eq!(state.drag_offset_y(), 0.0);
    assert_eq!(state.drag_folder(&entries, &row, 51.0, ROW_HEIGHT), None);
    assert_eq!(state.drag_offset_y(), 51.0);

    let moved = state
        .drag_folder(&entries, &row, 1.0, ROW_HEIGHT)
        .expect("one whole row down");
    assert_eq!(names(&moved), ["X", "G1", "G2", "Y"]);
    assert_eq!(state.drag_offset_y(), 0.0);
}

#[test]
fn nested_entry_stops_at_its_folder_edges_even_when_the_folder_is_in_the_middle() {
    let mut entries = middle_folder();
    let row = entry_row(&entries, "G2");
    let mut state = dragging(&row);

    entries = state
        .drag_entry(&entries, &row, -60.0, ROW_HEIGHT)
        .expect("first in its folder");
    assert_eq!(names(&entries), ["Top", "G2", "G1", "G3", "Bottom"]);
    assert_eq!(state.drag_offset_y(), 0.0);

    assert_eq!(state.drag_entry(&entries, &row, -500.0, ROW_HEIGHT), None);
    assert_eq!(state.drag_offset_y(), 0.0);
    entries = state
        .drag_entry(&entries, &row, ROW_HEIGHT, ROW_HEIGHT)
        .expect("one whole row back down");
    assert_eq!(names(&entries), ["Top", "G1", "G2", "G3", "Bottom"]);

    entries = state
        .drag_entry(&entries, &row, 1_000.0, ROW_HEIGHT)
        .expect("last in its folder");
    assert_eq!(names(&entries), ["Top", "G1", "G3", "G2", "Bottom"]);
    assert_eq!(state.drag_offset_y(), 0.0);

    assert_eq!(state.drag_entry(&entries, &row, 500.0, ROW_HEIGHT), None);
    assert_eq!(state.drag_offset_y(), 0.0);
    let back = state
        .drag_entry(&entries, &row, -ROW_HEIGHT, ROW_HEIGHT)
        .expect("one whole row back up");
    assert_eq!(names(&back), ["Top", "G1", "G2", "G3", "Bottom"]);
}

/// Deterministic pseudo-random drag deltas, mostly within a few rows with
/// occasional flings far past either end.
fn scripted_deltas() -> Vec<f32> {
    let mut seed: u32 = 0x5eed;
    (0..300)
        .map(|_| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let roll = (seed >> 8) % 1_000;
            match roll {
                0..=29 => 5_000.0,
                30..=59 => -5_000.0,
                _ => (roll % 241) as f32 - 120.0,
            }
        })
        .collect()
}

#[test]
fn long_top_level_drag_keeps_the_row_within_one_row_of_its_slot_and_others_in_order() {
    let mut entries = loose(&["A", "B", "C", "D", "E"]);
    let row = entry_row(&entries, "C");
    let mut state = dragging(&row);
    let mut visited_slots = BTreeSet::new();

    for (step, delta) in scripted_deltas().into_iter().enumerate() {
        if let Some(reordered) = state.drag_entry(&entries, &row, delta, ROW_HEIGHT) {
            assert_ne!(reordered, entries, "step {step}: reorder without a move");
            entries = reordered;
        }
        let order = names(&entries);
        let slot = order.iter().position(|name| *name == "C").unwrap();
        visited_slots.insert(slot);
        let others: Vec<&str> = order.iter().copied().filter(|name| *name != "C").collect();
        let offset = state.drag_offset_y();
        assert_eq!(others, ["A", "B", "D", "E"], "step {step}");
        assert!(offset.abs() < ROW_HEIGHT, "step {step}: offset {offset}");
        if slot == 0 {
            assert!(offset >= 0.0, "step {step}: first slot offset {offset}");
        }
        if slot == 4 {
            assert!(offset <= 0.0, "step {step}: last slot offset {offset}");
        }
    }
    assert_eq!(visited_slots, BTreeSet::from([0, 1, 2, 3, 4]));
}

#[test]
fn non_finite_item_height_neither_reorders_nor_stalls_a_later_drag_in_a_middle_slot() {
    let entries = loose(&["A", "B", "C", "D"]);
    let row = entry_row(&entries, "B");
    for bad_height in [f32::NAN, f32::INFINITY] {
        let mut state = dragging(&row);

        assert_eq!(
            state.drag_entry(&entries, &row, 10.0, bad_height),
            None,
            "height {bad_height}"
        );
        let moved = state.drag_entry(&entries, &row, ROW_HEIGHT, ROW_HEIGHT);
        assert_eq!(
            moved.as_deref().map(names),
            Some(vec!["A", "C", "B", "D"]),
            "height {bad_height}: offset {}",
            state.drag_offset_y()
        );
    }
}
