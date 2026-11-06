//! Google Drive adapter. Further operations are added as their contracts are migrated.
mod conditional;
pub use conditional::{ConditionalError, ConditionalUpdate, FileRevision, UploadedFile};

mod oauth;
pub use oauth::{OAuth, OAuthError, REDIRECT_URI};

mod loopback;
pub use loopback::loopback_code;

mod listing;
pub use listing::{DriveListing, ListingError};

mod folders;
pub use folders::{DriveFolders, FolderError};

mod download;
pub use download::{DownloadError, DownloadOptions, DriveDownload};

mod resumable;
pub use resumable::{CreatedFile, ReconcileStatus, ResumableUpload, UploadError, UploadSession};

mod interactive;
pub use interactive::InteractiveOAuth;

/// Observable progress of a single transfer. Total is absent when the server omits it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferProgress {
    pub name: String,
    pub transferred: u64,
    pub total: Option<u64>,
}

mod transfer;
pub use transfer::{DriveTransfer, TransferError};

mod http;
pub use http::{DriveHttpClients, DriveHttpPolicy, ResponseMetadataError};
