use std::fmt;

#[derive(Debug)]
pub enum UpdateError {
    /// The server answered with a status other than the expected one.
    Http(reqwest::StatusCode),
    Network(String),
    /// The server answered, but not with a release this updater can read.
    InvalidRelease(String),
    /// The release could not be proven to come from EmuSync: its signature, manifest or
    /// downloaded bytes do not match.
    Verification(String),
    Io(std::io::Error),
    EmptyDownload,
    IncompleteDownload {
        expected: u64,
        received: u64,
    },
    Callback(String),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(status) => write!(f, "HTTP error: {status}"),
            Self::Network(msg) => write!(f, "Network error: {msg}"),
            Self::InvalidRelease(msg) => write!(f, "Invalid release information: {msg}"),
            Self::Verification(msg) => write!(f, "Update verification failed: {msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::EmptyDownload => write!(f, "The downloaded update is empty"),
            Self::IncompleteDownload { expected, received } => write!(
                f,
                "The downloaded update is incomplete: expected {expected} bytes, received {received}"
            ),
            Self::Callback(msg) => write!(f, "Progress callback error: {msg}"),
        }
    }
}

impl std::error::Error for UpdateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for UpdateError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<reqwest::Error> for UpdateError {
    /// Keeps the causes, which `reqwest` leaves out of its own message (for example the
    /// timeout or refused connection behind "error sending request").
    fn from(err: reqwest::Error) -> Self {
        Self::Network(with_causes(&err))
    }
}

/// The error followed by each of its causes.
fn with_causes(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(current) = cause {
        message.push_str(": ");
        message.push_str(&current.to_string());
        cause = current.source();
    }
    message
}
