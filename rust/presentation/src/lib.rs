//! EmuSync native presentation layer implemented with egui.

pub mod actions;
pub mod app;
pub mod components;
pub mod cover_cache;
pub mod dialogs;
pub mod forms;
pub mod model;
pub mod state;
pub mod theme;

pub use actions::UiAction;
pub use app::{ActiveDialog, EmuSyncApp};
pub use cover_cache::CoverCache;
pub use forms::{
    EntryFormState, GameEditFormState, GameEditResult, SaveTargetFormState, SaveTargetRow,
};
pub use model::{
    AppStatus, CloudSyncStatus, EntryType, GameItem, PathTarget, PickMode, SaveSetupRequest,
};
pub use state::AppUiState;
pub use theme::{
    BACKGROUND, CARD_HOVER, DIVIDER, ERROR, ON_BACKGROUND, ON_SURFACE, ON_SURFACE_DIM, PRIMARY,
    PRIMARY_CONTAINER, SECONDARY, SECONDARY_CONTAINER, SUCCESS, SURFACE, SURFACE_SELECTED,
    SURFACE_VARIANT, WARNING, configure_egui_theme,
};
