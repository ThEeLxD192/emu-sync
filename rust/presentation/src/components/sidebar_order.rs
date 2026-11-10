//! Sidebar grouping, ordering and interaction state, ported from Kotlin's
//! `SidebarOrder.kt` and `SidebarInteractionState.kt`. Pure logic without egui.

use emusync_domain::library::GameEntry;
use std::collections::BTreeSet;

/// A top-level sidebar unit: a folder of entries sharing a group, or a standalone entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarUnit {
    Group {
        name: String,
        entries: Vec<GameEntry>,
    },
    Standalone(Box<GameEntry>),
}

/// A visible sidebar row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarRow {
    FolderHeader {
        group_name: String,
        count: usize,
        is_collapsed: bool,
        has_selected_child: bool,
        first_child: Option<GameEntry>,
        unit_index: usize,
        total_units: usize,
    },
    Entry {
        entry: GameEntry,
        is_nested: bool,
        group_name: Option<String>,
        sibling_index: usize,
        total_siblings: usize,
        unit_index: usize,
        total_units: usize,
    },
}

impl SidebarRow {
    /// Stable identity across reorders, used for drag tracking and widget ids.
    pub fn key(&self) -> String {
        match self {
            Self::FolderHeader { group_name, .. } => {
                format!("folder_{}", group_name.to_lowercase())
            }
            Self::Entry { entry, .. } => format!("entry_{}", entry.common().name),
        }
    }
}

fn group_of(entry: &GameEntry) -> Option<&str> {
    entry
        .common()
        .group
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty())
}

fn same_group(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Splits entries into units; each group appears where its first member appears and
/// collects every member regardless of position.
pub fn partition_into_units(entries: &[GameEntry]) -> Vec<SidebarUnit> {
    let mut units = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in entries {
        match group_of(entry) {
            None => units.push(SidebarUnit::Standalone(Box::new(entry.clone()))),
            Some(name) => {
                if seen.insert(name.to_lowercase()) {
                    let members = entries
                        .iter()
                        .filter(|e| group_of(e).is_some_and(|g| same_group(g, name)))
                        .cloned()
                        .collect();
                    units.push(SidebarUnit::Group {
                        name: name.to_string(),
                        entries: members,
                    });
                }
            }
        }
    }
    units
}

pub fn flatten_units(units: Vec<SidebarUnit>) -> Vec<GameEntry> {
    units
        .into_iter()
        .flat_map(|unit| match unit {
            SidebarUnit::Group { entries, .. } => entries,
            SidebarUnit::Standalone(entry) => vec![*entry],
        })
        .collect()
}

/// Moves a whole unit; invalid or no-op moves return the entries unchanged.
pub fn move_top_level_unit(entries: &[GameEntry], from: usize, to: usize) -> Vec<GameEntry> {
    let mut units = partition_into_units(entries);
    if from >= units.len() || to >= units.len() || from == to {
        return entries.to_vec();
    }
    let unit = units.remove(from);
    units.insert(to, unit);
    flatten_units(units)
}

/// Moves an entry within its group; invalid or no-op moves return the entries unchanged.
pub fn move_child_entry(
    entries: &[GameEntry],
    group_name: &str,
    from: usize,
    to: usize,
) -> Vec<GameEntry> {
    let mut units = partition_into_units(entries);
    let Some(SidebarUnit::Group {
        entries: siblings, ..
    }) = units
        .iter_mut()
        .find(|u| matches!(u, SidebarUnit::Group { name, .. } if same_group(name, group_name)))
    else {
        return entries.to_vec();
    };
    if from >= siblings.len() || to >= siblings.len() || from == to {
        return entries.to_vec();
    }
    let moved = siblings.remove(from);
    siblings.insert(to, moved);
    flatten_units(units)
}

fn is_collapsed(collapsed: &BTreeSet<String>, group: &str) -> bool {
    collapsed.iter().any(|c| same_group(c, group))
}

/// Computes visible rows; folder headers stay visible when collapsed.
pub fn compute_sidebar_rows(
    entries: &[GameEntry],
    collapsed_groups: &BTreeSet<String>,
    selected: Option<&GameEntry>,
) -> Vec<SidebarRow> {
    let units = partition_into_units(entries);
    let total_units = units.len();
    let mut rows = Vec::new();
    for (unit_index, unit) in units.into_iter().enumerate() {
        match unit {
            SidebarUnit::Standalone(entry) => rows.push(SidebarRow::Entry {
                entry: *entry,
                is_nested: false,
                group_name: None,
                sibling_index: 0,
                total_siblings: 1,
                unit_index,
                total_units,
            }),
            SidebarUnit::Group { name, entries } => {
                let collapsed = is_collapsed(collapsed_groups, &name);
                let has_selected_child = selected
                    .is_some_and(|s| entries.iter().any(|e| e.common().name == s.common().name));
                rows.push(SidebarRow::FolderHeader {
                    group_name: name.clone(),
                    count: entries.len(),
                    is_collapsed: collapsed,
                    has_selected_child,
                    first_child: entries.first().cloned(),
                    unit_index,
                    total_units,
                });
                if !collapsed {
                    let total_siblings = entries.len();
                    rows.extend(
                        entries
                            .into_iter()
                            .enumerate()
                            .map(|(sibling_index, entry)| SidebarRow::Entry {
                                entry,
                                is_nested: true,
                                group_name: Some(name.clone()),
                                sibling_index,
                                total_siblings,
                                unit_index,
                                total_units,
                            }),
                    );
                }
            }
        }
    }
    rows
}

/// Moves an entry row one step up (`-1`) or down (`1`) within its scope.
pub fn move_sidebar_entry(
    entries: &[GameEntry],
    row: &SidebarRow,
    direction: isize,
) -> Vec<GameEntry> {
    let SidebarRow::Entry {
        is_nested,
        group_name,
        sibling_index,
        unit_index,
        ..
    } = row
    else {
        return entries.to_vec();
    };
    match (is_nested, group_name) {
        (true, Some(group)) => match sibling_index.checked_add_signed(direction) {
            Some(to) => move_child_entry(entries, group, *sibling_index, to),
            None => entries.to_vec(),
        },
        _ => match unit_index.checked_add_signed(direction) {
            Some(to) => move_top_level_unit(entries, *unit_index, to),
            None => entries.to_vec(),
        },
    }
}

/// Collapse and drag state owned by the sidebar, independent of row rendering.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SidebarInteraction {
    dragging_key: Option<String>,
    drag_offset_y: f32,
    collapsed_groups: BTreeSet<String>,
}

impl SidebarInteraction {
    pub fn dragging_key(&self) -> Option<&str> {
        self.dragging_key.as_deref()
    }

    pub fn drag_offset_y(&self) -> f32 {
        self.drag_offset_y
    }

    pub fn collapsed_groups(&self) -> &BTreeSet<String> {
        &self.collapsed_groups
    }

    pub fn start_drag(&mut self, key: String) {
        self.dragging_key = Some(key);
        self.drag_offset_y = 0.0;
    }

    pub fn end_drag(&mut self) {
        self.dragging_key = None;
        self.drag_offset_y = 0.0;
    }

    pub fn toggle_group(&mut self, group_name: &str, currently_collapsed: bool) {
        if currently_collapsed {
            self.expand_group(group_name);
        } else {
            self.collapsed_groups.insert(group_name.to_string());
        }
    }

    /// Clicking a folder: expand it if collapsed; select its first child unless it already
    /// holds the selection, in which case an expanded folder collapses instead.
    /// Returns the entry to select, if any.
    pub fn activate_folder(&mut self, row: &SidebarRow) -> Option<GameEntry> {
        let SidebarRow::FolderHeader {
            group_name,
            is_collapsed,
            has_selected_child,
            first_child,
            ..
        } = row
        else {
            return None;
        };
        if *is_collapsed {
            self.expand_group(group_name);
        }
        if !has_selected_child {
            return first_child.clone();
        }
        if !is_collapsed {
            self.collapsed_groups.insert(group_name.clone());
        }
        None
    }

    /// Accumulates a folder drag; returns the new order once a whole row is crossed.
    pub fn drag_folder(
        &mut self,
        entries: &[GameEntry],
        row: &SidebarRow,
        delta: f32,
        item_height: f32,
    ) -> Option<Vec<GameEntry>> {
        let SidebarRow::FolderHeader { group_name, .. } = row else {
            return None;
        };
        self.apply_drag(&row.key(), delta, item_height, |steps| {
            let units = partition_into_units(entries);
            let current = units.iter().position(
                |u| matches!(u, SidebarUnit::Group { name, .. } if same_group(name, group_name)),
            )?;
            clamped_move(current, units.len() - 1, steps, |target| {
                move_top_level_unit(entries, current, target)
            })
        })
    }

    /// Accumulates an entry drag; nested entries stay within their group.
    pub fn drag_entry(
        &mut self,
        entries: &[GameEntry],
        row: &SidebarRow,
        delta: f32,
        item_height: f32,
    ) -> Option<Vec<GameEntry>> {
        let SidebarRow::Entry {
            entry,
            is_nested,
            group_name,
            ..
        } = row
        else {
            return None;
        };
        let name = &entry.common().name;
        self.apply_drag(&row.key(), delta, item_height, |steps| {
            let units = partition_into_units(entries);
            match (is_nested, group_name) {
                (true, Some(group)) => {
                    let siblings = units.iter().find_map(|u| match u {
                        SidebarUnit::Group { name, entries } if same_group(name, group) => {
                            Some((name, entries))
                        }
                        _ => None,
                    });
                    let (actual_group, siblings) = siblings?;
                    let current = siblings.iter().position(|e| &e.common().name == name)?;
                    clamped_move(current, siblings.len() - 1, steps, |target| {
                        move_child_entry(entries, actual_group, current, target)
                    })
                }
                _ => {
                    let current = units.iter().position(
                        |u| matches!(u, SidebarUnit::Standalone(e) if &e.common().name == name),
                    )?;
                    clamped_move(current, units.len() - 1, steps, |target| {
                        move_top_level_unit(entries, current, target)
                    })
                }
            }
        })
    }

    fn apply_drag(
        &mut self,
        key: &str,
        delta: f32,
        item_height: f32,
        compute_move: impl FnOnce(isize) -> Option<(Vec<GameEntry>, isize)>,
    ) -> Option<Vec<GameEntry>> {
        self.drag_offset_y += delta;
        if self.dragging_key.as_deref() != Some(key) || item_height <= 0.0 {
            return None;
        }
        // Truncation toward zero, as Kotlin's `Float.toInt()`.
        let steps = (self.drag_offset_y / item_height) as isize;
        let (entries, moved) = compute_move(steps)?;
        self.drag_offset_y -= moved as f32 * item_height;
        Some(entries)
    }

    fn expand_group(&mut self, name: &str) {
        self.collapsed_groups.retain(|c| !same_group(c, name));
    }
}

fn clamped_move(
    current: usize,
    last_index: usize,
    steps: isize,
    reorder: impl FnOnce(usize) -> Vec<GameEntry>,
) -> Option<(Vec<GameEntry>, isize)> {
    let target = current.saturating_add_signed(steps).min(last_index);
    (target != current).then(|| (reorder(target), target as isize - current as isize))
}
