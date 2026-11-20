//! Integration tests of `emusync-desktop-app`, compiled as a single test binary.
//!
//! Each file in `tests/` used to be its own binary, so every one recompiled `common` and was
//! linked separately against the whole application. Add new test files here as modules
//! instead of next to this directory.

mod common;

mod coordinator_close_tests;
mod coordinator_composition_tests;
mod coordinator_sync_tests;
mod desktop_composition_tests;
mod drive_recovery_conditional_tests;
mod drive_recovery_resumable_tests;
mod event_sender_tests;
mod game_edit_tests;
mod guarded_files;
mod interaction_regression_tests;
mod launch_target_tests;
mod library_regression_tests;
mod migration_regression_tests;
mod persisted_recovery;
mod platform_ui_tests;
mod proxy_policy_tests;
mod recovery_regression_tests;
mod retained_files;
mod runtime_owner;
mod save_setup_tests;
mod supervised_drive;
mod supervisor;
mod target_types_tests;
mod update_progress_tests;
mod update_verification_tests;
mod update_wiring_tests;
