use std::fmt;

#[derive(Debug)]
pub enum UpdateError {
    Http(String),
    Network(String),
    /// The server answered, but not with a release this updater can read.
    InvalidRelease(String),
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
            Self::Http(msg) => write!(f, "HTTP error: {msg}"),
            Self::Network(msg) => write!(f, "Network error: {msg}"),
            Self::InvalidRelease(msg) => write!(f, "Invalid release information: {msg}"),
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
