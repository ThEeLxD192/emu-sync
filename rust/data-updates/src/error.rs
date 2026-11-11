use std::fmt;

#[derive(Debug)]
pub enum UpdateError {
    Http(String),
    Network(String),
    Io(std::io::Error),
    EmptyDownload,
    IncompleteDownload { expected: u64, received: u64 },
    Callback(String),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(msg) => write!(f, "HTTP error: {msg}"),
            Self::Network(msg) => write!(f, "Network error: {msg}"),
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
    fn from(err: reqwest::Error) -> Self {
        Self::Network(err.to_string())
    }
}
