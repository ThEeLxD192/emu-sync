//! Integration tests of `emusync-domain`, compiled as a single test binary.
//!
//! Each file in `tests/` used to be its own binary, compiled and linked separately. Add new
//! test files here as modules instead of next to this directory. Fixture files stay in
//! `tests/fixtures/`.

mod execution_tests;
mod library_tests;
mod operation_tests;
mod shared_fixtures;
mod sync_cloud_path_tests;
mod sync_decision_tests;
mod sync_events_tests;
mod sync_ports_tests;
mod sync_remote_tests;
