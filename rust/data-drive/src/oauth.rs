use emusync_domain::config::{AppConfig, CredentialSnapshot, CredentialStore, Credentials};
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use std::{fmt, future::Future, time::Duration};

pub const REDIRECT_URI: &str = "http://localhost:8089";

#[derive(Debug)]
pub enum OAuthError {
    MissingCredentials,
    ConnectionChanged,
    AuthenticationRequired,
    TimedOut,
    Provider(String),
    Transport(reqwest::Error),
    Store(String),
}
impl fmt::Display for OAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCredentials => write!(f, "missing Google Drive credentials"),
            Self::ConnectionChanged => write!(f, "Google Drive connection changed"),
            Self::AuthenticationRequired => write!(f, "Google Drive authentication required"),
            Self::TimedOut => write!(f, "authorization timed out"),
            Self::Provider(message) => write!(f, "authorization failed: {message}"),
            Self::Transport(error) => write!(f, "authorization transport failed: {error}"),
            Self::Store(error) => write!(f, "credential storage failed: {error}"),
        }
    }
}
impl std::error::Error for OAuthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            Self::MissingCredentials
            | Self::ConnectionChanged
            | Self::AuthenticationRequired
            | Self::TimedOut
            | Self::Provider(_)
            | Self::Store(_) => None,
        }
    }
}
impl From<reqwest::Error> for OAuthError {
    fn from(error: reqwest::Error) -> Self {
        Self::Transport(error)
    }
}
#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
}
#[derive(Deserialize)]
struct ProviderError {
    error: Option<String>,
}

/// OAuth token exchange and revision-checked persistence; browser interaction is injected.
#[derive(Clone)]
pub struct OAuth<S> {
    client: Client,
    store: S,
    token_url: Url,
    redirect_uri: String,
}
impl<S> OAuth<S>
where
    S: CredentialStore,
    S::Error: fmt::Display,
{
    pub fn new(client: Client, store: S, token_url: Url) -> Self {
        Self {
            client,
            store,
            token_url,
            redirect_uri: REDIRECT_URI.into(),
        }
    }
    pub fn with_redirect_uri(mut self, uri: String) -> Self {
        self.redirect_uri = uri;
        self
    }
    fn snapshot(&self, config: &AppConfig) -> Result<CredentialSnapshot, OAuthError> {
        self.store
            .snapshot(config)
            .map_err(|e| OAuthError::Store(e.to_string()))
    }
    fn current(&self, config: &AppConfig, expected: &CredentialSnapshot) -> Result<(), OAuthError> {
        if self.snapshot(config)? == *expected {
            Ok(())
        } else {
            Err(OAuthError::ConnectionChanged)
        }
    }
    fn persist(
        &self,
        config: &AppConfig,
        expected: &CredentialSnapshot,
        credentials: Credentials,
    ) -> Result<(), OAuthError> {
        if self
            .store
            .compare_and_set(expected, Some(credentials), config)
            .map_err(|e| OAuthError::Store(e.to_string()))?
        {
            Ok(())
        } else {
            Err(OAuthError::ConnectionChanged)
        }
    }
    async fn request(
        &self,
        fields: &[(&str, &str)],
    ) -> Result<Result<Tokens, Option<String>>, OAuthError> {
        let response = self
            .client
            .post(self.token_url.clone())
            .form(fields)
            .send()
            .await?;
        if response.status() == StatusCode::OK {
            let tokens: Tokens = crate::http::read_metadata(response, Duration::from_secs(60))
                .await
                .map_err(|e| match e {
                    crate::ResponseMetadataError::Transport(err) => OAuthError::Transport(err),
                    other => OAuthError::Provider(other.to_string()),
                })?;
            return Ok(Ok(tokens));
        }
        let code = crate::http::read_metadata::<ProviderError>(response, Duration::from_secs(60))
            .await
            .ok()
            .and_then(|e| e.error);
        Ok(Err(code))
    }
    /// Only calls `interactive` when refresh is unavailable and interaction is permitted.
    /// The caller owns the loopback listener and must ensure it closes on cancellation.
    pub async fn authorize<F, Fut>(
        &self,
        config: &AppConfig,
        allow_interactive: bool,
        interactive: F,
    ) -> Result<String, OAuthError>
    where
        F: FnOnce(&str) -> Fut,
        Fut: Future<Output = Result<String, OAuthError>>,
    {
        self.authorize_scoped(config, allow_interactive, interactive)
            .await
            .map(|(token, _)| token)
    }
    /// Return the exact connection revision associated with the token.
    pub async fn authorize_scoped<F, Fut>(
        &self,
        config: &AppConfig,
        allow_interactive: bool,
        interactive: F,
    ) -> Result<(String, CredentialSnapshot), OAuthError>
    where
        F: FnOnce(&str) -> Fut,
        Fut: Future<Output = Result<String, OAuthError>>,
    {
        let mut credentials = config
            .credentials
            .clone()
            .filter(|c| !c.client_id.trim().is_empty() && !c.client_secret.trim().is_empty())
            .ok_or(OAuthError::MissingCredentials)?;
        let mut expected = self.snapshot(config)?;
        if expected.credentials.as_ref() != Some(&credentials) {
            return Err(OAuthError::ConnectionChanged);
        }
        if let Some(refresh) = credentials
            .refresh_token
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            let result = self
                .request(&[
                    ("client_id", &credentials.client_id),
                    ("client_secret", &credentials.client_secret),
                    ("refresh_token", refresh),
                    ("grant_type", "refresh_token"),
                ])
                .await;
            self.current(config, &expected)?;
            match result? {
                Ok(tokens) => {
                    if tokens.access_token.trim().is_empty() {
                        return Err(OAuthError::Provider("missing access token".into()));
                    }
                    return Ok((tokens.access_token, expected));
                }
                Err(Some(code)) if code == "invalid_grant" => {
                    credentials.refresh_token = None;
                    self.persist(config, &expected, credentials.clone())?;
                    expected.credentials = Some(credentials.clone());
                    expected.revision = expected.revision.wrapping_add(1);
                    self.current(config, &expected)?;
                }
                Err(code) => {
                    return Err(OAuthError::Provider(
                        code.unwrap_or_else(|| "refresh failed".into()),
                    ));
                }
            }
        }
        if !allow_interactive {
            return Err(OAuthError::AuthenticationRequired);
        }
        self.current(config, &expected)?;
        let code =
            tokio::time::timeout(Duration::from_secs(60), interactive(&credentials.client_id))
                .await
                .map_err(|_| OAuthError::TimedOut)??;
        self.current(config, &expected)?;
        let tokens = self
            .request(&[
                ("client_id", &credentials.client_id),
                ("client_secret", &credentials.client_secret),
                ("code", &code),
                ("redirect_uri", &self.redirect_uri),
                ("grant_type", "authorization_code"),
            ])
            .await?
            .map_err(|e| OAuthError::Provider(e.unwrap_or_else(|| "exchange failed".into())))?;
        if tokens.access_token.trim().is_empty() {
            return Err(OAuthError::Provider("missing access token".into()));
        }
        credentials.refresh_token = tokens.refresh_token.filter(|s| !s.trim().is_empty());
        self.persist(config, &expected, credentials.clone())?;
        if expected.credentials.as_ref() != Some(&credentials) {
            expected.credentials = Some(credentials);
            expected.revision = expected.revision.wrapping_add(1);
        }
        self.current(config, &expected)?;
        Ok((tokens.access_token, expected))
    }
}
