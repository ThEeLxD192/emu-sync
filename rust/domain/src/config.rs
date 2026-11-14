use crate::library::GameEntry;
use std::future::Future;

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: Option<String>,
}

/// Hand-written so the client secret and refresh token never reach logs or panics.
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppConfig {
    pub credentials: Option<Credentials>,
    pub entries: Vec<GameEntry>,
    /// The language the interface is shown in.
    pub language: Language,
}

/// A language the interface can be shown in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Language {
    #[default]
    English,
    Spanish,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::English, Language::Spanish];

    /// The code stored in the configuration (ISO 639-1).
    pub const fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Spanish => "es",
        }
    }

    /// The language a stored code names, if it is one the interface has.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.code().eq_ignore_ascii_case(code.trim()))
    }

    /// The language's own name, as shown in the language selector.
    pub const fn native_name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Spanish => "Español",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialSnapshot {
    pub credentials: Option<Credentials>,
    pub revision: i64,
}

pub trait ConfigurationStore {
    type Error;
    fn load(&self) -> Result<AppConfig, Self::Error>;
    /// Apply to the latest configuration atomically, preserving unrelated concurrent edits.
    fn update(
        &self,
        transform: impl FnOnce(AppConfig) -> AppConfig,
    ) -> Result<AppConfig, Self::Error>;
    fn update_credentials(
        &self,
        transform: impl FnOnce(Option<Credentials>) -> Option<Credentials>,
    ) -> Result<AppConfig, Self::Error>;
}
pub trait CredentialStore {
    type Error;
    fn snapshot(&self, fallback: &AppConfig) -> Result<CredentialSnapshot, Self::Error>;
    /// Both the credential value and connection revision must still match.
    /// A successful change advances the revision by one with wrapping arithmetic;
    /// a successful no-op leaves it unchanged.
    fn compare_and_set(
        &self,
        expected: &CredentialSnapshot,
        credentials: Option<Credentials>,
        fallback: &AppConfig,
    ) -> Result<bool, Self::Error>;
}
/// Execute a short synchronous action only while the connection revision is current.
/// Implementations serialize credential changes with the action and must not await inside it.
pub trait CredentialTransaction: CredentialStore {
    fn while_current<R>(
        &self,
        expected: &CredentialSnapshot,
        fallback: &AppConfig,
        action: impl FnOnce() -> R,
    ) -> Result<Option<R>, Self::Error>;
}
pub trait CloudAuthorization {
    type Error;
    fn authorize(
        &self,
        config: &AppConfig,
        allow_interactive: bool,
    ) -> impl Future<Output = Result<String, Self::Error>> + Send;
}
