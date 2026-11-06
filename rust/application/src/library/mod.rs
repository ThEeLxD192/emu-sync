//! Library operations and registration.

pub mod edit_library;
pub mod entry_edits;
pub mod entry_sync_checks;
pub mod load_library;
pub mod steam_registration;

pub use edit_library::{EditLibrary, EntryEditResult};
pub use entry_edits::{GameOverrideChanges, build_game_entry, preserve_shared_cloud_root};
pub use entry_sync_checks::{EntrySyncChecks, SyncCheckRequest};
pub use load_library::LoadLibrary;
pub use steam_registration::{SteamRegistration, SteamRegistrationError};
