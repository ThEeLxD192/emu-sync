//! Integration tests of `emusync-data-local`, compiled as a single test binary.
//!
//! Each file in `tests/` used to be its own binary, compiled and linked separately. Add new
//! test files here as modules instead of next to this directory. Modules that only exist on
//! one platform declare it with an inner `#![cfg(...)]`.

mod config;
mod credential_transaction;
mod history;
mod location;
mod location_writability;
mod path_identity;
mod pinned_path;
mod pinned_path_windows;
mod recovery_store;
mod restore_paths;
mod save_access;
mod staging;
mod targets;
mod tracked;
mod tracked_upload_recovery;
