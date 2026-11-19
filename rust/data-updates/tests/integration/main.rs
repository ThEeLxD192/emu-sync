//! Integration tests of `emusync-data-updates`, compiled as a single test binary.
//!
//! Each file in `tests/` used to be its own binary, so every one recompiled `common` and was
//! linked separately. Add new test files here as modules instead of next to this directory.
//! `tests/support/` is not part of it: it holds the unit tests of `src/downloader.rs`, which
//! includes them with `#[path]`.

mod common;

mod check_failure_tests;
mod download_lifecycle_tests;
mod download_safety_tests;
mod download_timeout_tests;
mod github_rules_tests;
mod github_source_tests;
mod release_client_tests;
mod release_tls_redirect_tests;
