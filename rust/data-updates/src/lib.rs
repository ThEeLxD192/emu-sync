pub mod downloader;
pub mod error;
pub mod github;
pub mod source;

pub use downloader::UpdateDownloader;
pub use error::UpdateError;
pub use github::{GitHubAsset, GitHubRelease, find_best_asset_for_platform, is_newer_version};
pub use source::GitHubUpdateSource;
