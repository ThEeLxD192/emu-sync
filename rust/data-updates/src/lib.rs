pub mod downloader;
pub mod error;
pub mod github;
pub mod http;
pub mod signature;
pub mod source;

pub use downloader::UpdateDownloader;
pub use error::UpdateError;
pub use github::{GitHubAsset, GitHubRelease, find_best_asset_for_platform, is_newer_version};
pub use http::{ReleaseTransport, release_client, release_client_builder};
pub use signature::{MANIFEST_NAME, ReleaseKey, SIGNATURE_NAME, signed_sha256};
pub use source::GitHubUpdateSource;
