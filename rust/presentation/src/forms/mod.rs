//! Presentation form states.

pub mod entry_form;
pub mod error;
pub mod game_edit_form;
pub mod save_targets;

pub use entry_form::EntryFormState;
pub use error::FormError;
pub use game_edit_form::{GameEditFormState, GameEditResult, RestoreChoice};
pub use save_targets::{SaveTargetFormState, SaveTargetRow};
