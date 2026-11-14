//! Modal dialogs and overlay views.

pub mod confirm_delete;
pub mod drive_setup;
pub mod entry_dialog;
pub(crate) mod form_fields;
pub mod game_edit_dialog;
pub mod status_overlay;
pub mod update_dialog;

pub use confirm_delete::{ConfirmDeleteOutcome, render_confirm_delete_dialog};
pub use drive_setup::{
    DriveSetupDialogState, DriveSetupError, DriveSetupEvents, render_drive_setup_dialog,
};
pub use entry_dialog::{EntryDialogEvents, render_entry_dialog};
pub use game_edit_dialog::{GameEditDialogEvents, render_game_edit_dialog};
pub use status_overlay::{
    SaveFolderSelection, StatusOverlayEvents, conflict_card_id, post_sync_failed_message,
    render_status_overlay, save_folder_option_id,
};
pub use update_dialog::{UpdateDialogEvents, render_update_dialog, update_dialog_dismissible};
