//! Cloud connection management and credential updating.

use emusync_domain::config::{AppConfig, CloudAuthorization, ConfigurationStore, Credentials};
use std::fmt::Display;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionResult {
    pub config: Option<AppConfig>,
    pub message: String,
    pub authorized: bool,
}

pub struct CloudConnection<S: ConfigurationStore, A: CloudAuthorization> {
    store: S,
    authorization: A,
}

impl<S: ConfigurationStore, A: CloudAuthorization> CloudConnection<S, A> {
    pub fn new(store: S, authorization: A) -> Self {
        Self {
            store,
            authorization,
        }
    }

    pub fn setup(
        &self,
        config: Option<&AppConfig>,
        client_id: &str,
        client_secret: &str,
    ) -> Result<ConnectionResult, S::Error> {
        if config.is_none() {
            return Ok(ConnectionResult {
                config: None,
                message: "No config loaded".into(),
                authorized: false,
            });
        }
        let id = client_id.trim();
        let secret = client_secret.trim();
        if id.is_empty() || secret.is_empty() {
            return Ok(ConnectionResult {
                config: None,
                message: "Client ID and Client Secret cannot be blank.".into(),
                authorized: false,
            });
        }
        let updated = self.store.update_credentials(|current| match current {
            Some(c) if c.client_id == id && c.client_secret == secret => Some(c),
            _ => Some(Credentials {
                client_id: id.to_owned(),
                client_secret: secret.to_owned(),
                refresh_token: None,
            }),
        })?;
        Ok(ConnectionResult {
            config: Some(updated),
            message: "Google Drive credentials saved.".into(),
            authorized: false,
        })
    }

    pub fn unlink(&self, config: Option<&AppConfig>) -> Result<ConnectionResult, S::Error> {
        let Some(cfg) = config else {
            return Ok(ConnectionResult {
                config: None,
                message: "No config loaded".into(),
                authorized: false,
            });
        };
        if cfg.credentials.is_none() {
            return Ok(ConnectionResult {
                config: None,
                message: "Google Drive is not configured.".into(),
                authorized: false,
            });
        }
        let updated = self.store.update_credentials(|current| {
            current.map(|c| Credentials {
                refresh_token: None,
                ..c
            })
        })?;
        Ok(ConnectionResult {
            config: Some(updated),
            message: "Google Drive disconnected.".into(),
            authorized: false,
        })
    }

    pub async fn login(&self, config: Option<&AppConfig>) -> ConnectionResult
    where
        A::Error: Display,
    {
        let Some(cfg) = config else {
            return ConnectionResult {
                config: None,
                message: "No config loaded".into(),
                authorized: false,
            };
        };
        if cfg.credentials.is_none() {
            return ConnectionResult {
                config: None,
                message: "Missing googleDrive configuration in config.json".into(),
                authorized: false,
            };
        }
        let (authorized, message) = match self.authorization.authorize(cfg, true).await {
            Ok(_) => (true, "Google Drive successfully connected!".into()),
            Err(err) => (false, format!("Google Drive login failed: {err}")),
        };

        ConnectionResult {
            config: self.store.load().ok().or_else(|| Some(cfg.clone())),
            message,
            authorized,
        }
    }

    pub fn load_stored_connection(&self) -> Result<AppConfig, S::Error> {
        self.store.load()
    }
}
