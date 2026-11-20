//! Integration tests of `emusync-application`, compiled as a single test binary.
//!
//! Each file in `tests/` used to be its own binary, so every one recompiled `common` and the
//! generic sync code it instantiates, and was linked separately. Add new test files here as
//! modules instead of next to this directory.

mod common;

mod channel_tests;
mod connection_tests;
mod context_tests;
mod edit_library_tests;
mod entry_sync_checks_tests;
mod excluded_folders_tests;
mod legacy_placement_tests;
mod lifecycle_tests;
mod load_library_tests;
mod manual_sync_tests;
mod migration_tests;
mod migrator_tests;
mod operation_tests;
mod orchestrator_tests;
mod post_game_tests;
mod status_tests;
mod steam_registration_tests;
mod target_mapping_tests;
mod target_resolver_tests;
mod update_coordinator_tests;
mod update_progress_tests;
mod update_revision_tests;
