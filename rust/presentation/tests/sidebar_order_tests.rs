//! Sidebar grouping, ordering and interaction contracts ported from Kotlin
//! `SidebarTest` and `SidebarInteractionStateTest`.

use emusync_domain::library::{Emulator, EntryCommon, GameEntry, NativeGame};
use emusync_presentation::components::sidebar_order::{
    SidebarInteraction, SidebarRow, SidebarUnit, compute_sidebar_rows, flatten_units,
    move_child_entry, move_top_level_unit, partition_into_units,
};
use std::collections::BTreeSet;

fn emulator(name: &str, group: Option<&str>, extensions: &[&str]) -> GameEntry {
    GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: format!("/usr/bin/{}", name.to_lowercase()),
            group: group.map(str::to_string),
            ..EntryCommon::default()
        },
        extensions: extensions.iter().map(|e| e.to_string()).collect(),
        ..Emulator::default()
    })
}

fn native(name: &str, group: Option<&str>) -> GameEntry {
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

fn eden() -> GameEntry {
    emulator("Eden", Some("Switch"), &["nsp", "xci"])
}
fn yuzu() -> GameEntry {
    emulator("Yuzu", Some("Switch"), &["nsp", "xci"])
}
fn pcsx2() -> GameEntry {
    emulator("PCSX2", None, &["iso"])
}
fn citra() -> GameEntry {
    emulator("Citra", Some("3DS"), &["3ds"])
}

fn collapsed(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| n.to_string()).collect()
}

fn header_of(rows: &[SidebarRow], index: usize) -> (&str, usize, bool, bool) {
    match &rows[index] {
        SidebarRow::FolderHeader {
            group_name,
            count,
            is_collapsed,
            has_selected_child,
            ..
        } => (group_name, *count, *is_collapsed, *has_selected_child),
        other => panic!("row {index} is not a folder header: {other:?}"),
    }
}

fn entry_of(rows: &[SidebarRow], index: usize) -> (&str, bool) {
    match &rows[index] {
        SidebarRow::Entry {
            entry, is_nested, ..
        } => (&entry.common().name, *is_nested),
        other => panic!("row {index} is not an entry: {other:?}"),
    }
}

#[test]
fn expanded_group_lists_header_then_children_then_standalone() {
    let rows = compute_sidebar_rows(&[eden(), pcsx2(), yuzu()], &BTreeSet::new(), None);

    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows[0],
        SidebarRow::FolderHeader {
            group_name: "Switch".into(),
            count: 2,
            is_collapsed: false,
            has_selected_child: false,
            first_child: Some(eden()),
            unit_index: 0,
            total_units: 2,
        }
    );
    assert!(
        matches!(&rows[1], SidebarRow::Entry { entry, is_nested: true, sibling_index: 0, total_siblings: 2, .. } if *entry == eden())
    );
    assert!(
        matches!(&rows[2], SidebarRow::Entry { entry, is_nested: true, sibling_index: 1, total_siblings: 2, .. } if *entry == yuzu())
    );
    assert!(
        matches!(&rows[3], SidebarRow::Entry { entry, is_nested: false, unit_index: 1, .. } if *entry == pcsx2())
    );
}

#[test]
fn collapsed_group_keeps_header_and_hides_children() {
    let rows = compute_sidebar_rows(&[eden(), yuzu(), pcsx2()], &collapsed(&["Switch"]), None);

    assert_eq!(rows.len(), 2);
    assert_eq!(header_of(&rows, 0), ("Switch", 2, true, false));
    assert_eq!(entry_of(&rows, 1), ("PCSX2", false));
}

#[test]
fn header_survives_collapse_and_reexpand() {
    let entries = [eden(), yuzu()];
    for (set, len) in [
        (BTreeSet::new(), 3),
        (collapsed(&["Switch"]), 1),
        (BTreeSet::new(), 3),
    ] {
        let rows = compute_sidebar_rows(&entries, &set, None);
        assert_eq!(rows.len(), len);
        assert_eq!(header_of(&rows, 0).0, "Switch");
    }
}

#[test]
fn grouping_and_collapse_ignore_case() {
    let yuzu_lower = emulator("Yuzu", Some("switch"), &["nsp"]);
    let entries = [eden(), yuzu_lower];

    let rows = compute_sidebar_rows(&entries, &BTreeSet::new(), None);
    assert_eq!(rows.len(), 3);
    assert_eq!(header_of(&rows, 0).1, 2);

    for name in ["switch", "SWITCH"] {
        let rows = compute_sidebar_rows(&entries, &collapsed(&[name]), None);
        assert_eq!(rows.len(), 1);
        assert!(header_of(&rows, 0).2);
    }
}

#[test]
fn header_marks_selected_child_by_name() {
    let entries = [eden(), yuzu(), pcsx2()];
    let with_eden = compute_sidebar_rows(&entries, &BTreeSet::new(), Some(&eden()));
    assert!(header_of(&with_eden, 0).3);
    let with_pcsx2 = compute_sidebar_rows(&entries, &BTreeSet::new(), Some(&pcsx2()));
    assert!(!header_of(&with_pcsx2, 0).3);
}

#[test]
fn mixed_groups_and_standalone_rows_follow_first_appearance() {
    let entries = [
        native("Hollow Knight", None),
        eden(),
        pcsx2(),
        yuzu(),
        citra(),
    ];
    let rows = compute_sidebar_rows(&entries, &collapsed(&["3DS"]), None);

    assert_eq!(rows.len(), 6);
    assert_eq!(entry_of(&rows, 0), ("Hollow Knight", false));
    assert_eq!(header_of(&rows, 1), ("Switch", 2, false, false));
    assert_eq!(entry_of(&rows, 2), ("Eden", true));
    assert_eq!(entry_of(&rows, 3), ("Yuzu", true));
    assert_eq!(entry_of(&rows, 4), ("PCSX2", false));
    assert_eq!(header_of(&rows, 5), ("3DS", 1, true, false));
}

#[test]
fn partition_gathers_scattered_group_members_and_flatten_makes_them_contiguous() {
    let units = partition_into_units(&[eden(), pcsx2(), yuzu()]);
    assert_eq!(
        units,
        vec![
            SidebarUnit::Group {
                name: "Switch".into(),
                entries: vec![eden(), yuzu()],
            },
            SidebarUnit::Standalone(Box::new(pcsx2())),
        ]
    );
    assert_eq!(flatten_units(units), vec![eden(), yuzu(), pcsx2()]);
}

#[test]
fn blank_group_is_standalone() {
    let blank = emulator("Blank", Some("   "), &[]);
    assert_eq!(
        partition_into_units(std::slice::from_ref(&blank)),
        vec![SidebarUnit::Standalone(Box::new(blank))]
    );
}

#[test]
fn folders_move_as_whole_units_past_entries_and_folders() {
    let entries = [eden(), yuzu(), pcsx2(), citra()];

    let down = move_top_level_unit(&entries, 0, 1);
    assert_eq!(down, vec![pcsx2(), eden(), yuzu(), citra()]);
    let end = move_top_level_unit(&down, 1, 2);
    assert_eq!(end, vec![pcsx2(), citra(), eden(), yuzu()]);
    let top = move_top_level_unit(&end, 2, 0);
    assert_eq!(top, vec![eden(), yuzu(), pcsx2(), citra()]);
}

#[test]
fn children_move_within_their_folder() {
    let ryujinx = emulator("Ryujinx", Some("Switch"), &["nsp"]);
    let entries = [eden(), yuzu(), ryujinx.clone(), pcsx2()];

    let moved = move_child_entry(&entries, "Switch", 0, 1);
    assert_eq!(moved, vec![yuzu(), eden(), ryujinx.clone(), pcsx2()]);
    let top = move_child_entry(&moved, "Switch", 2, 0);
    assert_eq!(top, vec![ryujinx, yuzu(), eden(), pcsx2()]);
}

// ── Interaction state ────────────────────────────────────────────

fn first() -> GameEntry {
    native("First", Some("Group"))
}
fn second() -> GameEntry {
    native("Second", Some("Group"))
}
fn standalone() -> GameEntry {
    native("Standalone", None)
}
fn library() -> Vec<GameEntry> {
    vec![first(), second(), standalone()]
}

fn header(state: &SidebarInteraction, selected: Option<&GameEntry>) -> SidebarRow {
    compute_sidebar_rows(&library(), state.collapsed_groups(), selected)
        .into_iter()
        .next()
        .unwrap()
}

fn entry_row(entry: &GameEntry) -> SidebarRow {
    compute_sidebar_rows(&library(), &BTreeSet::new(), None)
        .into_iter()
        .find(|r| matches!(r, SidebarRow::Entry { entry: e, .. } if e == entry))
        .unwrap()
}

#[test]
fn activating_folder_selects_first_child_then_toggles_collapse_around_selection() {
    let mut state = SidebarInteraction::default();

    assert_eq!(state.activate_folder(&header(&state, None)), Some(first()));
    assert!(state.collapsed_groups().is_empty());

    assert_eq!(state.activate_folder(&header(&state, Some(&first()))), None);
    assert_eq!(state.collapsed_groups(), &collapsed(&["Group"]));
    assert_eq!(state.activate_folder(&header(&state, Some(&first()))), None);
    assert!(state.collapsed_groups().is_empty());
}

#[test]
fn opening_collapsed_unselected_folder_expands_and_selects_first_child() {
    let mut state = SidebarInteraction::default();
    state.toggle_group("Group", false);

    assert_eq!(state.activate_folder(&header(&state, None)), Some(first()));
    assert!(state.collapsed_groups().is_empty());
}

#[test]
fn expanding_folder_removes_collapsed_names_regardless_of_case() {
    let mut state = SidebarInteraction::default();
    state.toggle_group("Group", false);

    state.toggle_group("gRoUp", true);

    assert!(state.collapsed_groups().is_empty());
}

#[test]
fn folder_drag_accumulates_partial_movement_and_moves_whole_group() {
    let mut state = SidebarInteraction::default();
    let row = header(&state, None);
    state.start_drag(row.key());

    assert_eq!(state.drag_folder(&library(), &row, 25.0, 52.0), None);
    assert_eq!(
        state.drag_folder(&library(), &row, 35.0, 52.0),
        Some(vec![standalone(), first(), second()])
    );
    assert_eq!(state.drag_offset_y(), 8.0);

    state.end_drag();
    assert_eq!(state.dragging_key(), None);
    assert_eq!(state.drag_offset_y(), 0.0);
}

#[test]
fn child_drag_stays_within_group_beyond_last_sibling() {
    let mut state = SidebarInteraction::default();
    let row = entry_row(&first());
    state.start_drag(row.key());

    assert_eq!(
        state.drag_entry(&library(), &row, 1_000.0, 52.0),
        Some(vec![second(), first(), standalone()])
    );
    assert_eq!(state.drag_offset_y(), 948.0);
}

#[test]
fn standalone_drag_crosses_whole_group_without_splitting_it() {
    let mut state = SidebarInteraction::default();
    let row = entry_row(&standalone());
    state.start_drag(row.key());

    assert_eq!(
        state.drag_entry(&library(), &row, -52.0, 52.0),
        Some(vec![standalone(), first(), second()])
    );
    assert_eq!(state.drag_offset_y(), 0.0);
}

#[test]
fn entry_removed_during_drag_cannot_reorder_remaining_library() {
    let mut state = SidebarInteraction::default();
    let row = entry_row(&first());
    state.start_drag(row.key());

    assert_eq!(
        state.drag_entry(&[second(), standalone()], &row, 52.0, 52.0),
        None
    );
    assert_eq!(state.drag_offset_y(), 52.0);
}

#[test]
fn drag_without_matching_key_never_reorders() {
    let mut state = SidebarInteraction::default();
    let row = entry_row(&standalone());
    state.start_drag("entry_Other".into());

    assert_eq!(state.drag_entry(&library(), &row, -104.0, 52.0), None);
}

#[test]
fn invalid_sibling_moves_preserve_original_order() {
    let entries = library();
    assert_eq!(move_child_entry(&entries, "Missing", 0, 1), entries);
    assert_eq!(move_child_entry(&entries, "Group", 0, 2), entries);
    assert_eq!(move_child_entry(&entries, "Group", 0, 0), entries);
    assert_eq!(move_top_level_unit(&entries, 0, 5), entries);
}
