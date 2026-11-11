//! Save targets that belong to one game, without emulator-wide fallbacks
//! (Kotlin's `SaveTargetAccess.gameSpecificTargets`).

use emusync_domain::library::{GameEntry, SaveTarget, SaveTargetAccess};

/// For an emulator, only the targets configured for that ROM; for a native game, its own
/// targets. Editing a game must start from these, never from the emulator-wide targets.
pub fn game_specific_targets<A: SaveTargetAccess>(
    access: &A,
    entry: &GameEntry,
    rom_name: Option<&str>,
) -> Result<Vec<SaveTarget>, A::Error> {
    match entry {
        GameEntry::Emulator(emulator) => match rom_name {
            Some(rom) => access.for_rom(emulator, rom),
            None => Ok(Vec::new()),
        },
        GameEntry::Native(_) => access.for_entry(entry),
    }
}
