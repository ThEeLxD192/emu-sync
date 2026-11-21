//! Integration tests of `emusync-presentation`, compiled as a single test binary.
//!
//! Each file in `tests/` used to be its own binary, so every one recompiled `common` and was
//! linked separately against egui. Add new test files here as modules instead of next to
//! this directory.

mod common;

mod app_state_tests;
mod async_results_tests;
mod confirm_delete_tests;
mod cover_cache_tests;
mod dialog_lifecycle_tests;
mod drive_setup_tests;
mod entry_form_tests;
mod game_edit_form_tests;
mod game_grid_scroll_tests;
mod game_item_tests;
mod language_tests;
mod main_view_tests;
mod save_targets_tests;
mod sidebar_drag_edges_tests;
mod sidebar_order_tests;
mod status_overlay_tests;
mod sync_text_tests;
mod update_dialog_tests;
