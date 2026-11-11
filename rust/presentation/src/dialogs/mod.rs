//! Modal dialogs and overlay views.

pub mod confirm_delete;
pub mod drive_setup;
pub mod entry_dialog;
pub(crate) mod form_fields;
pub mod game_edit_dialog;
pub mod status_overlay;
pub mod update_dialog;

pub use confirm_delete::{ConfirmDeleteOutcome, render_confirm_delete_dialog};
pub use drive_setup::{DriveSetupDialogState, render_drive_setup_dialog};
pub use entry_dialog::{EntryDialogEvents, render_entry_dialog};
pub use game_edit_dialog::{GameEditDialogEvents, render_game_edit_dialog};
pub use status_overlay::render_status_overlay;
pub use update_dialog::render_update_dialog;
