//! Integration tests of `emusync-platform-desktop`, compiled as a single test binary.
//!
//! Each file in `tests/` used to be its own binary, so every one recompiled `common` and was
//! linked separately. Add new test files here as modules instead of next to this directory.
//! Modules that only exist on one platform declare it with an inner `#![cfg(...)]`.

mod common;

mod appimage_environment_tests;
mod appimage_installer_tests;
mod binary_vdf_tests;
mod cover_lookup_tests;
mod desktop_update_installer_tests;
mod game_runner_tests;
mod gamescope_session_tests;
mod picker_tests;
mod process_monitor_tests;
mod process_tracker_tests;
mod rom_scanner_tests;
mod shell_argument_tests;
mod steam_launcher_scripts_tests;
mod steam_library_tests;
mod steam_migration_tests;
mod steam_paths_tests;
mod steam_registration_tests;
mod update_staging_tests;
