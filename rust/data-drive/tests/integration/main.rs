//! Integration tests of `emusync-data-drive`, compiled as a single test binary.
//!
//! Each file in `tests/` used to be its own binary, so every one recompiled `common` and was
//! linked separately, and the binaries ran one after another. Add new test files here as
//! modules instead of next to this directory.

mod common;

mod conditional;
mod conditional_cancel_grace;
mod conditional_timeouts;
mod download;
mod folders;
mod http_policy;
mod interactive;
mod listing;
mod loopback;
mod oauth;
mod resumable;
mod resumable_recovery;
mod resumable_retry_backoff;
mod tracked_integration;
mod transfer;
