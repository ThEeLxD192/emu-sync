//! Corrects save target kinds with the filesystem before they are persisted, as Kotlin's
//! `SaveTargetFormState.buildTargets` does: an existing path keeps its real kind, and
//! only missing or unreadable paths keep the kind chosen in the form.

use emusync_domain::library::{GameEntry, SaveTarget, SaveTargetAccess};

pub fn resolve_target_kinds(
    targets: Vec<SaveTarget>,
    access: &impl SaveTargetAccess,
) -> Vec<SaveTarget> {
    targets
        .into_iter()
        .map(|mut target| {
            if let Ok(Some(kind)) = access.type_of(target.path.trim()) {
                target.kind = kind;
            }
            target
        })
        .collect()
}

pub fn resolve_entry_target_kinds(
    mut entry: GameEntry,
    access: &impl SaveTargetAccess,
) -> GameEntry {
    let common = match &mut entry {
        GameEntry::Emulator(emulator) => &mut emulator.common,
        GameEntry::Native(native) => &mut native.common,
    };
    common.save_targets = resolve_target_kinds(std::mem::take(&mut common.save_targets), access);
    entry
}
